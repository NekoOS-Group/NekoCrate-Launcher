import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";
import { resourceDropIssue, routeResourceDrop } from "../src/resources.ts";
import { dropHint } from "../src/dropSurface.ts";

const archive = {
  path: "/Collection.zip",
  name: "Collection.zip",
  format: "instance_archive",
  kinds: [],
  issue: null,
  game_directories: [
    { prefix: "Pack A/.minecraft/", name: "Pack A" },
    { prefix: "Pack B/.minecraft/", name: "Pack B" },
  ],
};
const version = {
  path: "/Games/.minecraft/versions/Forge",
  name: "Forge",
  format: "version_directory",
  kinds: [],
  issue: null,
  version: "Forge",
  game_directory: "/Games/.minecraft",
};

test("game imports preserve content types and explicit drop constraints", () => {
  assert.equal(routeResourceDrop([archive]), "game");
  assert.equal(routeResourceDrop([version]), "game");
  assert.equal(
    routeResourceDrop([{ format: "versions_directory" }]),
    "folders",
  );
  assert.equal(resourceDropIssue([archive], "directory"), undefined);
  assert.ok(resourceDropIssue([archive], "instance"));
  assert.ok(resourceDropIssue([archive], "resourcepacks"));
  assert.ok(resourceDropIssue([version], "mods"));
  assert.equal(resourceDropIssue([version], "instance"), undefined);
  assert.equal(resourceDropIssue([version], "version"), undefined);
  assert.equal(routeResourceDrop([archive, { format: "resource" }]), "mixed");
});

test("full ZIP opens creation with name and location selection; nothing is imported before confirmation", async () => {
  const f = await settingsFixture();
  try {
    await f.context.handleResourceDrop(
      [archive.path],
      undefined,
      [archive],
      "directory",
    );
    const ui = f.context;
    assert.equal(ui.modal.value, "create");
    assert.equal(ui.creationTitle.value, "导入游戏目录");
    assert.equal(ui.form.name, "Pack A");
    assert.equal(ui.importTarget.value, null);
    assert.equal(ui.importArchiveOptions.value.length, 2);
    assert.equal(
      f.calls.some((c) =>
        ["import_game", "create", "scan_folders", "preview_modpack"].includes(
          c.op,
        ),
      ),
      false,
    );
    ui.chooseArchiveDirectory("Pack B/.minecraft/");
    assert.equal(ui.form.name, "Pack B");
    ui.form.name = "My Pack";
    ui.form.location = "external";
    assert.equal(ui.invalidCreationLocation.value, true);
    f.handlers.pick_instance_location = () => "/Games/New";
    await ui.chooseCreationDirectory();
    const pending = deferred();
    f.handlers.import_game = ({ request }) => {
      assert.equal(request.archive_prefix, "Pack B/.minecraft/");
      assert.equal(request.path, archive.path);
      assert.equal(request.directory, "/Games/New");
      assert.equal(request.name, "My Pack");
      assert.equal(request.target, null);
      return pending.promise;
    };
    const importing = ui.submit();
    await flush();
    assert.equal(ui.modal.value, null);
    assert.equal(ui.utility.value, "downloads");
    assert.equal(ui.current.value.cancellable, true);
    assert.deepEqual(
      ui.current.value.workflow.steps.map((s) => s.id),
      ["import_check", "import_files", "import_register"],
    );
    const imported = {
      ...f.state.database.instances[0],
      id: "imported",
      name: "My Pack",
    };
    f.state.database.instances.push(imported);
    pending.resolve(imported);
    await importing;
    assert.equal(ui.selectedInstanceId.value, "imported");
    ui.newInstance();
    assert.equal(ui.gameImport.value, undefined);
    assert.equal(ui.creationTitle.value, "新建实例");
  } finally {
    f.dispose();
  }
});

test("dropping an owned version on home locates the existing root and selects the requested version", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.instances[0].versions.push({
      id: "Forge",
      installed: true,
    });
    f.handlers.scan_folders = ({ paths }) => {
      assert.deepEqual(paths, [version.path]);
      return {
        candidates: [
          {
            path: version.game_directory,
            name: "World",
            versions: f.state.database.instances[0].versions,
            existing_id: "instance-a",
            blocked: null,
            preferred_version: "Forge",
          },
        ],
        issues: [],
        skipped: 0,
      };
    };
    f.handlers.select_version = ({ id, version }) => {
      assert.equal(id, "instance-a");
      assert.equal(version, "Forge");
      f.state.database.instances[0].selected_version = version;
      return true;
    };
    await f.context.handleResourceDrop([version.path], undefined, [version]);
    await flush();
    await flush();
    assert.equal(f.context.selectedInstanceId.value, "instance-a");
    assert.equal(f.context.selectedInstance.value.selected_version, "Forge");
    assert.equal(f.context.modal.value, null);
    assert.equal(
      f.calls.some((c) =>
        ["import_game", "import_folders", "create"].includes(c.op),
      ),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("version drops on an instance copy into that destination, while standalone versions allow new instance creation", async () => {
  const f = await settingsFixture();
  try {
    const ui = f.context,
      target = f.state.database.instances[0];
    await ui.handleResourceDrop(
      [version.path],
      undefined,
      [version],
      "instance",
      null,
      target,
    );
    assert.equal(ui.modal.value, "create");
    assert.equal(ui.importTarget.value.id, target.id);
    ui.form.location = "external";
    assert.equal(ui.invalidCreationLocation.value, false);
    f.handlers.import_game = ({ request }) => {
      assert.equal(request.target, target.id);
      assert.equal(request.archive_prefix, null);
      assert.equal(request.directory, null);
      return target;
    };
    await ui.submit();
    assert.equal(
      f.calls.some((c) => c.op === "scan_folders"),
      false,
    );
    await ui.handleResourceDrop(["/Forge"], undefined, [
      { ...version, path: "/Forge", game_directory: undefined },
    ]);
    assert.equal(ui.modal.value, "create");
    ui.importTargetId.value = null;
    assert.equal(ui.form.name, "Forge");
    f.handlers.import_game = ({ request }) => {
      assert.equal(request.target, null);
      assert.equal(request.name, "Forge");
      assert.equal(request.path, "/Forge");
      return target;
    };
    await ui.submit();
  } finally {
    f.dispose();
  }
});

test("a stale explicit version destination never falls back to associating the source", async () => {
  const f = await settingsFixture();
  try {
    await f.context.handleResourceDrop(
      [version.path],
      undefined,
      [version],
      "instance",
    );
    await flush();
    assert.equal(
      f.calls.some((c) => ["scan_folders", "import_game"].includes(c.op)),
      false,
    );
    assert.equal(f.context.modal.value, null);
    assert.match(dropHint.value.message, /目标实例/);
  } finally {
    f.dispose();
  }
});
