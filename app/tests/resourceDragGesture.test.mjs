import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, getCurrentInstance } from "vue";
import {
  useResourceTransfer,
  resourceTransferKey,
} from "../src/useResourceTransfer.ts";
import { dropInteraction, dropHint } from "../src/dropSurface.ts";

function fixture(exportResult) {
  const previous = Object.fromEntries(
    ["window", "document", "requestAnimationFrame", "cancelAnimationFrame"].map(
      (key) => [key, globalThis[key]],
    ),
  );
  const listeners = new Map();
  const target = {
    dataset: {
      resourceInstance: "b",
      resourceVersion: "two",
      dropKind: "version",
    },
    getBoundingClientRect: () => ({
      left: 30,
      top: 50,
      width: 200,
      height: 40,
    }),
  };
  let hovered = { closest: () => target };
  globalThis.document = {
    documentElement: { classList: { add() {}, remove() {} } },
    elementFromPoint: () => hovered,
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: (name, callback) => {
      if (listeners.get(name) === callback) listeners.delete(name);
    },
  };
  globalThis.window = {
    innerWidth: 900,
    innerHeight: 600,
    getSelection: () => ({ removeAllRanges() {} }),
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.requestAnimationFrame = () => 1;
  globalThis.cancelAnimationFrame = () => {};
  let drag, start;
  const copied = [],
    exported = [],
    errors = [];
  let finish = 0;
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
          { id: "b", versions: [{ id: "two", installed: true }] },
        ],
        running: () => [],
        busy: () => false,
        copy: async (...args) => {
          copied.push(args);
        },
        export: async (source) => {
          exported.push(source);
          if (exportResult instanceof Error) throw exportResult;
          return exportResult;
        },
        finishBrowse: () => finish++,
        error: (error) => errors.push(error),
      });
      start = getCurrentInstance().provides[resourceTransferKey];
      return () => null;
    },
  });
  app.mount({});
  const element = { setPointerCapture() {}, hasPointerCapture: () => false };
  const event = (x, y) => ({
    button: 0,
    isPrimary: true,
    pointerId: 4,
    clientX: x,
    clientY: y,
    currentTarget: element,
    preventDefault() {},
  });
  return {
    drag,
    copied,
    exported,
    errors,
    start: () =>
      start(event(100, 120), {
        kind: "mods",
        instance: "a",
        version: "one",
        file: "cat.jar",
        name: "Cat",
      }),
    move: (x, y) => listeners.get("pointermove")?.(event(x, y)),
    release: (x, y) => listeners.get("pointerup")?.(event(x, y)),
    finish: () => finish,
    dispose() {
      app.unmount();
      Object.assign(globalThis, previous);
    },
  };
}

test("internal drag uses the existing target and cleans up on release", () => {
  const f = fixture();
  try {
    f.start();
    f.move(150, 120);
    assert.equal(dropInteraction.value, true);
    assert.equal(dropHint.value.invalid, false);
    f.release(150, 120);
    assert.equal(f.copied.length, 1);
    assert.deepEqual(f.copied[0][1], { instance: "b", version: "two" });
    assert.equal(dropInteraction.value, false);
    assert.equal(f.drag.value, undefined);
    assert.equal(f.finish(), 1);
  } finally {
    f.dispose();
  }
});

test("leaving the window hands a listed resource to native export; returning drops only once", async () => {
  let resolve;
  const native = new Promise((done) => {
    resolve = done;
  });
  const f = fixture(native);
  try {
    f.start();
    f.move(150, 120);
    f.move(2, 120);
    assert.equal(f.exported.length, 1);
    assert.equal(f.drag.value.native, true);
    f.release(2, 120);
    assert.equal(f.copied.length, 0);
    assert.equal(f.drag.nativeEvent("over", 150, 120), true);
    assert.equal(f.drag.nativeEvent("drop", 150, 120), true);
    resolve(true);
    await new Promise(setImmediate);
    assert.equal(f.copied.length, 1);
    assert.equal(f.finish(), 1);
    assert.equal(f.drag.value, undefined);
  } finally {
    f.dispose();
  }
});

test("native cancel or failure leaves no drag state or accidental internal copy", async () => {
  for (const result of [false, new Error("native unavailable")]) {
    const f = fixture(result);
    try {
      f.start();
      f.move(2, 120);
      await new Promise(setImmediate);
      assert.equal(f.copied.length, 0);
      assert.equal(f.drag.value, undefined);
      assert.equal(dropInteraction.value, false);
    } finally {
      f.dispose();
    }
  }
});
