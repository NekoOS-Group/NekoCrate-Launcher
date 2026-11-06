import assert from "node:assert/strict";
import test from "node:test";
import {
  fitWindow,
  resizeWindow,
  snapAt,
  snappedWindow,
  validWindowLayout,
} from "../src/windowGeometry.ts";
import {
  focusToolWindow,
  raiseToolWindow,
  registerToolWindow,
  toolOrder,
} from "../src/toolWindows.ts";
import {
  activateDialog,
  activeDialog,
  registerDialog,
} from "../src/dialogSurface.ts";
import {
  settingsFixture,
  managerActions,
  flush,
  deferred,
} from "./helpers/appSettingsFixture.mjs";

test("resizing from all edges preserves opposite anchors, minimum size and visible bounds", () => {
  const area = { width: 1400, height: 900 };
  const start = { x: 250, y: 180, width: 700, height: 500 };
  for (const edge of ["n", "e", "s", "w", "ne", "nw", "se", "sw"]) {
    for (const delta of [-2000, -80, 100, 2000]) {
      const rect = resizeWindow(start, edge, delta, delta, area);
      assert.ok(rect.x >= 12 && rect.y >= 12);
      assert.ok(rect.x + rect.width <= area.width - 12);
      assert.ok(rect.y + rect.height <= area.height - 12);
      assert.ok(rect.width >= 340 && rect.height >= 240);
      if (edge.includes("w"))
        assert.equal(rect.x + rect.width, start.x + start.width);
      if (edge.includes("n"))
        assert.equal(rect.y + rect.height, start.y + start.height);
    }
  }
});
test("snap zones preview two nonoverlapping halves, maximize on top and adapt to smaller viewports", () => {
  const area = { width: 1200, height: 800 };
  assert.equal(snapAt(5, 400, area), "left");
  assert.equal(snapAt(1190, 400, area), "right");
  assert.equal(snapAt(600, 10, area), "maximized");
  assert.equal(snapAt(600, 300, area), undefined);
  const left = snappedWindow("left", area),
    right = snappedWindow("right", area);
  assert.equal(left.x + left.width + 12, right.x);
  assert.equal(right.x + right.width, area.width - 12);
  const small = { width: 600, height: 400 };
  assert.deepEqual(
    snappedWindow("left", small),
    snappedWindow("maximized", small),
  );
  assert.deepEqual(
    fitWindow({ x: 1500, y: 1500, width: 900, height: 800 }, small),
    { x: 12, y: 12, width: 576, height: 376 },
  );
  assert.equal(validWindowLayout({ rect: { ...left, x: NaN } }), false);
  assert.equal(validWindowLayout({ rect: left, snap: "left" }), true);
});
test("raising and closing tools preserves focus order while native confirmations retain priority", () => {
  const focused = [];
  const a = {},
    b = {},
    modal = {};
  const cleanA = registerDialog(a, false),
    cleanB = registerDialog(b, false);
  const unregisterA = registerToolWindow({
    id: "test-a",
    focus: () => focused.push("a"),
    close() {},
  });
  const unregisterB = registerToolWindow({
    id: "test-b",
    focus: () => focused.push("b"),
    close() {},
  });
  const cleanModal = registerDialog(modal);
  try {
    focusToolWindow("test-a");
    activateDialog(a);
    assert.deepEqual(toolOrder.value, ["test-b", "test-a"]);
    assert.equal(activeDialog.value, modal);
    assert.deepEqual(focused, ["a"]);
    assert.equal(raiseToolWindow("missing"), false);
    cleanModal();
    assert.equal(activeDialog.value, a);
    unregisterA();
    assert.equal(focused.at(-1), "b");
  } finally {
    cleanModal();
    cleanA();
    cleanB();
    unregisterA();
    unregisterB();
  }
  assert.equal(toolOrder.value.length, 0);
  assert.equal(activeDialog.value, null);
});
test("library, logs, tasks and two managers coexist; same-target opens reuse state and unrelated editors cannot redirect writes", async () => {
  const f = await settingsFixture();
  const ui = f.context;
  try {
    f.state.database.instances.push({
      ...f.state.database.instances[0],
      id: "instance-b",
      name: "B",
    });
    await ui.sync();
    const [a, b] = ui.instances.value;
    const wa = ui.manageVersions(a),
      wb = ui.manageVersions(b);
    ui.openResourceManager();
    ui.openLogs();
    ui.openDownloads();
    assert.equal(ui.managedWindows.value.length, 2);
    assert.equal(ui.libraryOpen.value, true);
    assert.equal(ui.showLogs.value, true);
    assert.equal(ui.showDownloads.value, true);
    assert.equal(ui.manageVersions(a).id, wa.id);
    assert.equal(ui.managedWindows.value.length, 2);
    ui.editing.value = { id: "unrelated-form" };
    const write = deferred();
    f.handlers.rename_version = () => write.promise;
    const renamed = managerActions(ui, wa).renameVersion("1.21.1", "A renamed");
    ui.switchManagedInstance(wa.id, b.id);
    assert.equal(
      wa.instanceId,
      a.id,
      "an already open target gets focus, not a duplicate editor",
    );
    assert.equal(f.calls.findLast((c) => c.op === "rename_version").id, a.id);
    write.resolve(true);
    await renamed;
    ui.setManagedBusy(wa.id, true);
    ui.closeManagedWindow(wa.id);
    assert.equal(ui.managedWindows.value.length, 2);
    ui.closeManagedWindow(wb.id);
    assert.equal(ui.managedWindows.value.length, 1);
    assert.equal(ui.showDownloads.value, true);
    ui.setManagedBusy(wa.id, false);
    f.state.database.instances = f.state.database.instances.filter(
      (item) => item.id === b.id,
    );
    await ui.sync();
    await flush();
    assert.equal(ui.managedWindows.value.length, 0);
    assert.equal(ui.libraryOpen.value, true);
  } finally {
    f.dispose();
  }
});
