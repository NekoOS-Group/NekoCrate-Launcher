import assert from "node:assert/strict";
import test from "node:test";
import { filterVersions, versionCategory } from "../src/versionCatalog.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const entry = (id, type = "release") => ({ id, type, url: "", sha1: "" });
const catalog = {
  latest: { release: "26.1.1", snapshot: "26.2-snapshot-1" },
  versions: [
    entry("26.2-snapshot-1", "snapshot"),
    entry("26w14a", "snapshot"),
    entry("26.1.1"),
    entry("1.21.1"),
    entry("1.20.1"),
    entry("25w14craftmine", "snapshot"),
    entry("b1.7.3", "old_beta"),
    entry("a1.2.6", "old_alpha"),
  ],
};
async function downloadFixture() {
  const harness = await settingsFixture();
  const { fixture, context: ui } = harness;
  const snapshot = fixture.snapshot.bind(fixture);
  fixture.snapshot = () => ({
    ...snapshot(),
    manifest: structuredClone(catalog),
  });
  fixture.state.database.instances.push({
    ...fixture.state.database.instances[0],
    id: "instance-b",
    name: "官方启动器",
  });
  await ui.sync();
  ui.newVersion(fixture.state.database.instances[0]);
  return harness;
}

test("catalog categories separate April Fools from ordinary April releases, pre-releases and early development builds", () => {
  for (const id of [
    "15w14a",
    "1.RV-Pre1",
    "3D Shareware v1.34",
    "20w14infinite",
    "22w13oneblockatatime",
    "23w13a_or_b",
    "24w14potato",
    "25w14craftmine",
    "26w14a",
  ])
    assert.equal(versionCategory(entry(id, "snapshot")), "april");
  assert.equal(versionCategory(entry("26.1.1")), "release");
  assert.equal(versionCategory(entry("1.21-pre1", "snapshot")), "snapshot");
  assert.equal(versionCategory(entry("1.21-rc1", "snapshot")), "snapshot");
  assert.equal(versionCategory(entry("rd-132211", "old_alpha")), "development");
  assert.deepEqual(
    filterVersions(catalog.versions, "development", "").map((v) => v.id),
    ["b1.7.3", "a1.2.6"],
  );
  assert.deepEqual(
    filterVersions(catalog.versions, "snapshot", "").map((v) => v.id),
    ["26.2-snapshot-1"],
  );
  assert.deepEqual(
    filterVersions(catalog.versions, "april", "  CRAFT  MINE  ").map(
      (v) => v.id,
    ),
    ["25w14craftmine"],
  );
  assert.deepEqual(
    filterVersions(catalog.versions, "release", "1.2").map((v) => v.id),
    ["1.21.1", "1.20.1"],
  );
  assert.deepEqual(filterVersions(catalog.versions, "release", "26w14a"), []);
});

test("switching installation targets preserves version, Fabric and the new-directory draft without creating anything", async () => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  try {
    ui.form.version = "1.20.1";
    ui.form.loader = "fabric";
    fixture.handlers.loader_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadLoaderVersions();
    ui.switchDownloadTarget(null);
    assert.equal(ui.modal.value, "version");
    assert.equal(ui.form.name, "新实例");
    ui.form.name = "Fabric World";
    ui.form.location = "external";
    fixture.handlers.pick_instance_location = () => "/Games/Fabric World";
    await ui.chooseCreationDirectory();
    ui.switchDownloadTarget("instance-b");
    assert.equal(ui.editing.value.name, "官方启动器");
    assert.equal(ui.form.version, "1.20.1");
    assert.equal(ui.form.loader, "fabric");
    assert.equal(ui.form.loader_version, "0.16.14");
    ui.switchDownloadTarget(null);
    assert.equal(ui.form.name, "Fabric World");
    assert.equal(ui.form.external_directory, "/Games/Fabric World");
    assert.equal(ui.canSubmitDownload.value, true);
    assert.equal(
      fixture.calls.some(({ op }) => op === "create"),
      false,
    );
  } finally {
    dispose();
  }
});

test("install uses the chosen target and blocks unknown versions or running targets before mutation", async () => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  try {
    ui.switchDownloadTarget("instance-b");
    ui.form.version = "not-in-official-catalog";
    await ui.submit();
    assert.equal(
      fixture.calls.some(({ op }) => op === "add_version"),
      false,
    );
    ui.form.version = "26.1.1";
    fixture.state.running = ["instance-b"];
    await ui.sync();
    assert.equal(ui.downloadTargetRunning.value, true);
    await ui.submit();
    assert.equal(
      fixture.calls.some(({ op }) => op === "add_version"),
      false,
    );
    fixture.state.running = [];
    await ui.sync();
    fixture.handlers.add_version = ({ id, version }) => {
      assert.equal(id, "instance-b");
      const target = fixture.state.database.instances.find((v) => v.id === id);
      target.selected_version = version;
      target.versions.push({ id: version, installed: false });
      return structuredClone(target);
    };
    fixture.handlers.install = ({ id }) => {
      assert.equal(id, "instance-b");
      return true;
    };
    await ui.submit();
    assert.equal(ui.modal.value, null);
    assert.equal(ui.selectedInstanceId.value, "instance-b");
    assert.equal(
      fixture.calls.some(({ op }) => op === "create"),
      false,
    );
  } finally {
    dispose();
  }
});

