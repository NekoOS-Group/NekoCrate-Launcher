import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer } from "vue";
import { useTrashDrag } from "../src/useTrashDrag.ts";
import { dropInteraction } from "../src/dropSurface.ts";

function fixture() {
  const previous = Object.fromEntries(
    ["window", "document"].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  const listeners = new Map(),
    windowListeners = new Map(),
    removed = [],
    classes = new Set();
  let locked = false,
    captured = false,
    selectionCleared = 0,
    ui;
  globalThis.window = {
    innerWidth: 1000,
    getSelection: () => ({
      removeAllRanges() {
        selectionCleared++;
      },
    }),
    addEventListener: (key, fn) => windowListeners.set(key, fn),
    removeEventListener: (key) => windowListeners.delete(key),
  };
  globalThis.document = {
    documentElement: {
      classList: {
        add: (key) => classes.add(key),
        remove: (key) => classes.delete(key),
      },
    },
    addEventListener: (key, fn) => listeners.set(key, fn),
    removeEventListener: (key) => listeners.delete(key),
  };
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      ui = useTrashDrag({
        canRemove: (runtime) => runtime.source === "managed" && !locked,
        describe: (runtime) => ({ kind: "java", name: runtime.path }),
        remove: async (runtime) => {
          removed.push(runtime);
        },
      });
      return () => null;
    },
  });
  app.mount({});
  const event = (x = 200, y = 300) => ({
    isPrimary: true,
    button: 0,
    pointerId: 1,
    clientX: x,
    clientY: y,
    preventDefault() {},
    stopImmediatePropagation() {},
    currentTarget: {
      setPointerCapture() {
        captured = true;
      },
      hasPointerCapture: () => captured,
      releasePointerCapture() {
        captured = false;
      },
    },
  });
  return {
    ui,
    removed,
    listeners,
    classes,
    start: (item) => ui.start(event(), item),
    move: (x, y) => listeners.get("pointermove")?.(event(x, y)),
    end: (x, y) => listeners.get("pointerup")?.(event(x, y)),
    cancel: () => listeners.get("pointercancel")?.(event()),
    escape: () => listeners.get("keydown")?.({ ...event(), key: "Escape" }),
    blur: () => windowListeners.get("blur")?.(),
    lock: () => {
      locked = true;
    },
    get captured() {
      return captured;
    },
    get selectionCleared() {
      return selectionCleared;
    },
    dispose() {
      app.unmount();
      for (const [key, value] of Object.entries(previous)) {
        if (value) Object.defineProperty(globalThis, key, value);
        else delete globalThis[key];
      }
    },
  };
}
const managed = { source: "managed", path: "/cache/jdk-21" };

test("only cached Java starts a trash drag; a click keeps normal tile selection", () => {
  const f = fixture();
  try {
    for (const source of ["system", "registered", "configured"]) {
      f.start({ ...managed, source });
      assert.equal(f.listeners.size, 0);
    }
    f.start(managed);
    f.move(202, 301);
    assert.equal(f.ui.drag.value, undefined);
    f.end(202, 301);
    assert.equal(f.listeners.has("click"), false);
    assert.deepEqual(f.removed, []);
  } finally {
    f.dispose();
  }
});

test("Java drag uses the existing trash bounds and suppresses selection and the trailing click", () => {
  const f = fixture();
  try {
    f.start(managed);
    f.move(950, 50);
    assert.equal(f.ui.drag.value.target.trash, true);
    assert.equal(f.ui.drag.value.source.kind, "java");
    assert.equal(f.captured, true);
    assert.equal(f.selectionCleared, 1);
    assert.equal(dropInteraction.value, true);
    assert.ok(f.classes.has("resource-transferring"));
    f.end(950, 50);
    assert.deepEqual(f.removed, [managed]);
    assert.equal(f.captured, false);
    assert.equal(dropInteraction.value, false);
    assert.equal(f.classes.size, 0);
    assert.equal(f.ui.drag.value, undefined);
    let prevented = false,
      stopped = false;
    f.listeners.get("click")({
      preventDefault() {
        prevented = true;
      },
      stopImmediatePropagation() {
        stopped = true;
      },
    });
    assert.ok(prevented && stopped);
  } finally {
    f.dispose();
  }
});

test("cancelled, off-target and newly locked Java drops never remove a runtime", () => {
  for (const action of ["cancel", "escape", "blur", "lock", "outside"]) {
    const f = fixture();
    try {
      f.start(managed);
      f.move(950, 50);
      if (action !== "outside") f[action]();
      f.end(action === "outside" ? 400 : 950, 50);
      assert.deepEqual(f.removed, [], action);
      assert.equal(f.ui.drag.value, undefined);
      assert.equal(dropInteraction.value, false);
    } finally {
      f.dispose();
    }
  }
});
