import assert from "node:assert/strict";
import test from "node:test";
import { resourceCopyDestination } from "../src/useResourceTransfer.ts";
import { settingsFixture } from "./helpers/appSettingsFixture.mjs";

test("Mod drop resolves only explicit, installed, idle destinations other than the source", () => {
  const source = {
    kind: "mods",
    instance: "a",
    version: "one",
    file: "example.jar",
  };
  const instances = ["a", "b"].map((id) => ({
    id,
    versions: [
      { id: "one", installed: true },
      { id: "two", installed: true },
      { id: "pending", installed: false },
    ],
  }));
  const element = (instance, version) => ({
    closest: () => ({
      dataset: { resourceInstance: instance, resourceVersion: version },
    }),
  });
  assert.deepEqual(
    resourceCopyDestination(element("a", "two"), source, instances, []),
    { instance: "a", version: "two" },
  );
  assert.deepEqual(
    resourceCopyDestination(element("b", "one"), source, instances, []),
    { instance: "b", version: "one" },
  );
  for (const [row, running] of [
    [element("a", "one"), []],
    [element("b", "one"), ["b"]],
    [element("a", "pending"), []],
    [element("stale", "one"), []],
    [element("a", "deleted"), []],
    [null, []],
  ]) {
    assert.equal(
      resourceCopyDestination(row, source, instances, running),
      undefined,
    );
  }
});

test("Mod copy sends a listed identity, refreshes views on success, and retains the original selection", async () => {
  const f = await settingsFixture();
  try {
    let refreshed = 0;
    f.context.versionManager.value = { refresh: async () => refreshed++ };
    f.handlers.copy_resource = () => true;
    const selected = f.context.instances.value[0].selected_version;
    const source = {
      kind: "mods",
      instance: "instance-a",
      version: "1.21.1",
      file: "example.jar.disabled",
      name: "Example",
    };
    await f.context.copyDraggedResource(source, {
      instance: "instance-b",
      version: "target",
    });
    const call = f.calls.find((c) => c.op === "copy_resource");
    assert.deepEqual(call, {
      op: "copy_resource",
      source: {
        kind: source.kind,
        instance: source.instance,
        version: source.version,
        file: source.file,
      },
      id: "instance-b",
      version: "target",
    });
    assert.equal(refreshed, 1);
    assert.equal(f.context.instances.value[0].selected_version, selected);
    f.handlers.copy_resource = () => {
      throw Error("conflicting file");
    };
    await f.context.copyDraggedResource(source, {
      instance: "instance-b",
      version: "target",
    });
    assert.equal(refreshed, 1);
    assert.equal(f.context.error.value, "conflicting file");
  } finally {
    f.dispose();
  }
});

test("a Mod cannot be copied into a resource-pack tab even when its parent is a valid target", () => {
  const source = {
    kind: "mods",
    instance: "a",
    version: "one",
    file: "a.jar",
    name: "a",
  };
  const row = { dataset: { resourceInstance: "b", resourceVersion: "two" } };
  const element = {
    closest: (selector) =>
      selector.includes("data-drop-kind")
        ? { dataset: { dropKind: "resourcepacks" } }
        : row,
  };
  const instances = [{ id: "b", versions: [{ id: "two", installed: true }] }];
  assert.equal(
    resourceCopyDestination(element, source, instances, []),
    undefined,
  );
});

test("existing dock browsing opens the folded area and a version menu without changing selected instance", async () => {
  const f = await settingsFixture();
  try {
    const ui = f.context;
    let opened = 0,
      closed = 0;
    ui.dropVersionMenu.value = {
      revealForDrop: () => opened++,
      endDrop: () => closed++,
    };
    const before = ui.selectedInstanceId.value;
    ui.dropInteraction.value = true;
    await new Promise(setImmediate);
    ui.browseDropTarget({
      closest: (selector) => (selector === "[data-drop-overflow]" ? {} : null),
    });
    await new Promise((resolve) => setTimeout(resolve, 340));
    assert.equal(ui.instanceDockExpanded.value, true);
    ui.browseDropTarget({
      closest: (selector) =>
        selector.startsWith(".instance-dock")
          ? { dataset: { resourceInstance: "instance-a" } }
          : null,
    });
    await new Promise((resolve) => setTimeout(resolve, 340));
    assert.equal(ui.dragBrowseInstance.value.id, "instance-a");
    assert.equal(opened, 1);
    assert.equal(ui.selectedInstanceId.value, before);
    ui.finishDropBrowse();
    assert.equal(ui.instanceDockExpanded.value, false);
    assert.equal(ui.dragBrowseInstance.value, undefined);
    assert.equal(closed, 1);
  } finally {
    f.dispose();
  }
});