test("new external installation validates first, creates once and reuses its directory after a Fabric failure", async () => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  try {
    ui.switchDownloadTarget(null);
    ui.form.version = "1.20.1";
    ui.form.loader = "fabric";
    ui.form.location = "external";
    ui.form.name = "Fabric World";
    fixture.handlers.loader_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadLoaderVersions();
    await ui.submit();
    assert.equal(
      fixture.calls.some(({ op }) => op === "create"),
      false,
    );
    ui.form.external_directory = "/Games/Fabric World";
    const creation = deferred();
    fixture.handlers.create = ({ name, directory }) => {
      assert.equal(name, "Fabric World");
      assert.equal(directory, "/Games/Fabric World");
      return creation.promise;
    };
    fixture.handlers.add_loader_version = () => {
      throw new Error("Network unavailable");
    };
    const submitting = ui.submit();
    await flush();
    assert.equal(ui.downloadSubmitting.value, true);
    await ui.submit();
    ui.switchDownloadTarget("instance-b");
    assert.equal(ui.editing.value, null);
    const target = {
      ...fixture.state.database.instances[0],
      id: "created",
      name: "Fabric World",
      versions: [],
      selected_version: null,
      game_directory: "/Games/Fabric World/.minecraft",
    };
    fixture.state.database.instances.push(target);
    creation.resolve(structuredClone(target));
    await submitting;
    assert.equal(ui.modal.value, "version");
    assert.equal(ui.editing.value.id, "created");
    assert.equal(ui.form.loader_version, "0.16.14");
    fixture.handlers.add_loader_version = ({ id, game_version, loader }) => {
      assert.equal(id, "created");
      assert.equal(game_version, "1.20.1");
      assert.equal(loader.version, "0.16.14");
      target.selected_version = "fabric-loader-0.16.14-1.20.1";
      target.versions.push({ id: target.selected_version, installed: false });
      return structuredClone(target);
    };
    fixture.handlers.install = ({ id }) => {
      assert.equal(id, "created");
      return true;
    };
    await ui.submit();
    assert.equal(ui.modal.value, null);
    assert.equal(fixture.calls.filter(({ op }) => op === "create").length, 1);
    assert.equal(fixture.calls.filter(({ op }) => op === "install").length, 1);
    assert.equal(ui.downloadSubmitting.value, false);
  } finally {
    dispose();
  }
});

test("changing the game clears Fabric eligibility synchronously and ignores an old loader response", async () => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  try {
    ui.form.loader = "fabric";
    fixture.handlers.loader_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadLoaderVersions();
    assert.equal(ui.canSubmitDownload.value, true);
    const old = deferred();
    fixture.handlers.loader_versions = () => old.promise;
    const pending = ui.loadLoaderVersions();
    ui.form.version = "1.20.1";
    assert.equal(ui.form.loader_version, "");
    assert.equal(ui.canSubmitDownload.value, false);
    ui.form.loader = "vanilla";
    old.resolve([{ version: "0.99.0", stable: true }]);
    await pending;
    assert.equal(ui.form.loader_version, "");
    assert.equal(ui.canSubmitDownload.value, true);
  } finally {
    dispose();
  }
});

test("an empty catalog loads its actual latest release and a refresh keeps a user's chosen older version", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const versions = deferred();
    fixture.handlers.versions = () => versions.promise;
    ui.newVersion(fixture.state.database.instances[0]);
    assert.equal(ui.form.version, "");
    assert.equal(ui.canSubmitDownload.value, false);
    versions.resolve(structuredClone(catalog));
    await flush();
    assert.equal(ui.form.version, "26.1.1");
    assert.equal(ui.versionsLoading.value, false);
    ui.form.version = "1.20.1";
    await ui.loadVersions();
    assert.equal(ui.form.version, "1.20.1");
  } finally {
    dispose();
  }
});

test("unavailable Fabric turns its switch off and shows only a brief information notice", async (t) => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  t.mock.timers.enable({ apis: ["setTimeout"] });
  try {
    fixture.handlers.loader_versions = () => [];
    ui.form.loader = "fabric";
    assert.equal(ui.canSubmitDownload.value, false);
    await ui.loadLoaderVersions();
    assert.equal(ui.form.loader, "vanilla");
    assert.equal(ui.form.loader_version, "");
    assert.equal(ui.loaderLoading.value, false);
    assert.equal(ui.error.value, "");
    assert.equal(ui.loaderNotice.value, "此游戏版本暂无可用的加载器 · Fabric");
    assert.equal(ui.canSubmitDownload.value, true);
    t.mock.timers.tick(2000);
    assert.equal(ui.loaderNotice.value, "");
  } finally {
    dispose();
  }
});

test("late empty Fabric responses cannot turn off a new selection; network failures stay distinct from unsupported games", async () => {
  const { fixture, context: ui, dispose } = await downloadFixture();
  try {
    const old = deferred();
    fixture.handlers.loader_versions = () => old.promise;
    ui.form.loader = "fabric";
    const pending = ui.loadLoaderVersions();
    ui.form.version = "1.20.1";
    fixture.handlers.loader_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadLoaderVersions();
    old.resolve([]);
    await pending;
    assert.equal(ui.form.loader, "fabric");
    assert.equal(ui.form.loader_version, "0.16.14");
    assert.equal(ui.loaderNotice.value, "");
    fixture.handlers.loader_versions = () => {
      throw new Error("503 Service Unavailable");
    };
    await ui.loadLoaderVersions(true);
    assert.equal(ui.error.value, "503 Service Unavailable");
    assert.equal(ui.form.loader, "fabric");
    assert.equal(ui.canSubmitDownload.value, true);
    assert.equal(ui.form.loader_version, "0.16.14");
    assert.equal(ui.loaderNotice.value, "");
  } finally {
    dispose();
  }
});
