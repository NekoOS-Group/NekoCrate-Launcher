import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("代理密码自动保存、重新打开恢复；其他设置不重复提交密码，留空明确清除", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.toggleUtility("settings");
    await flush();
    ui.selectProxyKind("http");
    ui.settings.proxy_url = "http://localhost:7890";
    ui.settings.proxy_username = "fixture-user";
    ui.settings.proxy_password = "fixture-password";
    await flush();
    assert.equal(fixture.proxyPassword, "fixture-password");
    assert.ok(fixture.state.database.settings.proxy_password_key);
    assert.equal(fixture.state.database.settings.proxy_password, undefined);
    assert.equal(ui.snapshot.value.database.settings.proxy_password, undefined);
    ui.settings.concurrency = 12;
    ui.selectProxyKind("socks");
    for (const source of ["modrinth", "curseforge", "bbsmc", "auto"]) {
      ui.settings.mod_download_source = source;
      await flush();
      assert.equal(fixture.state.database.settings.mod_download_source, source);
    }
    assert.equal(
      fixture.calls.filter(
        (c) =>
          c.op === "save_settings" && c.settings.proxy_password !== undefined,
      ).length,
      1,
    );
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.settings.proxy_password, undefined);
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.settings.proxy_password, "fixture-password");
    assert.equal(ui.settings.proxy_username, "fixture-user");
    assert.equal(
      fixture.calls.filter(
        (c) =>
          c.op === "save_settings" && c.settings.proxy_password !== undefined,
      ).length,
      1,
    );
    ui.settings.proxy_password = "";
    await flush();
    assert.equal(fixture.proxyPassword, "");
    assert.equal(fixture.state.database.settings.proxy_password_key, null);
  } finally {
    dispose();
  }
});

test("读取失败不会清除已存密码；迟到的读取结果不能覆盖新编辑或已关闭弹窗", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.state.database.settings.proxy_password_key = "fixture-old";
    fixture.proxyPassword = "old-password";
    await ui.sync();
    fixture.handlers.proxy_password = () => {
      throw new Error("vault unavailable");
    };
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.proxyPasswordLoading.value, false);
    assert.equal(ui.proxyPasswordError.value, "vault unavailable");
    ui.settings.concurrency = 13;
    await flush();
    assert.equal(fixture.proxyPassword, "old-password");
    assert.equal(
      fixture.state.database.settings.proxy_password_key,
      "fixture-old",
    );
    ui.toggleUtility("settings");
    await flush();
    const load = deferred();
    fixture.handlers.proxy_password = () => load.promise;
    ui.toggleUtility("settings");
    await flush();
    assert.equal(ui.proxyPasswordLoading.value, true);
    ui.settings.proxy_password = "new-password";
    await flush();
    load.resolve("old-password");
    await flush();
    assert.equal(ui.settings.proxy_password, "new-password");
    ui.toggleUtility("settings");
    await flush();
    const late = deferred();
    fixture.handlers.proxy_password = () => late.promise;
    ui.toggleUtility("settings");
    await flush();
    ui.toggleUtility("settings");
    await flush();
    late.resolve("hidden-password");
    await flush();
    assert.equal(ui.settings.proxy_password, undefined);
  } finally {
    dispose();
  }
});

test("无效代理草稿的密码延迟到地址完整再提交，保存失败可重试", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.toggleUtility("settings");
    await flush();
    ui.selectProxyKind("http");
    ui.settings.proxy_username = "fixture";
    ui.settings.proxy_password = "draft-password";
    ui.settings.concurrency = 9;
    await flush();
    assert.equal(fixture.proxyPassword, "");
    fixture.handlers.save_settings = () => {
      throw new Error("vault locked");
    };
    ui.settings.proxy_url = "http://localhost:7890";
    await flush();
    assert.equal(ui.settingsSaveError.value, "vault locked");
    delete fixture.handlers.save_settings;
    await ui.persistSettings();
    assert.equal(fixture.proxyPassword, "draft-password");
    assert.equal(fixture.state.database.settings.concurrency, 9);
  } finally {
    dispose();
  }
});
