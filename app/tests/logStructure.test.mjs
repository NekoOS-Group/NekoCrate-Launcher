import assert from "node:assert/strict";
import test from "node:test";
import {
  createLogPresenter,
  filterLogs,
  copyLogRows,
} from "../src/logPresentation.ts";
import {
  createLogOutline,
  visibleLogNodes,
  logBranch,
} from "../src/logStructure.ts";

const line = (message, extra = {}) => ({
  time: "20:25:51",
  kind: "log",
  instanceId: "a",
  message,
  ...extra,
});
const plain = (message, source = "Sodium", level = "WARN") =>
  line(`[main/${level}] [${source}]: ${message}`);
const mixin = (name) =>
  `Force-disabling mixin 'features.render.world.sky.${name}' as rule 'mixin.features.render.world.sky' (added by mods [iris]) disables it and children`;
const display = (roots, expansion = new Map(), searching = false) =>
  visibleLogNodes(roots, expansion, searching).map((node) => node.text);

test("consecutive same-template warnings fold with all distinct messages, counts and raw order intact", () => {
  const presenter = createLogPresenter(),
    outline = createLogOutline();
  const lines = [plain(mixin("ClientLevelMixin"))];
  let rows = presenter.update(lines);
  const id = rows[0].id;
  outline.build(rows);
  lines.push(
    plain(mixin("FogRendererMixin")),
    plain(mixin("FogRendererMixin")),
  );
  rows = presenter.update(lines);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].id, id);
  assert.equal(rows[0].count, 3);
  assert.deepEqual(
    rows[0].variants.map((v) => v.count),
    [1, 2],
  );
  const roots = outline.build(rows);
  assert.deepEqual(display(roots), [mixin("ClientLevelMixin")]);
  assert.deepEqual(display(roots, new Map([[id, true]])), [
    mixin("ClientLevelMixin"),
    mixin("FogRendererMixin"),
  ]);
  assert.equal(
    filterLogs(rows, {
      scope: "a",
      source: "Sodium · main",
      level: "warning",
      query: "FogRendererMixin",
    }).length,
    1,
  );
  assert.deepEqual(display(roots, new Map(), true), [
    mixin("ClientLevelMixin"),
    mixin("FogRendererMixin"),
  ]);
  assert.equal(
    copyLogRows(rows),
    lines.map((l) => `[${l.time}] ${l.message}`).join("\n"),
  );
  // Severity, sender, instance and interleaving are real grouping boundaries.
  for (const extra of [
    { message: `[main/ERROR] [Sodium]: ${mixin("OtherMixin")}` },
    { message: `[main/WARN] [Iris]: ${mixin("OtherMixin")}` },
    { instanceId: "b" },
  ]) {
    const pair = createLogPresenter().update([
      plain(mixin("ClientLevelMixin")),
      { ...plain(mixin("FogRendererMixin")), ...extra },
    ]);
    assert.equal(pair.length, 2);
  }
  assert.equal(
    createLogPresenter().update([
      plain("Failed with code 12"),
      plain("Failed with code 13"),
      plain("Missing 'first mod'"),
      plain("Missing 'second mod'"),
      plain(mixin("ClientLevelMixin"), "Sodium", "ERROR"),
      plain(mixin("FogRendererMixin"), "Sodium", "ERROR"),
    ]).length,
    6,
    "do not approximate different errors, numeric values or ordinary prose",
  );
});

