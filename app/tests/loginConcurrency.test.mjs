import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

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
    fixture.handlers.wait_login = () => true;
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
    second.resolve(true);
    await newLogin;
    assert.equal(ui.challenge.value, null);
    assert.equal(ui.loginBusy.value, false);
  } finally {
    dispose();
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
