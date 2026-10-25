import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("手动并发数与自适应开关独立保存，代理开关与协议切换保留地址且成组生效", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = 16;
    ui.settings.adaptive_concurrency = true;
    ui.proxyEnabled.value = false;
    await flush();
    assert.equal(fixture.state.database.settings.concurrency, 16);
    assert.equal(fixture.state.database.settings.adaptive_concurrency, true);
    assert.equal(fixture.state.database.settings.proxy_mode, "none");
    ui.proxyEnabled.value = true;
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "system");
    ui.selectProxyKind("http");
    ui.settings.proxy_url = "http://127.0.0.1:7890";
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "http");
    ui.selectProxyKind("socks");
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "socks");
    assert.equal(
      fixture.state.database.settings.proxy_url,
      "socks5h://127.0.0.1:7890",
    );
    ui.proxyEnabled.value = false;
    await flush();
    ui.proxyEnabled.value = true;
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "socks");
    ui.settings.adaptive_concurrency = false;
    await flush();
    assert.equal(fixture.state.database.settings.concurrency, 16);
    ui.toggleUtility("settings");
    ui.toggleUtility("settings");
    assert.equal(ui.proxyChoice.value, "socks");
    assert.equal(ui.proxyEnabled.value, true);
    for (const call of fixture.calls.filter(
      (call) => call.op === "save_settings",
    )) {
      if (call.settings.proxy_mode === "http")
        assert.match(call.settings.proxy_url, /^https?:/);
      if (call.settings.proxy_mode === "socks")
        assert.match(call.settings.proxy_url, /^socks5h?:/);
    }
  } finally {
    dispose();
  }
});

test("游戏和 Mod 来源独立保存，无效代理草稿不阻塞其他设置；有效模式与地址成组生效", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.toggleUtility("settings");
    await flush();
    ui.settings.download_source = "bmclapi";
    ui.settings.mod_download_source = "official";
    ui.settings.proxy_mode = "custom";
    ui.settings.proxy_url = "http://";
    ui.settings.retries = 4;
    await flush();
    assert.equal(fixture.state.database.settings.download_source, "bmclapi");
    assert.equal(
      fixture.state.database.settings.mod_download_source,
      "official",
    );
    assert.equal(fixture.state.database.settings.proxy_mode, "system");
    assert.equal(fixture.state.database.settings.retries, 4);
    ui.settings.proxy_url = " socks5://127.0.0.1:1080 ";
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "custom");
    assert.equal(
      fixture.state.database.settings.proxy_url,
      "socks5://127.0.0.1:1080",
    );
    ui.settings.proxy_url = "http://user:secret@localhost:7890";
    await flush();
    assert.equal(
      fixture.state.database.settings.proxy_url,
      "socks5://127.0.0.1:1080",
    );
    ui.settings.proxy_mode = "none";
    await flush();
    assert.equal(fixture.state.database.settings.proxy_mode, "none");
    for (const call of fixture.calls.filter(
      (call) => call.op === "save_settings",
    )) {
      assert.ok(
        call.settings.proxy_mode !== "custom" ||
          call.settings.proxy_url === "socks5://127.0.0.1:1080",
      );
      assert.ok(!call.settings.proxy_url.includes("secret"));
    }
  } finally {
    dispose();
  }
});

test("下载中修改设置即自动保存，下载完成保留弹窗、页签和最新设置", async () => {
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
    await flush();
    assert.equal(fixture.state.database.settings.concurrency, 4);
    assert.equal(fixture.state.database.settings.language, "en-US");
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value, task);
    assert.equal(task.status, "running");
    assert.equal(ui.utility.value, "settings");
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
    assert.equal(ui.snapshot.value.database.settings.concurrency, 7);
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

test("连续修改串行写入，关闭重开不丢失修改，旧请求不覆盖新设置", async () => {
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
    await flush();
    assert.equal(ui.settingsSaving.value, true);
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.settings.concurrency, 4);
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = 11;
    ui.settings.language = "en-US";
    await flush();
    assert.equal(
      fixture.calls.filter((call) => call.op === "save_settings").length,
      1,
    );
    ui.toggleUtility("settings");
    await flush();
    assert.equal(document.documentElement.lang, "en-US");
    save.resolve();
    await flush();
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.utility.value, "settings");
    assert.equal(ui.settings.concurrency, 11);
    assert.equal(ui.snapshot.value.database.settings.concurrency, 11);
    assert.equal(fixture.state.database.settings.concurrency, 11);
    assert.equal(fixture.state.database.settings.language, "en-US");
    assert.equal(
      fixture.calls.filter((call) => call.op === "save_settings").length,
      2,
    );
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
    await flush();
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

test("自动保存失败保留修改，重试可保存，不取消或结束下载任务", async () => {
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
    await flush();
    assert.equal(ui.utility.value, "settings");
    assert.equal(ui.settings.concurrency, 4);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value.status, "running");
    assert.equal(ui.settingsSaveError.value, "write failed");
    assert.equal(ui.settingsSaving.value, false);
    await ui.sync();
    assert.equal(ui.settings.concurrency, 4);
    delete fixture.handlers.save_settings;
    await ui.persistSettings();
    assert.equal(ui.settingsSaveError.value, "");
    assert.equal(fixture.state.database.settings.concurrency, 4);
    download.resolve(true);
    await running;
  } finally {
    dispose();
  }
});

