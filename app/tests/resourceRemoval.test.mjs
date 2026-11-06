import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, getCurrentInstance } from "vue";
import {
  useResourceTransfer,
  resourceTransferKey,
} from "../src/useResourceTransfer.ts";
import { dropHint } from "../src/dropSurface.ts";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";

const mod = {
  kind: "mods",
  instance: "a",
  version: "one",
  file: "cat.jar.disabled",
  name: "Cat",
};
const world = { ...mod, kind: "saves", file: "My World", name: "My World" };
test("declared pack Mods require confirmation before deleting the entire batch", async () => {
  for (const accept of [false, true]) {
    const f = await settingsFixture();
    try {
      f.handlers.remove_resources = ({ confirmed }) =>
        confirmed
          ? { removed: [mod, world], failed: [] }
          : { removed: [], failed: [], confirmation_required: [mod] };
      const removal = f.context.removeResources([mod, world]);
      await flush();
      assert.deepEqual(f.context.resourceRemovalConfirmation.value.names, [
        "Cat",
      ]);
      assert.equal(f.context.libraryBusy.value, true);
      assert.equal(
        f.calls.filter((c) => c.op === "remove_resources").length,
        1,
      );
      f.context.finishResourceRemovalConfirmation(accept);
      await removal;
      const calls = f.calls.filter((c) => c.op === "remove_resources");
      assert.equal(calls.length, accept ? 2 : 1);
      if (accept) {
        assert.equal(calls[1].confirmed, true);
        assert.deepEqual(calls[1].sources, [mod, world]);
      }
      assert.equal(f.context.resourceRemovalConfirmation.value, undefined);
      assert.equal(f.context.libraryBusy.value, false);
    } finally {
      f.dispose();
    }
  }
});
function dragFixture() {
  const previous = Object.fromEntries(
    ["window", "document", "requestAnimationFrame", "cancelAnimationFrame"].map(
      (key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)],
    ),
  );
  const listeners = new Map(),
    removed = [],
    copied = [],
    exported = [];
  const state = { running: [], busy: false, hovered: null };
  globalThis.window = {
    innerWidth: 1000,
    innerHeight: 700,
    getSelection: () => ({ removeAllRanges() {} }),
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.document = {
    documentElement: { classList: { add() {}, remove() {} } },
    elementFromPoint: () => state.hovered,
    addEventListener: (event, callback) => listeners.set(event, callback),
    removeEventListener: (event) => listeners.delete(event),
  };
  globalThis.requestAnimationFrame = () => 1;
  globalThis.cancelAnimationFrame = () => {};
  let drag, start;
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      drag = useResourceTransfer({
        instances: () => [
          {
            id: "b",
            name: "Target",
            versions: [{ id: "two", installed: true }],
          },
        ],
        running: () => state.running,
        busy: () => state.busy,
        copy: async (source, target) =>
          copied.push({ sources: [source], target }),
        copyMany: async (sources, target) => copied.push({ sources, target }),
        remove: async (sources) => removed.push(sources),
        export: async (source) => exported.push(source),
      });
      start = getCurrentInstance().provides[resourceTransferKey];
      return () => null;
    },
  });
  app.mount({});
  const event = (x = 250, y = 350) => ({
    pointerId: 1,
    button: 0,
    isPrimary: true,
    clientX: x,
    clientY: y,
    preventDefault() {},
    stopImmediatePropagation() {},
    currentTarget: {},
  });
  return {
    drag,
    state,
    removed,
    copied,
    exported,
    start: (sources) => start(event(), sources),
    move: (x, y) => listeners.get("pointermove")?.(event(x, y)),
    end: (x, y) => listeners.get("pointerup")?.(event(x, y)),
    escape: () => listeners.get("keydown")?.({ ...event(), key: "Escape" }),
    dispose() {
      app.unmount();
      for (const [key, value] of Object.entries(previous)) {
        if (value) Object.defineProperty(globalThis, key, value);
        else delete globalThis[key];
      }
    },
  };
}

test("resource trash wins over copy targets and native export at the overlapping window edge", () => {
  const f = dragFixture();
  try {
    const cached = { ...mod, global: true, instance: "", version: "" };
    f.start([mod, cached, world]);
    f.move(978, 30);
    assert.equal(f.drag.value.target.trash, true);
    assert.equal(dropHint.value, undefined);
    f.end(978, 30);
    assert.deepEqual(f.removed, [[mod, cached, world]]);
    assert.equal(f.copied.length, 0);
    assert.equal(f.exported.length, 0);
    assert.equal(f.drag.value, undefined);
  } finally {
    f.dispose();
  }
});

