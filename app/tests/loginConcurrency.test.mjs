import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const account = {
  id: "player",
  username: "Neko",
  expires_at: 9999999999,
  client_id: "client",
  owned: true,
  skin: null,
};

test("有效登录在启动、打开列表和重复检查时不发送刷新请求", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [account];
    f.state.database.selected_account = account.id;
    await f.context.sync();
    await flush();
    for (let i = 0; i < 3; i++) {
      f.context.toggleUtility("accounts");
      await f.context.refreshAccountAutomatically();
    }
    assert.equal(f.calls.filter((c) => c.op === "sync_account").length, 0);
    assert.equal(f.context.selectedAccount.value.username, account.username);
  } finally {
    f.dispose();
  }
});

test("自动刷新账户原位更新资料，不锁定界面或发起新的授权", async () => {
  const f = await settingsFixture();
  try {
    const refreshing = deferred();
    f.state.database.accounts = [
      { ...account, expires_at: Math.floor(Date.now() / 1000) + 30 },
    ];
    f.state.database.selected_account = account.id;
    f.handlers.sync_account = ({ id }) => {
      assert.equal(id, account.id);
      return refreshing.promise;
    };
    await f.context.sync();
    await flush();
    assert.equal(f.calls.filter((c) => c.op === "sync_account").length, 1);
    assert.equal(f.context.busy.value, false);
    assert.equal(f.context.loginBusy.value, false);
    const check = f.context.refreshAccountAutomatically();
    refreshing.resolve({ ...account, username: "New_Name" });
    await check;
    assert.equal(f.context.selectedAccount.value.username, "New_Name");
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
    f.context.toggleUtility("accounts");
    await f.context.refreshAccountAutomatically();
    assert.equal(f.calls.filter((c) => c.op === "sync_account").length, 1);
    assert.equal(
      f.calls.some((c) =>
        ["begin_login", "cancel_login", "refresh_account"].includes(c.op),
      ),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("自动刷新失败保留当前资料和页面错误，避免连续重试", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [
      { ...account, expires_at: Math.floor(Date.now() / 1000) + 30 },
    ];
    f.state.database.selected_account = account.id;
    f.handlers.sync_account = () => {
      throw new Error("offline");
    };
    f.context.error.value = "previous error";
    await f.context.sync();
    await flush();
    await f.context.refreshAccountAutomatically();
    assert.equal(f.context.selectedAccount.value.username, account.username);
    assert.equal(f.context.error.value, "previous error");
    assert.equal(f.context.busy.value, false);
    assert.equal(f.calls.filter((c) => c.op === "sync_account").length, 1);
  } finally {
    f.dispose();
  }
});

test("后台刷新迟到的结果不能覆盖重新登录获得的新资料", async () => {
  const f = await settingsFixture();
  try {
    const refreshing = deferred();
    f.state.database.accounts = [
      { ...account, expires_at: Math.floor(Date.now() / 1000) + 30 },
    ];
    f.state.database.selected_account = account.id;
    f.handlers.sync_account = () => refreshing.promise;
    await f.context.sync();
    await flush();
    const check = f.context.refreshAccountAutomatically();
    f.handlers.begin_login = ({ session_id }) => challenge(session_id);
    f.handlers.wait_login = () => ({ ...account, username: "New_Login" });
    await f.context.login();
    refreshing.resolve({ ...account, username: "Stale_Name" });
    await check;
    assert.equal(f.context.selectedAccount.value.username, "New_Login");
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
  } finally {
    f.dispose();
  }
});

function challenge(session_id) {
  return {
    session_id,
    verification_uri:
      "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?prompt=select_account",
    expires_in: 900,
  };
}

test("浏览器授权期间保留账户列表与标题，成功后原位加入头像", async () => {
  const f = await settingsFixture();
  try {
    const browser = deferred();
    f.handlers.begin_login = ({ session_id }) => challenge(session_id);
    f.handlers.wait_login = () => browser.promise;
    f.context.toggleUtility("accounts");
    const title = f.context.utilityTitle.value;
    const previous = f.context.snapshot.value.database.accounts;
    const login = f.context.login();
    await flush();
    assert.equal(
      f.calls.some((c) => c.op === "copy_text"),
      false,
    );
    assert.equal(f.context.utility.value, "accounts");
    assert.equal(f.context.utilityTitle.value, title);
    assert.equal(f.context.snapshot.value.database.accounts, previous);
    browser.resolve(account);
    await login;
    assert.equal(f.context.utility.value, "accounts");
    assert.equal(f.context.utilityTitle.value, title);
    assert.equal(f.context.error.value, "");
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
  } finally {
    f.dispose();
  }
});

test("无法建立浏览器授权时可直接再次添加账户重试", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.begin_login = () => {
      throw new Error("无法创建本机登录回调");
    };
    await f.context.login();
    assert.equal(f.context.loginBusy.value, false);
    assert.match(f.context.error.value, /本机登录回调/);
    f.handlers.begin_login = ({ session_id }) => challenge(session_id);
    f.handlers.wait_login = () => account;
    await f.context.login();
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
  } finally {
    f.dispose();
  }
});

