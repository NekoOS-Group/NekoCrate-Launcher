import assert from "node:assert/strict";
import test from "node:test";
import {
  createLogPresenter,
  presentLogs,
  filterLogs,
  copyLogRows,
} from "../src/logPresentation.ts";
const line = (message, instanceId = "a", kind = "log") => ({
  message,
  instanceId,
  kind,
  time: "12:34:56",
});
const header =
  '<log4j:Event logger="net.minecraft.Main" timestamp="12345" level="ERROR" thread="Render thread">';

test("multiline Log4j events show message, severity, logger, thread and stack without XML structure", () => {
  const lines = [
    header,
    "  <log4j:Message><![CDATA[Could not load &amp; test]]></log4j:Message>",
    "  <log4j:Throwable><![CDATA[java.lang.IllegalStateException: broken",
    "\tat example.Main.run(Main.java:42)",
    "Caused by: java.io.IOException: missing",
    "]]></log4j:Throwable>",
    "</log4j:Event>",
  ].map((text) => line(text));
  const raw = structuredClone(lines),
    presenter = createLogPresenter();
  assert.equal(presenter.update(lines.slice(0, 3)).length, 0);
  const rows = presenter.update(lines);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].level, "ERROR");
  assert.equal(rows[0].source, "net.minecraft.Main · Render thread");
  assert.equal(
    rows[0].message,
    "Could not load &amp; test\njava.lang.IllegalStateException: broken\n\tat example.Main.run(Main.java:42)\nCaused by: java.io.IOException: missing",
  );
  assert.equal(
    filterLogs(rows, { scope: "a", level: "error", query: "example.Main" })
      .length,
    1,
  );
  assert.equal(
    copyLogRows(rows),
    lines.map((l) => `[${l.time}] ${l.message}`).join("\n"),
  );
  assert.deepEqual(lines, raw);
  const next = presenter.update([...lines, line("[INFO] ready")]);
  assert.equal(next[0], rows[0]);
  assert.equal(next[1].message, "ready");
});

test("CDATA is literal text, XML entities decode once, and wrapper-like payload is not removed", () => {
  const rows = presentLogs([
    line(
      '<log4j:Event level="WARNING" thread="A &amp; B"><log4j:Message>&lt;img src=x onerror=alert(1)&gt; &#x1f431; &amp;lt;</log4j:Message></log4j:Event>',
    ),
    line(
      '<log4j:Event level="INFO"><log4j:Message><![CDATA[</log4j:Event> &lt;literal&gt;]]></log4j:Message></log4j:Event>',
    ),
    line("</log4j:Event>"),
    line("]]></log4j:Message>"),
    line("<img src=x onerror=alert(1)>"),
  ]);
  assert.equal(rows.length, 3);
  assert.equal(rows[0].source, "A & B");
  assert.equal(rows[0].level, "WARN");
  assert.equal(rows[0].message, "<img src=x onerror=alert(1)> 🐱 &lt;");
  assert.equal(rows[1].message, "</log4j:Event> &lt;literal&gt;");
  assert.equal(rows[2].message, "<img src=x onerror=alert(1)>");
});

test("XML state stays per game, flushes on crashes, and resets on clear or locale rebuild", () => {
  const lines = [
    line(header),
    line("[INFO] other game", "b"),
    line("<log4j:Message><![CDATA[partial body"),
    line("crashed", "a", "game_crash"),
  ];
  const presenter = createLogPresenter();
  assert.deepEqual(
    presenter.update(lines).map((r) => [r.scope, r.message]),
    [
      ["b", "other game"],
      ["a", "partial body"],
      ["a", "crashed"],
    ],
  );
  assert.deepEqual(presenter.update([]), []);
  const fresh = presenter.update([line("[INFO] fresh")]);
  assert.equal(fresh[0].message, "fresh");
  const xml = [
    line(
      '<Event xmlns="http://logging.apache.org/log4j/2.0/events" level="INFO"><Message>hello</Message></Event>',
    ),
  ];
  const first = presenter.update(xml, (text) => text, "en");
  assert.equal(first[0].message, "hello");
  assert.equal(presenter.update(xml, (text) => text, "zh")[0].id, first[0].id);
});

test("broken envelope does not swallow subsequent plaintext logs and duplicate events keep their raw records", () => {
  const compact =
    '<log4j:Event level="INFO"><log4j:Message>repeated</log4j:Message></log4j:Event>';
  const lines = [
    line(header),
    line("[WARN] unrelated stderr"),
    line(compact),
    line(compact),
  ];
  const rows = presentLogs(lines);
  assert.equal(rows.length, 2);
  assert.equal(rows[0].message, "unrelated stderr");
  assert.equal(rows[1].count, 2);
  assert.equal(rows[1].originals.length, 2);
});
