import assert from "node:assert/strict";
import test from "node:test";
import { readHomeTools, sanitizeHomeTools } from "../src/homeToolbar.ts";
test("toolbar migration preserves order, rejects unknown entries and restores missing settings", () => {
  assert.deepEqual(readHomeTools(null), ["resources", "settings", "logs"]);
  assert.deepEqual(readHomeTools('{"version":1,"tools":[]}'), ["settings"]);
  assert.deepEqual(readHomeTools('{"version":1,"tools":["logs","store"]}'), [
    "logs",
    "store",
    "settings",
  ]);
  assert.deepEqual(
    readHomeTools(
      '{"version":1,"tools":["store","logs","store","invalid","settings"]}',
    ),
    ["store", "logs", "settings"],
  );
  for (const value of [
    "broken",
    '{"version":1,"tools":null}',
    '{"version":2,"tools":["store"]}',
  ])
    assert.deepEqual(readHomeTools(value), ["resources", "settings", "logs"]);
  assert.deepEqual(sanitizeHomeTools(["__proto__", null, "logs"]), [
    "logs",
    "settings",
  ]);
  assert.deepEqual(sanitizeHomeTools(["settings", "logs"]), [
    "settings",
    "logs",
  ]);
});
