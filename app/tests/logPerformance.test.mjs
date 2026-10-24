import assert from "node:assert/strict";
import test from "node:test";
import { createLogStore, logRefreshMs } from "../src/logStore.ts";
import {
  presentLogs,
  createLogPresenter,
  copyLogRows,
} from "../src/logPresentation.ts";
import { LogViewport } from "../src/logViewport.ts";

const line = (i, instanceId = "a") => ({
  time: "12:00:00",
  kind: "log",
  message: `line ${i}`,
  instanceId,
});

test("burst ingestion retains the whole session and publishes once per batch only while subscribed", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const store = createLogStore();
  for (let i = 0; i < 10000; i++) store.append(line(i));
  assert.equal(store.snapshot().length, 10000);
  assert.equal(store.snapshot()[0].message, "line 0");
  let notifications = 0;
  const stop = store.subscribe(() => notifications++);
  t.mock.timers.tick(logRefreshMs);
  assert.equal(
    notifications,
    0,
    "hidden ingestion must not leave a scheduled refresh",
  );
  for (let i = 10000; i < 20000; i++) store.append(line(i));
  assert.equal(notifications, 0);
  assert.equal(store.snapshot().at(-1).message, "line 19999");
  t.mock.timers.tick(logRefreshMs);
  assert.equal(notifications, 1);
  assert.equal(
    copyLogRows(presentLogs(store.snapshot())).split("\n").length,
    20000,
  );
  store.append(line(20000));
  stop();
  t.mock.timers.tick(logRefreshMs);
  assert.equal(
    notifications,
    1,
    "closing the viewer cancels pending refreshes",
  );
  store.append(line(20001, "b"));
  store.clear("a");
  assert.deepEqual(store.snapshot(), [line(20001, "b")]);
  store.clear();
  assert.deepEqual(store.snapshot(), []);
  store.append(line(20002));
  assert.deepEqual(store.snapshot(), [line(20002)]);
  store.dispose();
});

test("variable-height virtualization bounds the rendered set, follows the tail, and preserves anchors", () => {
  const entries = Array.from({ length: 800 }, (_, i) => line(i));
  const rows = presentLogs(entries);
  const viewport = new LogViewport();
  viewport.resize(700, 280);
  viewport.setRows(rows);
  assert.ok(viewport.window().items.length <= 17);
  assert.equal(viewport.window().items.at(-1).id, rows.at(-1).id);
  assert.equal(viewport.top, 800 * 28 - 280);
  viewport.measure([{ id: rows.at(-1).id, height: 280 }]);
  assert.equal(viewport.top, 800 * 28 - 28);
  assert.equal(viewport.window().items.at(-1).id, rows.at(-1).id);
  viewport.scroll(100 * 28 + 5);
  assert.equal(viewport.following, false);
  viewport.measure([{ id: rows[20].id, height: 280 }]);
  assert.equal(
    viewport.top,
    100 * 28 + 5 + 252,
    "offscreen remeasurement must preserve the same row and offset",
  );
  const updated = presentLogs([
    ...entries.slice(10),
    ...Array.from({ length: 10 }, (_, i) => line(i + 800)),
  ]);
  viewport.setRows(updated);
  assert.equal(
    viewport.top,
    90 * 28 + 5 + 252,
    "ring eviction keeps the same visible entry anchored",
  );
  assert.ok(viewport.window().items.some((row) => row.id === rows[100].id));
  viewport.resize(500, 280);
  assert.equal(
    viewport.top,
    90 * 28 + 5,
    "width changes discard stale wrap heights",
  );
  viewport.latest();
  assert.equal(viewport.following, true);
  assert.equal(viewport.window().items.at(-1).message, "line 809");
  viewport.setRows([], true);
  assert.equal(viewport.top, 0);
  assert.deepEqual(viewport.window().items, []);
  viewport.setRows([rows[0]], true);
  viewport.measure([{ id: rows[0].id, height: 4000 }]);
  assert.equal(viewport.window().items.length, 1);
  assert.equal(
    viewport.window().height,
    4000,
    "long messages retain their full wrapped height",
  );
});

test("incremental presentation reuses history and preserves cross-batch duplicates, exception context and locale changes", () => {
  const entries = [
    { ...line(0), kind: "info", message: "启动游戏" },
    { ...line(1), message: "[ERROR] failed" },
  ];
  const presenter = createLogPresenter();
  let translations = 0;
  const translate = (text) => {
    translations++;
    return `en:${text}`;
  };
  const first = presenter.update(entries, translate, "en");
  entries.push({ ...line(2), message: "[ERROR] failed" });
  entries.push({
    ...line(3),
    message: "\tat com.example.Sound.play(Sound.java:12)",
  });
  const second = presenter.update(entries, translate, "en");
  assert.equal(
    translations,
    1,
    "appending must not translate historical rows again",
  );
  assert.equal(second[0], first[0]);
  assert.equal(second[1].count, 2);
  assert.equal(second[2].filterLevel, "ERROR");
  assert.equal(second[2].group, second[1].group);
  assert.deepEqual(second, presentLogs(entries, translate));
  const chinese = presenter.update(entries, (text) => text, "zh");
  assert.equal(chinese[0].message, "启动游戏");
  assert.deepEqual(presenter.update([], translate, "en"), []);
  assert.deepEqual(
    presenter.update(entries.slice(2), translate, "en"),
    presentLogs(entries.slice(2), translate),
  );
});