test("关闭账户列表不结束浏览器授权，登录完成也不会重新打开弹窗", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const authorization = deferred();
    fixture.handlers.begin_login = ({ session_id }) => challenge(session_id);
    fixture.handlers.wait_login = () => authorization.promise;
    fixture.handlers.cancel_login = () =>
      assert.fail("收起账户列表不能取消授权");
    ui.toggleUtility("accounts");
    const signingIn = ui.login();
    await flush();
    ui.closeUtility();
    await flush();
    assert.equal(ui.utility.value, null);
    assert.equal(ui.loginBusy.value, true);
    assert.equal(ui.error.value, "");
    authorization.resolve(account);
    await signingIn;
    assert.equal(ui.utility.value, null);
    assert.equal(ui.loginBusy.value, false);
    assert.equal(ui.snapshot.value.database.selected_account, account.id);
  } finally {
    dispose();
  }
});

test("重新登录使用独立会话，旧授权迟到的错误不清空新会话和等待状态", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const first = deferred(),
      second = deferred();
    let waits = 0;
    fixture.handlers.begin_login = ({ session_id }) => challenge(session_id);
    fixture.handlers.wait_login = ({ session_id }) => {
      assert.equal(session_id, ui.challenge.value.session_id);
      return ++waits === 1 ? first.promise : second.promise;
    };
    const oldLogin = ui.login();
    await flush();
    const oldSession = ui.challenge.value.session_id;
    const newLogin = ui.login();
    await flush();
    const newChallenge = ui.challenge.value;
    assert.notEqual(oldSession, newChallenge.session_id);
    first.reject(new Error("登录已取消"));
    await oldLogin;
    assert.equal(ui.challenge.value, newChallenge);
    assert.equal(ui.loginBusy.value, true);
    assert.equal(ui.error.value, "");
    second.resolve(account);
    await newLogin;
    assert.equal(ui.challenge.value, null);
    assert.equal(ui.loginBusy.value, false);
  } finally {
    dispose();
  }
});

test("登录成功同步展示新账户且不请求全量快照，迟到的旧快照不能覆盖它", async () => {
  const f = await settingsFixture();
  try {
    const stale = deferred(),
      oldState = f.fixture.snapshot();
    oldState.state.running = ["instance-a"];
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    f.handlers.begin_login = ({ session_id }) => challenge(session_id);
    f.handlers.wait_login = () => account;
    const before = f.calls.filter((c) => c.op === "snapshot").length;
    await f.context.login();
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
    assert.deepEqual(f.context.snapshot.value.database.accounts, [account]);
    assert.equal(f.context.challenge.value, null);
    assert.equal(f.context.loginBusy.value, false);
    assert.equal(f.calls.filter((c) => c.op === "snapshot").length, before);
    stale.resolve(oldState);
    await syncing;
    assert.deepEqual(f.context.snapshot.value.running, ["instance-a"]);
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
  } finally {
    f.dispose();
  }
});

