import test from "node:test";
import assert from "node:assert/strict";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const release = (version = "dev0.5") => ({
  available: true,
  latest_version: version,
  updater_url: "https://example.invalid/latest.json",
  install_support: null,
});
const progress = (stage) => ({
  stage,
  downloaded: 100,
  total: 100,
  version: "dev0.5",
  error: null,
});

test("应用内下载完成后安装重启，重复点击不会重复下载", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.updateStatus.value = release();
    const download = deferred();
    fixture.handlers.download_launcher_update = () => download.promise;
    fixture.handlers.install_launcher_update = () =>
      fixture.updateEvent({ payload: progress("installing") });
    const first = ui.downloadLauncherUpdate();
    await ui.downloadLauncherUpdate();
    assert.equal(
      fixture.calls.filter((c) => c.op === "download_launcher_update").length,
      1,
    );
    fixture.updateEvent({
      payload: { ...progress("downloading"), downloaded: 53 },
    });
    assert.match(ui.updateProgressLabel.value, /53%/);
    download.resolve(progress("ready"));
    await first;
    assert.equal(
      fixture.calls.filter((c) => c.op === "install_launcher_update").length,
      1,
    );
    assert.equal(ui.updateProgress.value.stage, "installing");
  } finally {
    dispose();
  }
});

test("游戏或下载运行时保留已下载更新，稍后安装不用重新下载", async () => {
  for (const reason of ["game", "download"]) {
    const { fixture, context: ui, dispose } = await settingsFixture();
    try {
      ui.updateStatus.value = release();
      if (reason === "game") ui.snapshot.value.running = ["instance-a"];
      else ui.busy.value = true;
      fixture.handlers.download_launcher_update = () => progress("ready");
      fixture.handlers.install_launcher_update = () => true;
      await ui.downloadLauncherUpdate();
      assert.equal(ui.updateProgress.value.stage, "ready");
      assert.equal(
        fixture.calls.some((c) => c.op === "install_launcher_update"),
        false,
      );
      ui.snapshot.value.running = [];
      ui.busy.value = false;
      await ui.installLauncherUpdate();
      assert.equal(
        fixture.calls.filter((c) => c.op === "download_launcher_update").length,
        1,
      );
      assert.equal(
        fixture.calls.filter((c) => c.op === "install_launcher_update").length,
        1,
      );
    } finally {
      dispose();
    }
  }
});

test("取消下载更新不会进入安装阶段", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.updateStatus.value = release();
    const download = deferred();
    fixture.handlers.download_launcher_update = () => download.promise;
    fixture.handlers.cancel_launcher_update = () =>
      download.resolve(progress("idle"));
    const pending = ui.downloadLauncherUpdate();
    await ui.cancelLauncherUpdate();
    await pending;
    assert.equal(ui.updateActive.value, false);
    assert.equal(
      fixture.calls.some((c) => c.op === "install_launcher_update"),
      false,
    );
  } finally {
    dispose();
  }
});

test("更新失败重试会重新检查版本，不反复请求失效版本", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.updateStatus.value = release("dev0.4.1");
    fixture.handlers.download_launcher_update = () => {
      throw new Error("更新版本已变化，请重新检查更新");
    };
    await ui.downloadLauncherUpdate();
    assert.equal(ui.updateProgress.value.stage, "error");
    fixture.handlers.check_update = () => release("dev0.5");
    fixture.handlers.download_launcher_update = () => progress("ready");
    fixture.handlers.install_launcher_update = () => true;
    await ui.retryLauncherUpdate();
    assert.equal(
      fixture.calls.filter((c) => c.op === "download_launcher_update").at(-1)
        .version,
      "dev0.5",
    );
    assert.equal(ui.updateProgress.value.stage, "ready");
    await flush();
  } finally {
    dispose();
  }
});
