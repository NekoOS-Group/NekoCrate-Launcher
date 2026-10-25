import assert from "node:assert/strict";
import test from "node:test";
import { signInWithMicrosoft } from "../src/login.ts";

const challenge = {
  session_id: "test-session",
  user_code: "TEST-CODE",
  verification_uri: "https://www.microsoft.com/link",
  expires_in: 900,
  interval: 5,
};

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
    copyCode: async () => {},
    copyFailed: () => assert.fail("验证码应成功复制"),
    openBrowser: async () => {},
    browserFailed: () => assert.fail("浏览器应成功打开"),
    waitForLogin: async () => {},
    ...overrides,
  };
}

test("点击登录即展示验证码并自动打开服务返回的微软授权页", async () => {
  let displayed;
  let copied;
  const opened = [];
  await signInWithMicrosoft(
    operations({
      showChallenge: (value) => {
        displayed = value;
      },
      waitForLogin: async (value) => {
        assert.equal(value.session_id, challenge.session_id);
      },
      copyCode: async (code) => {
        copied = code;
      },
      openBrowser: async (url) => {
        assert.equal(displayed, challenge, "浏览器打开前应已展示验证码");
        assert.equal(copied, challenge.user_code, "浏览器打开前应已复制验证码");
        opened.push(url);
      },
    }),
  );
  assert.deepEqual(opened, [challenge.verification_uri]);
});

test("打开浏览器失败仍保留设备授权，允许用户手动打开后完成登录", async () => {
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

test("验证码请求失败时不打开无效网页，也不开始等待授权", async () => {
  const failure = new Error("invalid client id");
  await assert.rejects(
    signInWithMicrosoft(
      operations({
        begin: async () => {
          throw failure;
        },
        showChallenge: () => assert.fail("不应展示验证码"),
        openBrowser: async () => assert.fail("不应打开浏览器"),
        waitForLogin: async () => assert.fail("不应轮询登录"),
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
  let polling = false;
  const login = signInWithMicrosoft(
    operations({
      waitForLogin: () => {
        polling = true;
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
  assert.equal(polling, true);
  signIn.reject(failure);
  await rejected;
  browser.reject(new Error("late browser failure"));
  await new Promise((resolve) => setImmediate(resolve));
});

test("已被新登录替换的验证码响应不再显示、打开浏览器或开始轮询", async () => {
  const begin = deferred();
  let current = true;
  const login = signInWithMicrosoft(
    operations({
      begin: () => begin.promise,
      isCurrent: () => current,
      showChallenge: () => assert.fail("旧验证码不应显示"),
      copyCode: () => assert.fail("旧验证码不应复制"),
      waitForLogin: () => assert.fail("旧授权不应轮询"),
      openBrowser: () => assert.fail("旧授权不应打开浏览器"),
    }),
  );
  current = false;
  begin.resolve(challenge);
  await login;
});

test("自动复制失败不打断微软授权和浏览器打开", async () => {
  const signIn = deferred();
  let reported = false,
    opened = false;
  const login = signInWithMicrosoft(
    operations({
      copyCode: async () => {
        throw new Error("clipboard unavailable");
      },
      copyFailed: () => {
        reported = true;
      },
      openBrowser: async () => {
        opened = true;
      },
      waitForLogin: () => signIn.promise,
    }),
  );
  await new Promise(setImmediate);
  assert.equal(reported, true);
  assert.equal(opened, true);
  signIn.resolve(true);
  await login;
});

test("复制过程中授权已取消时不再弹出浏览器或报告迟到的复制失败", async () => {
  const clipboard = deferred(),
    signIn = deferred();
  const login = signInWithMicrosoft(
    operations({
      copyCode: () => clipboard.promise,
      copyFailed: () => assert.fail("不得报告迟到的剪贴板错误"),
      openBrowser: () => assert.fail("已取消的授权不得打开网页"),
      waitForLogin: () => signIn.promise,
    }),
  );
  const rejected = assert.rejects(login, /登录已取消/);
  await new Promise(setImmediate);
  signIn.reject(new Error("登录已取消"));
  await rejected;
  clipboard.reject(new Error("late clipboard failure"));
  await new Promise(setImmediate);
});
