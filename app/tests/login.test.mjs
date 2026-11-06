import assert from "node:assert/strict";
import test from "node:test";
import { signInWithMicrosoft } from "../src/login.ts";

const challenge = {
  session_id: "test-session",
  verification_uri:
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?prompt=select_account",
  expires_in: 900,
};

test("浏览器授权先等待回调，再打开账号选择页", async () => {
  const browser = challenge;
  let displayed,
    waiting = false;
  const opened = [];
  const account = { id: "browser-player" };
  const result = await signInWithMicrosoft(
    operations({
      begin: async () => browser,
      showChallenge: (value) => {
        displayed = value;
      },
      openBrowser: async (url) => {
        assert.equal(displayed, browser);
        assert.equal(waiting, true);
        opened.push(url);
      },
      waitForLogin: async (value) => {
        assert.equal(value.session_id, browser.session_id);
        waiting = true;
        return account;
      },
    }),
  );
  assert.equal(result, account);
  assert.deepEqual(opened, [browser.verification_uri]);
});

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function operations(overrides = {}) {
  return {
    begin: async () => challenge,
    showChallenge: () => {},
    openBrowser: async () => {},
    browserFailed: () => assert.fail("浏览器应成功打开"),
    waitForLogin: async () => {},
    ...overrides,
  };
}

test("打开浏览器失败会报告错误，并保持授权结果的接收", async () => {
  const signIn = deferred();
  const reported = deferred();
  const browserError = new Error("no default browser");
  let completed = false;
  const login = signInWithMicrosoft(
    operations({
      openBrowser: async () => {
        throw browserError;
      },
      browserFailed: reported.resolve,
      waitForLogin: () => signIn.promise,
    }),
  ).then(() => {
    completed = true;
  });
  assert.equal(await reported.promise, browserError);
  assert.equal(completed, false);
  signIn.resolve();
  await login;
  assert.equal(completed, true);
});

test("授权初始化失败时不打开无效网页，也不开始等待授权", async () => {
  const failure = new Error("invalid client id");
  await assert.rejects(
    signInWithMicrosoft(
      operations({
        begin: async () => {
          throw failure;
        },
        showChallenge: () => assert.fail("不应记录未建立的会话"),
        openBrowser: async () => assert.fail("不应打开浏览器"),
        waitForLogin: async () => assert.fail("不应等待登录"),
      }),
    ),
    (error) => error === failure,
  );
});

test("等待浏览器打开期间也能取消授权，迟到的打开失败不覆盖取消结果", async () => {
  const browser = deferred();
  const signIn = deferred();
  const opened = deferred();
  const failure = new Error("登录已取消");
  let waiting = false;
  const login = signInWithMicrosoft(
    operations({
      waitForLogin: () => {
        waiting = true;
        return signIn.promise;
      },
      openBrowser: () => {
        opened.resolve();
        return browser.promise;
      },
      browserFailed: () => assert.fail("已结束的登录不得再报告打开失败"),
    }),
  );
  const rejected = assert.rejects(login, (error) => error === failure);
  await opened.promise;
  assert.equal(waiting, true);
  signIn.reject(failure);
  await rejected;
  browser.reject(new Error("late browser failure"));
  await new Promise((resolve) => setImmediate(resolve));
});

test("已被新登录替换的响应不再记录、打开浏览器或等待授权", async () => {
  const begin = deferred();
  let current = true;
  const login = signInWithMicrosoft(
    operations({
      begin: () => begin.promise,
      isCurrent: () => current,
      showChallenge: () => assert.fail("旧会话不应记录"),
      waitForLogin: () => assert.fail("旧授权不应等待"),
      openBrowser: () => assert.fail("旧授权不应打开浏览器"),
    }),
  );
  current = false;
  begin.resolve(challenge);
  await login;
});

test("授权成功立即返回账户，不等待系统打开浏览器的迟到回调", async () => {
  const browser = deferred(),
    authorization = deferred(),
    opened = deferred();
  const account = { id: "player", username: "Neko" };
  let result;
  const login = signInWithMicrosoft(
    operations({
      openBrowser: () => {
        opened.resolve();
        return browser.promise;
      },
      waitForLogin: () => authorization.promise,
      browserFailed: () => assert.fail("授权成功后不应显示迟到的浏览器错误"),
    }),
  ).then((value) => {
    result = value;
  });
  await opened.promise;
  authorization.resolve(account);
  await new Promise(setImmediate);
  assert.equal(result, account);
  browser.reject(new Error("late callback"));
  await login;
  await new Promise(setImmediate);
});
