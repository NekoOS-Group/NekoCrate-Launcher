import assert from "node:assert/strict";
import test from "node:test";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";
import {
  parseLog,
  presentLogs,
  filterLogs,
  copyLogRows,
} from "../src/logPresentation.ts";

test("Minecraft/Forge/Fabric headers use one time column and distinct severity/source", () => {
  assert.deepEqual(
    parseLog("[14:57:43] [Client thread/INFO] [FML]: Loaded 185 mods"),
    {
      level: "INFO",
      source: "FML · Client thread",
      message: "Loaded 185 mods",
      detail: "",
    },
  );
  assert.deepEqual(
    parseLog("[14:57:43.123] [Render thread/WARN]: Low memory"),
    {
      level: "WARN",
      source: "Render thread",
      message: "Low memory",
      detail: "",
    },
  );
  assert.equal(
    parseLog("[2026-10-24 14:57:43,123] [main/ERROR] [loader]: Failed").message,
    "Failed",
  );
  assert.equal(
    parseLog("2026-10-24T14:57:43Z WARNING: Unsupported flag").level,
    "WARN",
  );
  for (const level of ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"]) {
    assert.equal(parseLog(`[${level}] message`).level, level);
    assert.equal(parseLog(`${level}: message`).message, "message");
  }
  assert.equal(
    parseLog("file contains ERROR and [14:57:43]").message,
    "file contains ERROR and [14:57:43]",
  );
  assert.equal(parseLog("file contains ERROR").level, "");
  assert.equal(parseLog("游戏已退出", "game_crash").level, "ERROR");
  assert.equal(parseLog("Retrying", "retry").level, "WARN");
});

test("stderr wrappers and terminal escapes stay readable without losing exception indentation", () => {
  const frame = parseLog(
    "[15:07:32] [Thread-11/INFO] [STDERR]: [java.lang.ThreadGroup:uncaughtException:1052]: \tat com.example.Sound.play(Sound.java:12)",
  );
  assert.equal(frame.message, "\tat com.example.Sound.play(Sound.java:12)");
  assert.equal(frame.detail, "stack");
  assert.ok(frame.source.includes("uncaughtException:1052"));
  assert.equal(
    parseLog(
      "[15:17:23] [CullThread/INFO] [STDOUT/]: [dev.example.Task:run:89]: Stopping",
    ).message,
    "Stopping",
  );
  assert.equal(
    parseLog("Caused by: java.io.IOException: bad path").detail,
    "cause",
  );
  assert.equal(
    parseLog("java.lang.IllegalStateException: broken").detail,
    "cause",
  );
  assert.equal(parseLog("\t... 8 more").detail, "stack");
  assert.equal(parseLog("\x1b[31mERROR\x1b[0m: failed\x00").level, "ERROR");
  assert.equal(
    parseLog("\x1b]8;;https://example.com\x07link\x1b]8;;\x07").message,
    "link",
  );
  assert.equal(
    parseLog("<img src=x onerror=alert(1)>").message,
    "<img src=x onerror=alert(1)>",
  );
});

test("only adjacent equal rows from the same stream merge; raw logs and row identity survive", () => {
  const make = (time, message, instanceId = "a") => ({
    time,
    message,
    kind: "log",
    instanceId,
  });
  const lines = [
    make("14:57:43", "[14:57:43] [main/INFO]: Repeated"),
    make("14:57:44", "[14:57:44] [main/INFO]: Repeated"),
    make("14:57:44", "[14:57:44] [main/INFO]: Repeated", "b"),
    make("14:57:45", "[14:57:45] [main/ERROR]: Repeated"),
    make("14:57:46", "[14:57:46] [main/INFO]: Repeated"),
  ];
  const original = structuredClone(lines);
  const rows = presentLogs(lines);
  assert.equal(rows.length, 4);
  assert.equal(rows[0].count, 2);
  assert.equal(rows[0].lastTime, "14:57:44");
  assert.equal(rows[1].count, 1);
  assert.equal(rows[2].level, "ERROR");
  assert.deepEqual(lines, original);
  assert.equal(presentLogs(lines.slice(2))[0].id, rows[1].id);
  assert.equal(presentLogs([make("now", ""), make("now", "")]).length, 2);
  const launcher = { time: "now", kind: "info", message: "启动游戏" };
  assert.equal(
    presentLogs([launcher], () => "Launch game")[0].message,
    "Launch game",
  );
  assert.equal(presentLogs([launcher])[0].message, "启动游戏");
});

test("double-clicking a running instance opens scoped logs; idle and downloading instances retain their actions", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const instance = fixture.state.database.instances[0];
    ui.snapshot.value.running = [instance.id];
    ui.utility.value = "settings";
    await ui.openInstance(instance);
    assert.equal(ui.showLogs.value, true);
    assert.equal(ui.logScope.value, instance.id);
    assert.equal(ui.utility.value, null);
    assert.equal(ui.modal.value, null);
    assert.equal(
      fixture.calls.some((call) => call.op === "instance_directory"),
      false,
    );
    const session = ui.logSession.value;
    ui.openLogs();
    assert.equal(ui.logScope.value, "all");
    assert.equal(ui.logSession.value, session + 1);
    fixture.event({
      payload: {
        kind: "game_crash",
        instance_id: instance.id,
        message: "crash",
      },
    });
    assert.equal(ui.logScope.value, instance.id);
    assert.equal(ui.logSession.value, session + 2);
    assert.equal(ui.logStore.snapshot().at(-1).kind, "game_crash");
  } finally {
    dispose();
  }
});

test("level and literal text filters preserve an exception chain and copy its original lines", () => {
  const line = (message, instanceId = "a") => ({
    time: "12:00:00",
    kind: "log",
    message,
    instanceId,
  });
  const lines = [
    line("[12:00:00] [main/INFO]: ready"),
    line("[12:00:01] [main/ERROR]: OpenAL failure"),
    line("\tat com.example.Audio.play(Audio.java:12)"),
    line("Caused by: java.lang.IllegalStateException: broken"),
    line("\t... 8 more"),
    line("[12:00:02] [main/INFO]: other instance", "b"),
    line("[12:00:03] [main/INFO]: continued"),
    line("WARNING: warning", null),
    line("DEBUG: OpenAL debug [a+b]"),
  ];
  const rows = presentLogs(lines);
  const filtered = filterLogs(rows, {
    scope: "a",
    level: "error",
    query: "OpEnAl",
  });
  assert.equal(filtered.length, 4);
  assert.equal(
    copyLogRows(filtered),
    lines
      .slice(1, 5)
      .map((l) => `[${l.time}] ${l.message}`)
      .join("\n"),
  );
  assert.equal(
    filterLogs(rows, { scope: "a", level: "debug", query: "[a+b]" }).length,
    1,
  );
  assert.equal(
    filterLogs(rows, { scope: "a", level: "debug", query: ".*" }).length,
    0,
  );
  assert.equal(
    filterLogs(rows, { scope: "launcher", level: "warning", query: "" }).length,
    1,
  );
  assert.equal(
    filterLogs(rows, { scope: "b", level: "all", query: "" }).length,
    1,
  );
  assert.equal(
    filterLogs(rows, { scope: "a", level: "info", query: "" }).length,
    2,
  );
});