test("divider-bounded announcements become one message across batches, without swallowing later logs", () => {
  const presenter = createLogPresenter(),
    outline = createLogOutline();
  const body = [
    "[KONKRETE] Loading v1.9.9 in client-side mode on FABRIC!",
    "---------------------------",
    "KONKRETE SHIPS AND USES THE FOLLOWING LIBRARIES:",
    " ",
    "Open Imaging Copyright © 2014 Dhyan Blum.",
    "Open Imaging is licensed under Apache-2.0.",
    " ",
    "JsonPath Copyright © 2017 Jayway.",
    "JsonPath is licensed under Apache-2.0.",
    "---------------------------",
    "[KONKRETE] Server-side modules initialized and ready to use!",
  ];
  const lines = body.map((message) =>
    plain(message, "de.keksuccino.konkrete.Konkrete", "INFO"),
  );
  outline.build(presenter.update(lines.slice(0, 5)));
  const rows = presenter.update(lines);
  assert.equal(rows.length, 3);
  assert.equal(rows[1].count, 1);
  assert.ok(rows[1].block);
  assert.equal(rows[1].message, body.slice(2, 9).join("\n"));
  assert.equal(
    copyLogRows(rows),
    lines.map((l) => `[${l.time}] ${l.message}`).join("\n"),
  );
  const roots = outline.build(rows);
  assert.equal(display(roots).length, 3);
  const expanded = display(roots, new Map([[rows[1].id, true]]));
  assert.equal(expanded.length, 5);
  assert.ok(expanded[2].includes("Copyright © 2014"));
  assert.ok(expanded[2].includes("licensed under Apache-2.0"));
  for (const change of [
    { time: "20:26:00" },
    { instanceId: "b" },
    { message: "[other/INFO]: another sender" },
    { message: "[main/ERROR] [de.keksuccino.konkrete.Konkrete]: failure" },
  ]) {
    const interrupted = createLogPresenter().update([
      lines[1],
      lines[2],
      { ...lines[4], ...change },
    ]);
    assert.equal(interrupted.length, 2);
  }
});

test("ASCII, Unicode and tab-indented trees retain hierarchy, sender context, search and stable folding across appends", () => {
  assert.deepEqual(logBranch("│  ├── child 1"), { indent: 3, text: "child 1" });
  assert.equal(logBranch("message |-- ordinary text"), undefined);
  assert.equal(logBranch("\tat example.Main.run(Main.java:2)"), undefined);
  const presenter = createLogPresenter(),
    outline = createLogOutline();
  const lines = [
    line("[main/INFO]: Loading 3 mods:"),
    line("\t- parent 1.0"),
    line("\t  |-- child 2.0"),
    line("\t  \\-- other 3.0"),
    line("\t- sibling 1.0"),
  ];
  let rows = presenter.update(lines);
  let roots = outline.build(rows);
  assert.equal(roots.length, 1);
  assert.deepEqual(
    roots[0].children.map((n) => n.text),
    ["parent 1.0", "sibling 1.0"],
  );
  assert.equal(roots[0].children[0].children.length, 2);
  assert.equal(roots[0].descendants, 4);
  const id = roots[0].children[0].id;
  assert.deepEqual(display(roots, new Map([[id, false]])), [
    "Loading 3 mods:",
    "parent 1.0",
    "sibling 1.0",
  ]);
  const kept = roots[0].children[0];
  lines.push(line("\t- last 1.0"), line("[main/INFO]: Ready"));
  rows = presenter.update(lines);
  roots = outline.build(rows);
  assert.equal(roots[0].children[0], kept);
  assert.equal(roots[0].descendants, 5);
  assert.equal(roots.at(-1).text, "Ready");
  const matches = filterLogs(rows, {
    scope: "a",
    source: "main",
    level: "info",
    query: "child",
  });
  assert.equal(
    matches.length,
    6,
    "keep the matching branch's header and hierarchy",
  );
  assert.equal(display(outline.build(matches, "query")).length, 6);
  assert.equal(copyLogRows(rows).split("\n").length, lines.length);
  assert.deepEqual(outline.build([], "clear"), []);
});

test("sender filtering keeps unlabelled exception frames and separates logger/thread pairs", () => {
  const rows = createLogPresenter().update([
    line(
      "[Render thread/ERROR] [com.mojang.authlib.yggdrasil.YggdrasilAuthenticationService]: Authentication failed",
    ),
    line("\tat example.Main.run(Main.java:2)"),
    line(
      "[worker/INFO] [com.mojang.authlib.yggdrasil.YggdrasilAuthenticationService]: Ready",
    ),
  ]);
  const filtered = filterLogs(rows, {
    scope: "all",
    source:
      "com.mojang.authlib.yggdrasil.YggdrasilAuthenticationService · Render thread",
    level: "all",
    query: "",
  });
  assert.equal(filtered.length, 2);
  assert.equal(filtered[1].detail, "stack");
  const stderr = createLogPresenter().update([
    line("[main/ERROR] [STDERR]: [example.Main:run:1]: OpenAL failure"),
    line(
      "[main/INFO] [STDERR]: [example.Main:run:2]: \tat example.Main.run(Main.java:2)",
    ),
  ]);
  assert.equal(
    filterLogs(stderr, { scope: "a", level: "error", query: "OpenAL" }).length,
    2,
    "different stderr call sites must not split exception context",
  );
});
