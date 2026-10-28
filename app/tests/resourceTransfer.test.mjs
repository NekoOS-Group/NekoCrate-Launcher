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
      dataset: { resourceCopyInstance: instance, resourceCopyVersion: version },
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
