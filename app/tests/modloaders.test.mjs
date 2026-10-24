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
    fixture.handlers.fabric_versions = () => first.promise;
    const pending = ui.loadFabricVersions();
    ui.form.version = "1.20.1";
    await flush();
    fixture.handlers.fabric_versions = () => [];
    await ui.loadFabricVersions();
    first.resolve([{ version: "0.99.1", stable: true }]);
    await pending;
    assert.equal(ui.form.fabric_version, "");
    assert.equal(ui.form.loader, "vanilla");
    assert.equal(ui.fabricUnavailable.value, false);
    assert.equal(ui.fabricNotice.value, "此游戏版本暂无可用的 Fabric");
    await ui.submit();
    assert.equal(
      fixture.calls.some((r) => r.op === "add_fabric_version"),
      false,
    );
    fixture.handlers.fabric_versions = () => [
      { version: "0.17.0-beta.1", stable: false },
      { version: "0.16.14", stable: true },
    ];
    ui.form.loader = "fabric";
    await ui.loadFabricVersions();
    assert.equal(ui.form.fabric_version, "0.16.14");
    assert.equal(ui.fabricUnavailable.value, false);
    ui.modal.value = null;
    await flush();
    assert.equal(ui.form.fabric_version, "");
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
    fixture.handlers.fabric_versions = () => [
      { version: "0.16.14", stable: true },
    ];
    await ui.loadFabricVersions();
    const child = "fabric-loader-0.16.14-1.21.1";
    fixture.handlers.add_fabric_version = (request) => {
      assert.equal(request.game_version, "1.21.1");
      assert.equal(request.loader_version, "0.16.14");
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
      ["metadata", "files", "natives", "java", "finalize"],
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
