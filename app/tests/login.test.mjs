import assert from "node:assert/strict";
import test from "node:test";
import { signInWithMicrosoft } from "../src/login.ts";

const challenge = {
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
    openBrowser: async () => {},
    browserFailed: () => assert.fail("浏览器应成功打开"),
    waitForLogin: async () => {},
    ...overrides,
  };
}

test("点击登录即展示验证码并自动打开服务返回的微软授权页", async () => {
  let displayed;
  const opened = [];
  await signInWithMicrosoft(
    operations({
      showChallenge: (value) => {
        displayed = value;
      },
      openBrowser: async (url) => {
        assert.equal(displayed, challenge, "浏览器打开前应已展示验证码");
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
