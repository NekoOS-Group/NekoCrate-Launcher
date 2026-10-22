import assert from "node:assert/strict";
import test from "node:test";
import { prepareAndLaunch } from "../src/launch.ts";

function instance(installed = false) {
  return {
    id: "game-directory-a",
    name: "日常游戏",
    versions: [{ id: "1.21.1", installed }],
    selected_version: "1.21.1",
    memory_mb: 2048,
    java_path: null,
    created_at: 0,
    last_played: null,
  };
}

test("已安装版本直接启动，不重复安装", async () => {
  const calls = [];
  await prepareAndLaunch(instance(true), {
    install: async () => {
      assert.fail("不应重新安装");
    },
    launch: async (target) => {
      calls.push(target.id);
    },
  });
  assert.deepEqual(calls, ["game-directory-a"]);
});

test("下载完成后启动原实例，即使用户期间浏览了另一个实例", async () => {
  const original = instance();
  let selection = original;
  let finish;
  const downloading = new Promise((resolve) => {
    finish = resolve;
  });
  const launched = [];
  const starting = prepareAndLaunch(selection, {
    install: async (target) => {
      assert.equal(target.id, original.id);
      return downloading;
    },
    launch: async (target) => {
      launched.push(target.id);
    },
  });
  selection = { ...instance(true), id: "game-directory-b" };
  assert.deepEqual(launched, []);
  finish(true);
  await starting;
  assert.deepEqual(launched, [original.id]);
  assert.notEqual(launched[0], selection.id);
});

for (const result of [false, undefined]) {
  test(`安装失败或取消后不启动：${result}`, async () => {
    await prepareAndLaunch(instance(), {
      install: async () => result,
      launch: async () => {
        assert.fail("未完成安装时不得启动");
      },
    });
  });
}

test("没有版本时不安装或启动", async () => {
  await prepareAndLaunch(
    { ...instance(), versions: [], selected_version: null },
    {
      install: async () => {
        assert.fail("没有可安装的版本");
      },
      launch: async () => {
        assert.fail("没有可启动的版本");
      },
    },
  );
});

test("安装异常不会落入启动流程", async () => {
  await assert.rejects(
    prepareAndLaunch(instance(), {
      install: async () => {
        throw new Error("connection failed");
      },
      launch: async () => {
        assert.fail("安装异常后不得启动");
      },
    }),
    /connection failed/,
  );
});
