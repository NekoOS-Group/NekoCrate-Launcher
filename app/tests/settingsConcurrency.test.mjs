import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("下载中可保存全局设置，下载完成保留后来打开的设置、页签和未保存草稿", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const download = deferred();
    fixture.handlers.install = () => download.promise;
    const running = ui.install(fixture.state.database.instances[0]);
    assert.equal(ui.busy.value, true);
    const task = ui.current.value;
    ui.toggleUtility("settings");
    await flush();
    ui.settings.language = "en-US";
    ui.settings.concurrency = 4;
    ui.settings.apply_language_to_game = true;
    await ui.saveSettings();
    assert.equal(fixture.state.database.settings.concurrency, 4);
    assert.equal(fixture.state.database.settings.language, "en-US");
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value, task);
    assert.equal(task.status, "running");
    ui.toggleUtility("settings");
    await flush();
    ui.settingsTab.value = "network";
    ui.settings.concurrency = 7;
    fixture.event({
      payload: {
        kind: "complete",
        message: "游戏文件与 Java 已准备好",
        instance_id: "instance-a",
        completed: 1,
        total: 1,
        bytes: 0,
      },
    });
    fixture.state.database.instances[0].versions[0].installed = true;
    download.resolve(true);
    await running;
    await flush();
    assert.equal(ui.utility.value, "settings");
    assert.equal(ui.settingsTab.value, "network");
    assert.equal(ui.settings.concurrency, 7);
    assert.equal(ui.snapshot.value.database.settings.concurrency, 4);
    assert.equal(
      ui.snapshot.value.database.instances[0].versions[0].installed,
      true,
    );
    assert.equal(ui.busy.value, false);
    assert.equal(task.status, "done");
    assert.equal(ui.error.value, "");
  } finally {
    dispose();
  }
});

test("旧保存请求完成不得关闭重开的设置或覆盖新草稿", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const save = deferred();
    fixture.handlers.save_settings = async (request) => {
      await save.promise;
      fixture.state.database.settings = structuredClone(request.settings);
      return true;
    };
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = 4;
    const saving = ui.saveSettings();
    ui.toggleUtility("settings");
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = 11;
    save.resolve();
    await saving;
    assert.equal(ui.utility.value, "settings");
    assert.equal(ui.settings.concurrency, 11);
    assert.equal(ui.snapshot.value.database.settings.concurrency, 4);
  } finally {
    dispose();
  }
});

test("下载的旧快照晚于设置保存返回时，不能回滚设置或丢失安装完成状态", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const download = deferred(),
      stale = deferred();
    fixture.handlers.install = () => download.promise;
    const running = ui.install(fixture.state.database.instances[0]);
    ui.toggleUtility("settings");
    await flush();
    fixture.state.database.instances[0].versions[0].installed = true;
    const oldSnapshot = fixture.snapshot();
    let first = true;
    fixture.handlers.snapshot = () => {
      if (first) {
        first = false;
        return stale.promise;
      }
      return fixture.snapshot();
    };
    download.resolve(true);
    await flush();
    ui.settings.language = "en-US";
    await ui.saveSettings();
    stale.resolve(oldSnapshot);
    await running;
    assert.equal(ui.snapshot.value.database.settings.language, "en-US");
    assert.equal(
      ui.snapshot.value.database.instances[0].versions[0].installed,
      true,
    );
  } finally {
    dispose();
  }
});

test("保存失败保留草稿和弹窗，不取消或结束下载任务", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const download = deferred();
    fixture.handlers.install = () => download.promise;
    fixture.handlers.save_settings = () => {
      throw new Error("write failed");
    };
    const running = ui.install(fixture.state.database.instances[0]);
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = 4;
    await ui.saveSettings();
    assert.equal(ui.utility.value, "settings");
    assert.equal(ui.settings.concurrency, 4);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value.status, "running");
    assert.equal(ui.error.value, "write failed");
    assert.equal(ui.settingsSaving.value, false);
    download.resolve(true);
    await running;
  } finally {
    dispose();
  }
});
