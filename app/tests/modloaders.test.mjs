import assert from "node:assert/strict";
import test from "node:test";
import { versionDescription } from "../src/versionDescription.ts";
import { setLanguage } from "../src/i18n.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("version subtitles use resolved game/loader metadata rather than guessing pack names", () => {
  setLanguage("zh-CN", "zh-CN");
  assert.equal(
    versionDescription({
      id: "RLCraft",
      minecraft_version: "1.12.2",
      loader: { kind: "forge", version: "14.23.5.2860" },
    }),
    "1.12.2 · Forge 14.23.5.2860",
  );
  assert.equal(
    versionDescription({
      id: "My world",
      minecraft_version: "1.21.1",
      loader: { kind: "neoforge", version: "21.1.172" },
    }),
    "1.21.1 · NeoForge 21.1.172",
  );
  assert.equal(
    versionDescription({
      id: "fabric-loader-0.16.14-1.21.1",
      minecraft_version: "1.21.1",
      loader: { kind: "fabric", version: "0.16.14" },
    }),
    "1.21.1 · Fabric 0.16.14",
  );
  assert.equal(versionDescription({ id: "1.21.1" }), "1.21.1 · 原版");
  setLanguage("en-US", "en-US");
  assert.equal(versionDescription({ id: "1.21.1" }), "1.21.1 · Vanilla");
});

test("Fabric selection ignores late responses and stops submitting unsupported versions", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.manifest.value = {
      latest: { release: "1.21.1" },
      versions: [{ id: "1.21.1", type: "release", url: "", sha1: "" }],
    };
    ui.newVersion(fixture.state.database.instances[0]);
    ui.form.loader = "fabric";
    await flush();
    const first = deferred();
    fixture.handlers.loader_versions = () => first.promise;
    const pending = ui.loadLoaderVersions();
    ui.form.version = "1.20.1";
    await flush();
    fixture.handlers.loader_versions = () => [];
    await ui.loadLoaderVersions();
    first.resolve([{ version: "0.99.1", stable: true }]);
    await pending;
    assert.equal(ui.form.loader_version, "");
    assert.equal(ui.form.loader, "vanilla");
    assert.equal(ui.loaderUnavailable.value, false);
    assert.equal(ui.loaderNotice.value, "此游戏版本暂无可用的加载器 · Fabric");
    await ui.submit();
    assert.equal(
      fixture.calls.some((r) => r.op === "add_loader_version"),
      false,
    );
    fixture.handlers.loader_versions = () => [
      { version: "0.17.0-beta.1", stable: false },
      { version: "0.16.14", stable: true },
    ];
    ui.form.loader = "fabric";
    await ui.loadLoaderVersions(true);
    assert.equal(ui.form.loader_version, "0.16.14");
    assert.equal(ui.loaderUnavailable.value, false);
    ui.modal.value = null;
    await flush();
    assert.equal(ui.form.loader_version, "");
  } finally {
    dispose();
  }
});

test("Fabric creation hands its new child version to the existing download workflow", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.manifest.value = {
      latest: { release: "1.21.1" },
      versions: [{ id: "1.21.1", type: "release", url: "", sha1: "" }],
    };
    const instance = fixture.state.database.instances[0];
    ui.newVersion(instance);
    ui.form.loader = "fabric";
    await flush();
    fixture.handlers.loader_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadLoaderVersions();
    const child = "fabric-loader-0.16.14-1.21.1";
    fixture.handlers.add_loader_version = (request) => {
      assert.equal(request.game_version, "1.21.1");
      assert.equal(request.loader.version, "0.16.14");
      instance.versions.push({
        id: child,
        installed: false,
        minecraft_version: "1.21.1",
        loader: { kind: "fabric", version: "0.16.14" },
      });
      instance.selected_version = child;
      return structuredClone(instance);
    };
    fixture.handlers.install = () => {
      assert.equal(instance.selected_version, child);
      instance.versions.at(-1).installed = true;
      return true;
    };
    await ui.submit();
    assert.equal(ui.modal.value, null);
    const task = ui.tasks.value.find((task) => task.version_id === child);
    assert.equal(task.status, "done");
    assert.deepEqual(
      task.workflow.steps.map((step) => step.id),
      ["metadata", "loader", "files", "natives", "java", "finalize"],
    );
    assert.equal(
      fixture.calls.some((r) => r.op === "add_version"),
      false,
    );
  } finally {
    dispose();
  }
});

test("unexpected exits automatically show logs; normal exits and manual stops stay quiet", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const event = (kind, message) =>
      fixture.event({
        payload: {
          kind,
          message,
          instance_id: "instance-a",
          completed: 0,
          total: 0,
          bytes: 0,
        },
      });
    event("game_exit", "游戏已正常退出");
    assert.equal(ui.showLogs.value, false);
    event("game_exit", "游戏已停止");
    assert.equal(ui.showLogs.value, false);
    event("log", "Preloading Tricks: transformer failed");
    event("game_crash", "游戏已退出（状态 exit status: 255）");
    await flush();
    assert.equal(ui.showLogs.value, true);
    assert.equal(ui.logStore.snapshot().at(-1).kind, "game_crash");
    assert.ok(
      ui.logStore
        .snapshot()
        .some((line) => line.message.includes("transformer failed")),
    );
  } finally {
    dispose();
  }
});

test("Forge and NeoForge use the chosen kind, recommended build and common installer", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.manifest.value = {
      latest: { release: "1.20.1" },
      versions: [{ id: "1.20.1", type: "release", url: "", sha1: "" }],
    };
    const instance = fixture.state.database.instances[0];
    ui.newVersion(instance);
    const old = deferred();
    fixture.handlers.loader_versions = ({ kind, game_version }) => {
      assert.equal(game_version, "1.20.1");
      if (kind === "forge") return old.promise;
      assert.equal(kind, "neoforge");
      return [{ version: "47.1.106", stable: true }];
    };
    ui.form.loader = "forge";
    const loading = ui.loadLoaderVersions();
    ui.form.loader = "neoforge";
    assert.equal(ui.canSubmitDownload.value, false);
    await ui.loadLoaderVersions();
    old.resolve([{ version: "47.4.4", recommended: true }]);
    await loading;
    assert.equal(ui.form.loader, "neoforge");
    assert.equal(ui.form.loader_version, "47.1.106");
    fixture.handlers.loader_versions = ({ kind }) => {
      assert.equal(kind, "forge");
      return [
        { version: "47.4.28", recommended: false },
        { version: "47.4.4", recommended: true },
      ];
    };
    ui.form.loader = "forge";
    await ui.loadLoaderVersions();
    assert.equal(ui.form.loader_version, "47.4.4");
    fixture.handlers.add_loader_version = ({ id, game_version, loader }) => {
      assert.equal(id, instance.id);
      assert.deepEqual(loader, { kind: "forge", version: "47.4.4" });
      const version = {
        id: "1.20.1-forge-47.4.4",
        minecraft_version: game_version,
        loader,
        installed: false,
      };
      instance.versions.push(version);
      instance.selected_version = version.id;
      return structuredClone(instance);
    };
    fixture.handlers.install = () => true;
    await ui.submitDownload();
    const task = ui.tasks.value.find(
      (t) => t.version_id === "1.20.1-forge-47.4.4",
    );
    assert.equal(task.status, "done");
    assert.ok(task.workflow.steps.some((step) => step.id === "loader"));
  } finally {
    dispose();
  }
});
