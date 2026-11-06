import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, getCurrentInstance } from "vue";
import {
  useResourceTransfer,
  resourceTransferKey,
} from "../src/useResourceTransfer.ts";
import { dropInteraction, dropHint } from "../src/dropSurface.ts";

function fixture(exportResult, preview) {
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
  let frame;
  globalThis.requestAnimationFrame = (callback) => {
    frame = callback;
    return 1;
  };
  globalThis.cancelAnimationFrame = () => {
    frame = undefined;
  };
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
        preview,
        copy: async (...args) => {
          copied.push(args);
        },
        copyMany: async (...args) => {
          copied.push(args);
        },
        exportMany: async (sources) => {
          exported.push(sources);
          return exportResult;
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
    start: (sources) =>
      start(
        event(100, 120),
        sources || {
          kind: "mods",
          instance: "a",
          version: "one",
          file: "cat.jar",
          name: "Cat",
        },
      ),
    move: (x, y) => listeners.get("pointermove")?.(event(x, y)),
    release: (x, y) => listeners.get("pointerup")?.(event(x, y)),
    finish: () => finish,
    frame: () => frame?.(),
    target,
    hover: (element) => {
      hovered = element;
    },
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

test("a stationary pointer tracks changed layout and clears a target that moves away", () => {
  const f = fixture();
  try {
    f.start();
    f.move(150, 120);
    const original = dropHint.value;
    f.frame();
    assert.equal(
      dropHint.value,
      original,
      "unchanged geometry does not trigger another render",
    );
    f.target.getBoundingClientRect = () => ({
      left: 80,
      top: 90,
      width: 200,
      height: 40,
    });
    f.frame();
    assert.equal(dropHint.value.left, 80);
    assert.equal(dropHint.value.top, 90);
    f.hover(null);
    f.frame();
    assert.equal(dropHint.value, undefined);
    assert.equal(f.drag.value.target, undefined);
    f.release(150, 120);
    assert.equal(f.copied.length, 0);
  } finally {
    f.dispose();
  }
});

test("asynchronous drop validation updates the target under the current cursor", async () => {
  let reject;
  const f = fixture(
    undefined,
    () =>
      new Promise((_resolve, no) => {
        reject = no;
      }),
  );
  try {
    f.start();
    f.move(150, 120);
    assert.equal(f.drag.value.target.version, "two");
    reject(Error("Target is locked"));
    await new Promise(setImmediate);
    assert.equal(dropHint.value.invalid, true);
    assert.equal(f.drag.value.target, undefined);
    f.release(150, 120);
    assert.equal(f.copied.length, 0);
  } finally {
    f.dispose();
  }
});

test("mismatched types show no frame and cannot copy, while matching compatibility failures stay red", async () => {
  let reject;
  const f = fixture(
    undefined,
    () =>
      new Promise((_yes, no) => {
        reject = no;
      }),
  );
  try {
    f.start();
    f.move(150, 120);
    assert.ok(dropHint.value);
    f.target.dataset.dropKind = "resourcepacks";
    f.move(150, 120);
    assert.equal(dropHint.value, undefined);
    assert.equal(f.drag.value.target, undefined);
    reject(Error("Incompatible Minecraft version"));
    await new Promise(setImmediate);
    assert.equal(
      dropHint.value,
      undefined,
      "a stale compatibility result must not resurrect a wrong-type frame",
    );
    f.target.dataset.dropKind = "mods";
    f.move(150, 120);
    await new Promise(setImmediate);
    assert.equal(dropHint.value.invalid, true);
    assert.match(dropHint.value.message, /Incompatible Minecraft/);
    f.target.dataset.dropKind = "config";
    f.move(150, 120);
    assert.equal(dropHint.value, undefined);
    f.release(150, 120);
    assert.deepEqual(f.copied, []);
  } finally {
    f.dispose();
  }
});

test("native leave does not restore a stale highlight on the next frame", async () => {
  let resolve;
  const f = fixture(
    new Promise((done) => {
      resolve = done;
    }),
  );
  try {
    f.start();
    f.move(2, 120);
    f.drag.nativeEvent("over", 150, 120);
    assert.ok(dropHint.value);
    f.drag.nativeEvent("leave", 150, 120);
    f.frame();
    assert.equal(dropHint.value, undefined);
    assert.equal(f.drag.value.target, undefined);
    resolve(false);
    await new Promise(setImmediate);
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

const batch = ["cat.jar", "ice.jar.disabled"].map((file) => ({
  kind: "mods",
  instance: "a",
  version: "one",
  file,
  name: file,
}));
test("selected resources copy as one batch and native export receives the entire selection", async () => {
  const f = fixture();
  try {
    f.start(batch);
    f.move(150, 150);
    assert.equal(f.drag.value.sources.length, 2);
    f.release(150, 150);
    await new Promise(setImmediate);
    assert.equal(f.copied.length, 1);
    assert.deepEqual(f.copied[0][0], batch);
  } finally {
    f.dispose();
  }
  const g = fixture({ action: "copy" });
  try {
    g.start(batch);
    g.move(895, 150);
    await new Promise(setImmediate);
    assert.deepEqual(g.exported, [batch]);
  } finally {
    g.dispose();
  }
});