test("无效数字不写入也不阻止其他选项生效，合法数值立即保存", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    assert.equal(
      fixture.calls.filter((call) => call.op === "save_settings").length,
      0,
    );
    ui.toggleUtility("settings");
    await flush();
    ui.settings.concurrency = "";
    ui.settings.retries = 9;
    await flush();
    assert.equal(
      fixture.calls.filter((call) => call.op === "save_settings").length,
      0,
    );
    ui.settings.language = "en-US";
    ui.settings.download_source = "bmclapi";
    await flush();
    assert.equal(fixture.state.database.settings.language, "en-US");
    assert.equal(fixture.state.database.settings.download_source, "bmclapi");
    assert.equal(fixture.state.database.settings.concurrency, 8);
    assert.equal(fixture.state.database.settings.retries, 2);
    assert.equal(ui.settings.concurrency, "");
    assert.equal(ui.settings.retries, 9);
    ui.settings.concurrency = 12;
    ui.settings.retries = 0;
    await flush();
    assert.equal(fixture.state.database.settings.concurrency, 12);
    assert.equal(fixture.state.database.settings.retries, 0);
    ui.toggleUtility("settings");
    await flush();
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.settings.concurrency, 12);
    assert.equal(ui.settings.retries, 0);
  } finally {
    dispose();
  }
});

test("立即启动下载时等待设置写入，任务读取新配置", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const save = deferred();
    fixture.handlers.save_settings = async (request) => {
      await save.promise;
      fixture.state.database.settings = structuredClone(request.settings);
      return true;
    };
    let started = false;
    fixture.handlers.install = () => {
      started = true;
      assert.equal(fixture.state.database.settings.download_source, "bmclapi");
      return true;
    };
    ui.toggleUtility("settings");
    await flush();
    ui.settings.download_source = "bmclapi";
    const installing = ui.install(fixture.state.database.instances[0]);
    await flush();
    assert.equal(started, false);
    save.resolve();
    await installing;
    assert.equal(started, true);
  } finally {
    dispose();
  }
});

test("旧配置的 Client ID 为空时直接开始登录，授权期间仍能自动保存设置", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const signIn = deferred();
    const challenge = {
      session_id: "test-session",
      user_code: "TEST-CODE",
      verification_uri: "https://www.microsoft.com/link",
      expires_in: 900,
      interval: 5,
    };
    fixture.handlers.begin_login = () => challenge;
    fixture.handlers.wait_login = () => signIn.promise;
    ui.toggleUtility("accounts");
    assert.equal(fixture.state.database.settings.microsoft_client_id, "");
    const requestStart = fixture.calls.length;
    const signingIn = ui.login();
    await flush();
    assert.deepEqual(ui.challenge.value, challenge);
    assert.equal(ui.busy.value, false);
    assert.equal(ui.loginBusy.value, true);
    assert.deepEqual(
      fixture.calls
        .slice(requestStart)
        .filter((call) => call.op !== "snapshot")
        .map((call) => call.op),
      ["begin_login", "wait_login", "copy_text"],
    );
    assert.equal(fixture.clipboard, challenge.user_code);
    ui.toggleUtility("settings");
    await flush();
    ui.settings.language = "en-US";
    await flush();
    assert.equal(fixture.state.database.settings.language, "en-US");
    assert.equal(fixture.state.database.settings.microsoft_client_id, "");
    assert.equal(ui.busy.value, false);
    assert.equal(ui.loginBusy.value, true);
    signIn.resolve(true);
    await signingIn;
    assert.equal(ui.challenge.value, null);
    assert.equal(ui.loginBusy.value, false);
    assert.equal(ui.busy.value, false);
  } finally {
    dispose();
  }
});
