import assert from "node:assert/strict";
import test from "node:test";
import { shallowRef } from "vue";
import {
  useResourceSelection,
  changeSelectedMods,
} from "../src/useResourceSelection.ts";
const click = (options = {}) => ({
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  ...options,
});
test("range, toggle and select all stay within the visible list, and preserve a plain-click anchor", () => {
  const items = shallowRef(["a", "b", "c", "d"]);
  const s = useResourceSelection(
    () => items.value,
    (value) => value,
  );
  assert.equal(s.click(click(), "b"), false);
  s.click(click({ shiftKey: true }), "d");
  assert.deepEqual(s.selected.value, ["b", "c", "d"]);
  s.click(click({ metaKey: true }), "c");
  assert.deepEqual(s.selected.value, ["b", "d"]);
  assert.deepEqual(s.drag("b"), ["b", "d"]);
  assert.deepEqual(s.drag("a"), ["a"]);
  items.value = ["a", "b"];
  assert.deepEqual(s.drag("b"), ["b"]);
  let prevented = 0;
  const key = {
    key: "a",
    ctrlKey: true,
    preventDefault() {
      prevented++;
    },
    stopPropagation() {},
    target: { closest: () => null },
  };
  s.keyboard(key);
  assert.deepEqual(s.selected.value, ["a", "b"]);
  s.keyboard({ ...key, key: "Escape" });
  assert.equal(s.selected.value.length, 0);
  s.keyboard({ ...key, target: { closest: () => ({}) } });
  assert.equal(
    s.selected.value.length,
    0,
    "text editing keeps native select-all",
  );
  assert.equal(prevented, 2);
});
test("batch toggles continue after a failed file and skip already-matching state", async () => {
  const rows = [
    { name: "a", enabled: true },
    { name: "b", enabled: true },
    { name: "c", enabled: true },
    { name: "d", enabled: false },
  ];
  const calls = [];
  const failures = await changeSelectedMods(
    rows,
    false,
    (x) => x.enabled,
    async (x) => {
      calls.push(x.name);
      if (x.name === "b") throw new Error("locked");
      return x.name + ".disabled";
    },
    (x, file) => {
      x.file = file;
      x.enabled = false;
    },
  );
  assert.deepEqual(calls, ["a", "b", "c"]);
  assert.equal(failures.length, 1);
  assert.equal(failures[0].item.name, "b");
  assert.deepEqual(
    rows.map((x) => x.name),
    ["a", "b", "c", "d"],
  );
  assert.equal(rows[2].file, "c.disabled");
});
