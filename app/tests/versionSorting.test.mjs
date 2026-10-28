import assert from "node:assert/strict";
import test from "node:test";
import { versionDropTarget } from "../src/useVersionSorting.ts";
import { moveInstanceBefore } from "../src/instanceOrder.ts";
import { settingsFixture } from "./helpers/appSettingsFixture.mjs";

test("vertical insertion remains stable after neighbors move, including both ends", () => {
  const items = ["a", "b", "c", "d"].map((id) => ({ id }));
  const positions = (list) =>
    list.map((v, i) => ({ id: v.id, top: 100 + i * 56, height: 56 }));
  for (const source of items)
    for (let y = 85; y < 350; y += 3) {
      const target = versionDropTarget(positions(items), source.id, y);
      const reordered = moveInstanceBefore(items, source.id, target);
      const next = versionDropTarget(positions(reordered), source.id, y);
      assert.deepEqual(moveInstanceBefore(items, source.id, next), reordered);
    }
  assert.equal(versionDropTarget(positions(items), "c", 80), "a");
  assert.equal(versionDropTarget(positions(items), "a", 400), null);
});

test("version moves persist per instance, preserve selection, and roll back only order on failure", async () => {
  const { fixture: f, context: ui, dispose } = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions = ["a", "b", "c"].map((id) => ({ id, installed: false }));
    instance.selected_version = "b";
    await ui.sync();
    f.handlers.move_version = ({ id, version, before_id }) => {
      assert.equal(id, instance.id);
      instance.versions = moveInstanceBefore(
        instance.versions,
        version,
        before_id,
      );
      return true;
    };
    assert.equal(await ui.reorderVersion(instance.id, "c", "a"), true);
    assert.deepEqual(
      ui.instances.value[0].versions.map((v) => v.id),
      ["c", "a", "b"],
    );
    assert.equal(ui.instances.value[0].selected_version, "b");
    f.handlers.move_version = () => {
      throw new Error("stale target");
    };
    assert.equal(await ui.reorderVersion(instance.id, "b", "c"), false);
    assert.deepEqual(
      ui.instances.value[0].versions.map((v) => v.id),
      ["c", "a", "b"],
    );
    assert.equal(ui.error.value, "stale target");
    assert.equal(ui.versionOrderSaving.value, false);
  } finally {
    dispose();
  }
});

test("version trash keeps failed deletes, blocks running games, and accepts deleting the last version", async () => {
  const { fixture: f, context: ui, dispose } = await settingsFixture();
  try {
    const instance = f.state.database.instances[0],
      version = instance.versions[0].id;
    await ui.sync();
    f.handlers.delete_version = () => {
      throw new Error("dependent version");
    };
    assert.equal(await ui.removeDraggedVersion(instance.id, version), false);
    assert.equal(ui.instances.value[0].versions.length, 1);
    f.state.running = [instance.id];
    await ui.sync();
    const calls = f.calls.filter((c) => c.op === "delete_version").length;
    assert.equal(await ui.removeDraggedVersion(instance.id, version), false);
    assert.equal(
      f.calls.filter((c) => c.op === "delete_version").length,
      calls,
    );
    f.state.running = [];
    await ui.sync();
    f.handlers.delete_version = ({ id, version: source }) => {
      assert.equal(id, instance.id);
      assert.equal(source, version);
      instance.versions = [];
      instance.selected_version = null;
      return structuredClone(instance);
    };
    assert.equal(await ui.removeDraggedVersion(instance.id, version), true);
    assert.equal(ui.instances.value[0].versions.length, 0);
    assert.equal(ui.instances.value[0].selected_version, null);
    assert.equal(ui.busy.value, false);
  } finally {
    dispose();
  }
});