test("头像与皮肤预览共用请求，凭据刷新不重置已显示的皮肤", async () => {
  const f = await settingsFixture();
  try {
    const skin = deferred();
    f.handlers.account_skin = () => skin.promise;
    f.state.database.accounts = [
      { ...account, skin: { url: "texture-a", slim: false } },
    ];
    f.state.database.selected_account = account.id;
    await f.context.sync();
    await flush();
    const avatar = f.context.fetchAccountSkin(account.id);
    assert.equal(f.calls.filter((c) => c.op === "account_skin").length, 1);
    skin.resolve([1, 2, 3]);
    await avatar;
    await flush();
    const texture = f.context.skinTexture.value;
    assert.ok(texture?.startsWith("data:image/png;base64,"));
    f.state.database.accounts[0].expires_at++;
    await f.context.sync();
    await flush();
    assert.equal(f.context.skinTexture.value, texture);
    assert.equal(f.context.skinLoading.value, false);
    assert.equal(f.calls.filter((c) => c.op === "account_skin").length, 1);
  } finally {
    f.dispose();
  }
});

test("切换账户只等待选择写入，失败时保留原选择与面板", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [account];
    await f.context.sync();
    f.context.utility.value = "accounts";
    f.handlers.select_account = () => {
      throw new Error("cannot save");
    };
    await f.context.chooseAccount(account);
    assert.equal(f.context.snapshot.value.database.selected_account, null);
    assert.equal(f.context.utility.value, "accounts");
    const before = f.calls.filter((c) => c.op === "snapshot").length;
    f.handlers.select_account = () => true;
    await f.context.chooseAccount(account);
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
    assert.equal(f.context.utility.value, null);
    assert.equal(f.calls.filter((c) => c.op === "snapshot").length, before);
  } finally {
    f.dispose();
  }
});

test("新任务先取消授权并等待清理，旧登录结束不解锁正在下载的任务", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const authorization = deferred(),
      cancellation = deferred(),
      download = deferred();
    fixture.handlers.begin_login = ({ session_id }) => challenge(session_id);
    fixture.handlers.wait_login = () => authorization.promise;
    const signingIn = ui.login();
    await flush();
    const session = ui.challenge.value.session_id;
    let started = false;
    fixture.handlers.cancel_login = ({ session_id }) => {
      assert.equal(session_id, session);
      return cancellation.promise;
    };
    fixture.handlers.install = () => {
      started = true;
      return download.promise;
    };
    const installing = ui.action(
      "下载游戏",
      "install",
      { id: "instance-a" },
      true,
    );
    await flush();
    assert.equal(started, false);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.challenge.value, null);
    authorization.reject(new Error("登录已取消"));
    await signingIn;
    assert.equal(ui.busy.value, true);
    assert.equal(ui.error.value, "");
    cancellation.resolve(true);
    await flush();
    assert.equal(started, true);
    await ui.login();
    assert.equal(
      fixture.calls.filter((call) => call.op === "begin_login").length,
      1,
    );
    download.resolve(true);
    await installing;
    assert.equal(ui.busy.value, false);
    assert.equal(
      fixture.calls.some((call) => call.op === "cancel"),
      false,
    );
  } finally {
    dispose();
  }
});

test("组件重载时只取消自己的授权，迟到的会话不再等待回调", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const begin = deferred();
  fixture.handlers.begin_login = () => begin.promise;
  fixture.handlers.cancel_login = () => true;
  const signingIn = ui.login();
  await flush();
  const session = fixture.calls.find(
    (call) => call.op === "begin_login",
  ).session_id;
  dispose();
  begin.resolve(challenge(session));
  await signingIn;
  assert.equal(
    fixture.calls.find((call) => call.op === "cancel_login").session_id,
    session,
  );
  assert.equal(
    fixture.calls.some(
      (call) => call.op === "cancel" || call.op === "wait_login",
    ),
    false,
  );
  assert.equal(ui.challenge.value, null);
});
