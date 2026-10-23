import assert from "node:assert/strict";
import test from "node:test";
import { computed, watchEffect } from "vue";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";

test("日志仍即时通知界面、保留原顺序及最后 800 条，复制和清空读取相同内容", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const latest = computed(() => ui.logs.value.at(-1)?.message);
  let displayed = [];
  const stop = watchEffect(() => {
    displayed = ui.logs.value.map((line) => line.message);
  });
  try {
    for (let i = 0; i < 1000; i++)
      fixture.event({
        payload: {
          kind: "log",
          message: `line ${i}`,
          instance_id: null,
          total: 0,
          completed: 0,
          bytes: 0,
        },
      });
    assert.equal(latest.value, "line 999");
    await flush();
    assert.deepEqual(
      displayed,
      Array.from({ length: 800 }, (_, i) => `line ${i + 200}`),
    );
    const copied = ui.logs.value
      .map((line) => `[${line.time}] ${line.message}`)
      .join("\n");
    assert.equal(copied.split("\n").length, 800);
    assert.ok(copied.endsWith("line 999"));
    ui.logs.value = [];
    await flush();
    assert.deepEqual(displayed, []);
    ui.appendLog({ time: "12:00:00", kind: "error", message: "save failed" });
    await flush();
    assert.deepEqual(displayed, ["save failed"]);
  } finally {
    stop();
    dispose();
  }
});
