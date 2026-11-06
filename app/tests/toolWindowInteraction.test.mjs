import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick, reactive } from "vue";
import { toolOrder } from "../src/toolWindows.ts";
const file = new URL("../src/components/ToolWindow.vue", import.meta.url);
const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
let source = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "tool-interaction",
  }).content,
  {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  },
).outputText;
source = source.replace(
  /from ["']([^"']+)["']/g,
  (_, name) =>
    `from ${JSON.stringify(name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Component = (await import(url(source))).default;
const flush = async () => {
  await nextTick();
  await nextTick();
};
async function fixture(saved) {
  const old = Object.fromEntries(
    [
      "HTMLElement",
      "document",
      "window",
      "innerWidth",
      "innerHeight",
      "localStorage",
    ].map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  const captures = new Set(),
    classes = new Set(),
    events = [],
    listeners = new Map(),
    storage = new Map();
  if (saved) storage.set("nekocrate.window.test", JSON.stringify(saved));
  class Element {
    dataset = {};
    isConnected = true;
    closest(selector) {
      return selector.includes("modal-header") ? this : null;
    }
    contains() {
      return true;
    }
    querySelector() {
      return null;
    }
    focus() {
      globalThis.document.activeElement = this;
    }
    show() {
      this.open = true;
    }
    getBoundingClientRect() {
      return { x: 100, y: 100, width: 600, height: 400 };
    }
    setPointerCapture(id) {
      captures.add(id);
    }
    hasPointerCapture(id) {
      return captures.has(id);
    }
    releasePointerCapture(id) {
      captures.delete(id);
    }
  }
  const document = {
    activeElement: null,
    documentElement: {
      classList: {
        add: (name) => classes.add(name),
        remove: (name) => classes.delete(name),
      },
    },
    addEventListener: (name, fn) => listeners.set(name, fn),
    removeEventListener: (name) => listeners.delete(name),
  };
  const window = {
    innerWidth: 1200,
    innerHeight: 800,
    addEventListener: (name, fn) => listeners.set(name, fn),
    removeEventListener: (name) => listeners.delete(name),
    getSelection: () => ({ removeAllRanges() {} }),
  };
  for (const [key, value] of Object.entries({
    HTMLElement: Element,
    document,
    window,
    innerWidth: 1200,
    innerHeight: 800,
    localStorage: {
      getItem: (key) => storage.get(key),
      setItem: (key, value) => storage.set(key, value),
    },
  }))
    Object.defineProperty(globalThis, key, {
      value,
      configurable: true,
      writable: true,
    });
  const props = reactive({ windowId: "test", label: "Test", locked: false });
  let ui;
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      ui = Component.setup(props, {
        expose() {},
        emit: (...args) => events.push(args),
      });
      ui.dialog.value = new Element();
      ui.frame.value = new Element();
      return () => null;
    },
  });
  app.mount({});
  await flush();
  const title = new Element();
  const pointer = (x, y, extra = {}) => ({
    button: 0,
    isPrimary: true,
    pointerId: 1,
    clientX: x,
    clientY: y,
    target: title,
    preventDefault() {},
    ...extra,
  });
  return {
    ui,
    props,
    events,
    storage,
    captures,
    classes,
    listeners,
    pointer,
    title,
    window,
    dispose() {
      app.unmount();
      for (const [key, descriptor] of Object.entries(old)) {
        if (descriptor) Object.defineProperty(globalThis, key, descriptor);
        else delete globalThis[key];
      }
    },
  };
}
test("drag only captures after threshold; edge preview commits on release, restore and cancel keep the saved geometry", async () => {
  const h = await fixture();
  try {
    const original = { ...h.ui.rect.value };
    h.ui.down(h.pointer(400, 220));
    assert.equal(
      h.captures.size,
      0,
      "ordinary title clicks must still produce dblclick",
    );
    h.ui.move(h.pointer(401, 220));
    assert.deepEqual(h.ui.rect.value, original);
    h.ui.move(h.pointer(5, 300));
    assert.equal(h.captures.size, 1);
    assert.equal(h.ui.preview.value, "left");
    h.ui.up(h.pointer(5, 300));
    assert.equal(h.ui.snap.value, "left");
    assert.equal(h.ui.rect.value.width, 582);
    assert.equal(h.captures.size, 0);
    const saved = JSON.parse(h.storage.get("nekocrate.window.test"));
    assert.equal(saved.snap, "left");
    h.ui.maximize();
    assert.deepEqual(h.ui.rect.value, saved.rect);
    h.ui.doubleClick({ target: h.title, preventDefault() {} });
    assert.equal(h.ui.snap.value, "maximized");
    h.ui.down(h.pointer(420, 30));
    h.ui.move(h.pointer(600, 150));
    assert.equal(h.ui.snap.value, undefined);
    h.ui.cancel();
    assert.equal(h.ui.snap.value, "maximized");
  } finally {
    h.dispose();
  }
  assert.deepEqual(toolOrder.value, []);
});
test("selection is blocked before movement and restored after release, cancellation, blur and unmount", async () => {
  const h = await fixture();
  const guarded = () => {
    assert.ok(h.classes.has("tool-window-manipulating"));
    const event = new Event("selectstart", { cancelable: true });
    h.listeners.get("selectstart")(event);
    assert.ok(event.defaultPrevented);
  };
  const released = () => {
    assert.equal(h.classes.has("tool-window-manipulating"), false);
    assert.equal(h.listeners.has("selectstart"), false);
  };
  try {
    // Text fields and body text retain normal selection, even in the title bar.
    for (const closest of [() => h.title, () => null]) {
      const target = new h.title.constructor();
      target.closest = closest;
      h.ui.down(h.pointer(400, 220, { target }));
      released();
    }
    h.ui.down(h.pointer(400, 220));
    guarded();
    h.ui.move(h.pointer(401, 220));
    guarded();
    assert.equal(h.ui.interacting.value, false);
    h.ui.up(h.pointer(401, 220));
    released();
    for (const end of ["pointercancel", "blur"]) {
      h.ui.down(h.pointer(900, 600), "se");
      guarded();
      h.listeners.get(end)();
      released();
    }
    h.ui.down(h.pointer(400, 220));
    h.ui.move(h.pointer(430, 250));
    guarded();
  } finally {
    h.dispose();
  }
  released();
  assert.equal(h.listeners.size, 0);
});
test("interactive header controls are not dragged, resizing persists, viewport shrink clamps and locked windows reject close", async () => {
  const h = await fixture();
  try {
    const button = new h.title.constructor();
    button.closest = () => button;
    h.ui.down(h.pointer(400, 220, { target: button }));
    h.ui.move(h.pointer(500, 300));
    assert.equal(h.ui.interacting.value, false);
    h.ui.down(h.pointer(900, 600), "se");
    h.ui.move(h.pointer(1000, 650));
    h.ui.up(h.pointer(1000, 650));
    assert.equal(h.ui.rect.value.width, 700);
    assert.equal(h.ui.rect.value.height, 450);
    h.window.innerWidth = 760;
    h.window.innerHeight = 500;
    h.ui.viewportChanged();
    assert.ok(h.ui.rect.value.x + h.ui.rect.value.width <= 748);
    assert.ok(h.ui.rect.value.y + h.ui.rect.value.height <= 488);
    h.props.locked = true;
    h.ui.close();
    assert.equal(h.events.length, 0);
    h.props.locked = false;
    h.ui.close();
    assert.deepEqual(h.events, [["close"]]);
  } finally {
    h.dispose();
  }
  assert.equal(h.listeners.size, 0);
});
test("saved layouts reopen in bounds and malformed saved geometry falls back to the measured panel", async () => {
  let h = await fixture({
    rect: { x: 3000, y: 2000, width: 600, height: 400 },
    snap: "right",
  });
  try {
    assert.equal(h.ui.rect.value.x, 606);
    assert.equal(h.ui.rect.value.width, 582);
  } finally {
    h.dispose();
  }
  h = await fixture({ rect: { x: "bad", y: 0, width: 1, height: 1 } });
  try {
    assert.deepEqual(h.ui.rect.value, {
      x: 300,
      y: 200,
      width: 600,
      height: 400,
    });
  } finally {
    h.dispose();
  }
});
