import assert from "node:assert/strict";
import test from "node:test";
import { dropHint } from "../src/dropSurface.ts";
import {
  resourceDropTarget,
  routeResourceDrop,
  resourceDropIssue,
} from "../src/resources.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("drop routing uses inspected content, including ZIP worlds and ambiguous resource packs", () => {
  assert.equal(
    routeResourceDrop([{ format: "resource", kinds: ["saves"] }]),
    "resources",
  );
  assert.equal(
    routeResourceDrop([
      { format: "resource", kinds: ["datapacks", "resourcepacks"] },
    ]),
    "resources",
  );
  assert.equal(routeResourceDrop([{ format: "modpack" }]), "modpack");
  assert.equal(
    routeResourceDrop([{ format: "directory" }, { format: "directory" }]),
    "folders",
  );
  assert.equal(
    routeResourceDrop([{ format: "resource" }, { format: "directory" }]),
    "mixed",
  );
});

test("a version row overrides the selected-version fallback without accepting stale IDs", () => {
  const instance = { id: "games", versions: [{ id: "a" }, { id: "b" }] };
  const element = {
    closest: () => ({
      dataset: { resourceInstance: "games", resourceVersion: "b" },
    }),
  };
  assert.equal(
    resourceDropTarget(element, [instance], { instance: "games", version: "a" })
      .version,
    "b",
  );
  assert.equal(
    resourceDropTarget(null, [instance], { instance: "games", version: "a" })
      .version,
    "a",
  );
  assert.equal(resourceDropTarget(element, []), undefined);
  assert.equal(
    resourceDropTarget(null, [instance], {
      instance: "games",
      version: "deleted",
    }),
    undefined,
  );
});

