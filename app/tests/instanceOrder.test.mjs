import assert from "node:assert/strict";
import test from "node:test";
import {
  instanceDropTarget,
  moveInstanceBefore,
} from "../src/instanceOrder.ts";

const items = ["a", "b", "c", "d"].map((id) => ({
  id,
  selected_version: `version-${id}`,
}));
const ids = (list) => list.map((item) => item.id);

test("前移、后移及移至末尾保持实例身份、版本和其他实例的相对顺序", () => {
  assert.deepEqual(ids(moveInstanceBefore(items, "d", "a")), [
    "d",
    "a",
    "b",
    "c",
  ]);
  assert.deepEqual(ids(moveInstanceBefore(items, "a", "d")), [
    "b",
    "c",
    "a",
    "d",
  ]);
  assert.deepEqual(ids(moveInstanceBefore(items, "a", null)), [
    "b",
    "c",
    "d",
    "a",
  ]);
  for (const source of items) {
    for (const target of [...items.map((item) => item.id), null]) {
      const moved = moveInstanceBefore(items, source.id, target);
      assert.equal(moved.length, items.length);
      assert.equal(new Set(ids(moved)).size, items.length);
      for (const item of moved)
        assert.equal(
          item,
          items.find((original) => original.id === item.id),
        );
      assert.deepEqual(
        ids(moved.filter((item) => item.id !== source.id)),
        ids(items.filter((item) => item.id !== source.id)),
      );
    }
  }
  assert.deepEqual(ids(items), ["a", "b", "c", "d"]);
});

test("自身位置、拖动期间被删除的来源或目标不会丢弃现有实例", () => {
  for (const [id, before] of [
    ["a", "a"],
    ["missing", "a"],
    ["a", "missing"],
  ]) {
    assert.deepEqual(moveInstanceBefore(items, id, before), items);
  }
});

test("拖放以相邻实例中心为分界，忽略拖动来源，并使用滚动后的视口坐标", () => {
  const positions = items.map((item, index) => ({
    id: item.id,
    left: 100 + 60 * index,
    width: 56,
  }));
  assert.equal(instanceDropTarget(positions, "b", 100), "a");
  assert.equal(instanceDropTarget(positions, "b", 128), "c");
  assert.equal(instanceDropTarget(positions, "b", 247), "c");
  assert.equal(instanceDropTarget(positions, "b", 248), "d");
  assert.equal(instanceDropTarget(positions, "b", 340), null);
  const scrolled = positions.map((position) => ({
    ...position,
    left: position.left - 120,
  }));
  assert.equal(instanceDropTarget(scrolled, "b", 128), "d");
});

test("相邻实例让位后，静止指针不会导致顺序来回抖动", () => {
  const positions = (list) =>
    list.map((item, index) => ({
      id: item.id,
      left: 100 + 60 * index,
      width: 56,
    }));
  for (const source of items) {
    for (let x = 85; x <= 355; x += 3) {
      const before = instanceDropTarget(positions(items), source.id, x);
      const preview = moveInstanceBefore(items, source.id, before);
      const nextTarget = instanceDropTarget(positions(preview), source.id, x);
      assert.deepEqual(
        ids(moveInstanceBefore(items, source.id, nextTarget)),
        ids(preview),
      );
    }
  }
});