test("clicks, cancellation, outside drops, and games that start during a drag never delete files", () => {
  const f = dragFixture();
  try {
    f.start(mod);
    f.end(250, 350);
    f.start(mod);
    f.move(950, 50);
    f.escape();
    f.end(950, 50);
    f.start(mod);
    f.move(500, 500);
    f.end(500, 500);
    f.start(mod);
    f.move(950, 50);
    f.state.running = ["a"];
    f.end(950, 50);
    assert.equal(f.removed.length, 0);
  } finally {
    f.dispose();
  }
});

test("installed resources browsed in the library cannot be trashed, including a mixed selection", () => {
  const f = dragFixture();
  try {
    const installed = { ...mod, removable: false };
    const cached = { ...mod, global: true, removable: true };
    for (const sources of [[installed], [installed, cached]]) {
      f.start(sources);
      f.move(950, 50);
      assert.equal(f.drag.value.target, undefined);
      f.end(950, 50);
    }
    assert.deepEqual(f.removed, []);
    assert.deepEqual(f.copied, []);
    f.state.hovered = {
      closest: () => ({
        dataset: { resourceInstance: "b", resourceVersion: "two" },
        getBoundingClientRect: () => ({
          left: 300,
          top: 400,
          width: 100,
          height: 40,
        }),
      }),
    };
    f.start(installed);
    f.move(500, 500);
    f.end(500, 500);
    assert.equal(
      f.copied.length,
      1,
      "read-only library items can still be copied",
    );
    f.start(cached);
    f.move(950, 50);
    f.end(950, 50);
    assert.deepEqual(f.removed, [[cached]]);
  } finally {
    f.dispose();
  }
});

test("the shared removal action rejects library-only installed sources before calling the backend", async () => {
  const f = await settingsFixture();
  try {
    await f.context.removeResources([
      { ...mod, removable: false },
      { ...world, global: true },
    ]);
    assert.equal(
      f.calls.some((call) => call.op === "remove_resources"),
      false,
    );
    assert.equal(f.context.libraryBusy.value, false);
  } finally {
    f.dispose();
  }
});

test("world drag copies to an explicit version and hands a folder to the native backup drag", async () => {
  const f = dragFixture();
  try {
    f.state.hovered = {
      closest: () => ({
        dataset: { resourceInstance: "b", resourceVersion: "two" },
        getBoundingClientRect: () => ({
          left: 300,
          top: 400,
          width: 100,
          height: 40,
        }),
      }),
    };
    f.start(world);
    f.move(500, 500);
    assert.match(dropHint.value.message, /My World|Target/);
    f.end(500, 500);
    assert.deepEqual(f.copied, [
      { sources: [world], target: { instance: "b", version: "two" } },
    ]);
    f.start(world);
    f.move(5, 350);
    await new Promise(setImmediate);
    assert.deepEqual(f.exported, [world]);
    assert.equal(f.removed.length, 0);
  } finally {
    f.dispose();
  }
});

test("deletion refreshes all open resource views, clears removed draft items, and reports individual failures", async () => {
  const f = await settingsFixture();
  try {
    let refreshed = 0,
      libraryRefreshes = 0;
    f.context.setVersionManager("a", { refresh: async () => refreshed++ });
    f.context.setVersionManager("b", { refresh: async () => refreshed++ });
    f.context.resourceManager.value = {
      refresh: async () => libraryRefreshes++,
    };
    f.context.selectedLocalResources.value = [mod, world];
    f.handlers.remove_resources = () => ({
      removed: [mod],
      failed: [{ source: world, message: "World is locked" }],
    });
    await f.context.removeResources([mod, world]);
    assert.deepEqual(f.calls.find((c) => c.op === "remove_resources").sources, [
      mod,
      world,
    ]);
    assert.equal(refreshed, 2);
    assert.equal(libraryRefreshes, 1);
    assert.deepEqual(f.context.selectedLocalResources.value, [world]);
    assert.equal(f.context.error.value, "My World: World is locked");
    assert.equal(f.context.libraryBusy.value, false);
    f.handlers.remove_resources = () => {
      throw Error("Recycle bin unavailable");
    };
    await f.context.removeResources([world]);
    assert.equal(f.context.error.value, "Recycle bin unavailable");
    assert.equal(f.context.libraryBusy.value, false);
    assert.equal(refreshed, 2);
  } finally {
    f.dispose();
  }
});