test("resource ZIP drop opens the selected profile's resource preview and does not import a modpack", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.inspect_resources = ({ paths }) =>
      paths.map((path) => ({
        path,
        name: "world.zip",
        kinds: ["saves"],
        format: "resource",
      }));
    f.fileDrop({ payload: { type: "drop", paths: ["/world.zip"] } });
    await flush();
    assert.equal(f.context.resourceSession.value.instance.id, "instance-a");
    assert.equal(f.context.resourceSession.value.version, "1.21.1");
    assert.equal(f.context.resourceSession.value.kind, "saves");
    assert.equal(
      f.calls.some((c) =>
        ["scan_folders", "preview_modpack", "import_folders"].includes(c.op),
      ),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("resource installation has independent progress and supports pause without marking Minecraft installed", async () => {
  const f = await settingsFixture();
  const completed = deferred();
  try {
    f.handlers.install_resources = () => completed.promise;
    f.handlers.pause = () => true;
    f.handlers.resume = () => true;
    const plan = {
      id: "resource-task",
      target: { instance: "instance-a", version: "1.21.1" },
      entries: [{ origin: { platform: "modrinth" } }],
    };
    const result = f.context.installResources(plan);
    await flush();
    assert.equal(f.context.current.value.resource_task, "resource-task");
    assert.equal(f.context.current.value.version_id, null);
    assert.equal(
      f.context.current.value.workflow.steps[0].id,
      "resource_check",
    );
    await f.context.setDownloadPaused(true);
    assert.equal(f.context.current.value.status, "paused");
    await f.context.setDownloadPaused(false);
    assert.equal(f.context.current.value.status, "running");
    completed.resolve(true);
    assert.equal(await result, true);
    assert.equal(f.context.instances.value[0].versions[0].installed, false);
    assert.equal(
      f.context.instances.value[0].versions[0].download_progress,
      undefined,
    );
  } finally {
    completed.resolve(true);
    f.dispose();
  }
});

test("invalid or mixed drops report a target-local error and never silently associate directories", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.inspect_resources = () => [
      { format: "unknown", issue: "invalid archive" },
    ];
    f.fileDrop({ payload: { type: "drop", paths: ["/broken.zip"] } });
    await flush();
    assert.equal(dropHint.value.message, "invalid archive");
    assert.equal(f.context.error.value, "");
    assert.equal(f.context.resourceSession.value, undefined);
    assert.equal(
      f.calls.some((c) => c.op === "scan_folders"),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("ZIP target constraints preserve inspected types and reject mismatches, including mixed batches", () => {
  const files = [
    { name: "textures.zip", format: "resource", kinds: ["resourcepacks"] },
    { name: "shader.zip", format: "resource", kinds: ["shaderpacks"] },
    { name: "world.zip", format: "resource", kinds: ["saves"] },
    { name: "pack.zip", format: "modpack", kinds: [] },
  ];
  const before = structuredClone(files);
  for (const file of files) {
    for (const kind of [
      "mods",
      "resourcepacks",
      "shaderpacks",
      "datapacks",
      "saves",
      "modpack",
    ]) {
      const matches =
        kind === "modpack"
          ? file.format === "modpack"
          : file.kinds.includes(kind);
      assert.equal(
        resourceDropIssue([file], kind) === undefined,
        matches,
        `${file.name} -> ${kind}`,
      );
    }
  }
  assert.ok(resourceDropIssue(files.slice(0, 2), "resourcepacks"));
  assert.equal(
    resourceDropIssue(
      [
        {
          name: "both.zip",
          format: "resource",
          kinds: ["datapacks", "resourcepacks"],
        },
      ],
      "datapacks",
    ),
    undefined,
  );
  assert.deepEqual(files, before);
});

function dropElement(dataset, parent) {
  return {
    dataset,
    getBoundingClientRect: () => ({
      left: 30,
      top: 60,
      width: 350,
      height: 260,
    }),
    closest(selector) {
      if (selector.includes("data-drop-kind") && this.dataset.dropKind)
        return this;
      if (
        selector.includes("data-resource-instance") &&
        this.dataset.resourceInstance
      )
        return this;
      return parent?.closest(selector) || null;
    },
  };
}

test("a ZIP dropped on the wrong resource tab stays there with a reason and never opens another importer", async () => {
  const f = await settingsFixture();
  try {
    f.fixture.hovered = dropElement(
      { dropKind: "resourcepacks" },
      dropElement({
        resourceInstance: "instance-a",
        resourceVersion: "1.21.1",
      }),
    );
    f.handlers.inspect_resources = () => [
      {
        name: "world.zip",
        path: "/world.zip",
        format: "resource",
        kinds: ["saves"],
        issue: null,
      },
    ];
    f.fileDrop({
      payload: {
        type: "enter",
        position: { x: 100, y: 100 },
        paths: ["/world.zip"],
      },
    });
    await flush();
    assert.equal(dropHint.value.invalid, true);
    for (let i = 0; i < 10; i++)
      f.fileDrop({ payload: { type: "over", position: { x: 105, y: 100 } } });
    f.fileDrop({
      payload: {
        type: "drop",
        position: { x: 100, y: 100 },
        paths: ["/world.zip"],
      },
    });
    await flush();
    assert.equal(
      f.calls.filter((call) => call.op === "inspect_resources").length,
      1,
    );
    assert.equal(
      f.calls.some((call) =>
        ["preview_modpack", "scan_folders", "supply_modpack_files"].includes(
          call.op,
        ),
      ),
      false,
    );
    assert.equal(f.context.resourceSession.value, undefined);
    assert.equal(f.context.error.value, "");
    assert.equal(dropHint.value.left, 30);
    assert.equal(dropHint.value.invalid, true);
    assert.match(dropHint.value.message, /world.zip/);
  } finally {
    f.dispose();
  }
});

test("a waiting manual install does not hijack drops on another version", async () => {
  const f = await settingsFixture();
  try {
    f.context.tasks.value = [
      {
        id: "waiting",
        status: "waiting",
        modpack_status: { missing: [{ key: "file" }] },
      },
    ];
    f.fixture.hovered = dropElement({
      resourceInstance: "instance-a",
      resourceVersion: "1.21.1",
      dropKind: "version",
    });
    f.handlers.inspect_resources = () => [
      {
        name: "cat.jar",
        path: "/cat.jar",
        format: "resource",
        kinds: ["mods"],
        issue: null,
      },
    ];
    f.fileDrop({
      payload: {
        type: "drop",
        position: { x: 100, y: 100 },
        paths: ["/cat.jar"],
      },
    });
    await flush();
    assert.equal(f.context.resourceSession.value.version, "1.21.1");
    assert.equal(f.context.utility.value, null);
    assert.equal(
      f.calls.some((call) => call.op === "supply_modpack_files"),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("an explicit stale instance target never inherits the current version", () => {
  const instances = [
    { id: "a", versions: [{ id: "selected" }] },
    { id: "b", versions: [{ id: "selected" }] },
  ];
  assert.equal(
    resourceDropTarget(dropElement({ resourceInstance: "b" }), instances, {
      instance: "a",
      version: "selected",
    }),
    undefined,
  );
});
