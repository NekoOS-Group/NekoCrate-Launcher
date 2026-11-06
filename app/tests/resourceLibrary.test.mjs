import assert from "node:assert/strict";
import test from "node:test";
import {
  cacheResourceParts,
  createResourceLibraryRpc,
} from "../src/resourceLibrary.ts";
import { resourceCopyDestination } from "../src/useResourceTransfer.ts";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";

test("each cache category has a destination and library presentation never queries a fake instance", async () => {
  const calls = [];
  const rpc = createResourceLibraryRpc(async (op, data) => {
    calls.push({ op, data });
    return "ok";
  });
  assert.equal(cacheResourceParts.java.tab, "java");
  assert.equal(cacheResourceParts.skins.tab, "skins");
  for (const kind of ["mods", "resourcepacks", "shaderpacks", "datapacks"])
    assert.equal(cacheResourceParts[kind].tab, "resources");
  for (const op of [
    "version_content_metadata",
    "version_content_presentation",
    "version_content_source",
    "open_content_folder",
  ]) {
    await rpc(op, {
      id: "",
      version: "",
      kind: "mods",
      file: "cat.jar",
      source: "fabric.mod.json",
    });
    assert.equal(calls.at(-1).op, "resource_manager");
    assert.equal(calls.at(-1).data.action.content, "mods");
    await rpc(op, {
      id: "real",
      version: "1.20.1",
      kind: "mods",
      file: "cat.jar",
    });
    assert.equal(calls.at(-1).op, op);
  }
  await rpc("project_details", { platform: "modrinth", project: "cat" });
  assert.equal(calls.at(-1).op, "project_details");
});

function libraryElement(kind) {
  return {
    dataset: { dropKind: kind, resourceLibrary: "" },
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }),
    closest(selector) {
      return selector.includes("data-resource-library") ||
        selector.includes("data-drop-kind")
        ? this
        : null;
    },
  };
}
test("global destinations accept matching resources without games, never misroute types, and reject duplicate self-copy", () => {
  const source = {
    kind: "mods",
    instance: "game",
    version: "1.20.1",
    file: "cat.jar",
    name: "Cat",
  };
  assert.deepEqual(
    resourceCopyDestination(libraryElement("mods"), source, [], []),
    { global: true, instance: "", version: "" },
  );
  assert.equal(
    resourceCopyDestination(libraryElement("shaderpacks"), source, [], []),
    undefined,
  );
  assert.equal(
    resourceCopyDestination(
      libraryElement("mods"),
      { ...source, global: true },
      [],
      [],
    ),
    undefined,
  );
  assert.deepEqual(
    resourceCopyDestination(
      libraryElement("datapacks"),
      { ...source, kind: "datapacks" },
      [],
      [],
    ),
    { global: true, instance: "", version: "" },
  );
});

test("internal collection calls the library and preserves global identity when copying back into a version", async () => {
  const f = await settingsFixture();
  try {
    let refreshed = 0;
    f.context.resourceManager.value = { refresh: async () => refreshed++ };
    f.handlers.resource_manager = () => 1;
    const source = {
      kind: "mods",
      instance: "instance-a",
      version: "1.21.1",
      file: "cat.jar",
      name: "Cat",
    };
    await f.context.copyDraggedResources([source], {
      global: true,
      instance: "",
      version: "",
    });
    assert.equal(refreshed, 1);
    assert.equal(f.context.libraryBusy.value, false);
    assert.deepEqual(f.calls.find((c) => c.op === "resource_manager").action, {
      kind: "cache_resources",
      sources: [source],
    });
    f.handlers.copy_resource = () => true;
    await f.context.copyDraggedResource(
      { ...source, global: true, instance: "", version: "" },
      { instance: "instance-b", version: "1.20.1" },
    );
    assert.equal(
      f.calls.find((c) => c.op === "copy_resource").source.global,
      true,
    );
  } finally {
    f.dispose();
  }
});

test("external library drops do not require a selected version and preserve category validation", async () => {
  const f = await settingsFixture();
  try {
    f.context.modal.value = "library";
    const drops = [];
    f.context.resourceManager.value = {
      drop: async (...args) => drops.push(args),
    };
    const element = libraryElement("mods");
    f.fixture.hovered = element;
    assert.equal(f.context.dropScope(element).library, true);
    // Exercise the actual native drop callback with the existing inspection pipeline.
    f.handlers.inspect_resources = () => [
      {
        path: "/cat.jar",
        name: "cat.jar",
        format: "resource",
        kinds: ["mods"],
        issue: null,
      },
    ];
    f.fileDrop({
      payload: {
        type: "drop",
        paths: ["/cat.jar"],
        position: { x: 10, y: 10 },
      },
    });
    await flush();
    assert.deepEqual(drops, [[["/cat.jar"], "mods"]]);
    assert.equal(
      f.calls.some((c) => c.op === "prepare_resource_import"),
      false,
    );
  } finally {
    f.dispose();
  }
});
