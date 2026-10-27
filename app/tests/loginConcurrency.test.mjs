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

function challenge(session_id) {
  return {
    session_id,
    user_code: "TEST-CODE",
    verification_uri: "https://www.microsoft.com/link",
    expires_in: 900,
    interval: 5,
  };
}

test("关闭授权弹窗结束当前登录，随后可以重新添加账户", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const authorization = deferred();
    fixture.handlers.begin_login = ({ session_id }) => challenge(session_id);
    fixture.handlers.wait_login = () => authorization.promise;
    fixture.handlers.cancel_login = ({ session_id }) => {
      assert.equal(session_id, session);
      authorization.reject(new Error("登录已取消"));
      return true;
    };
    ui.toggleUtility("accounts");
    const signingIn = ui.login();
    await flush();
    const session = ui.challenge.value.session_id;
    ui.closeUtility();
    await signingIn;
    await flush();
    assert.equal(ui.utility.value, null);
    assert.equal(ui.challenge.value, null);
    assert.equal(ui.loginBusy.value, false);
    assert.equal(ui.error.value, "");
    fixture.handlers.wait_login = () => account;
    ui.toggleUtility("accounts");
    await ui.login();
    assert.equal(
      fixture.calls.filter((call) => call.op === "begin_login").length,
      2,
    );
  } finally {
    dispose();
  }
});

test("重新登录使用独立会话，旧授权迟到的错误不清空新验证码和等待状态", async () => {
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

test("组件重载时只取消自己的授权，迟到的验证码不再启动轮询", async () => {
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
