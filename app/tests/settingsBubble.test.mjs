import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, markRaw, nextTick, reactive } from "vue";
import { bubblePosition } from "../src/bubblePosition.ts";
import { registerDialog } from "../src/dialogSurface.ts";
import { finishDialogLeave } from "../src/dialogMotion.ts";

test("settings bubbles flip at viewport edges and stay attached when resized", () => {
  const viewport = { width: 800, height: 600 };
  const panel = { width: 480, height: 280 };
  const top = { left: 360, top: 24, width: 36, height: 36 };
  const a = bubblePosition(top, panel, viewport, "below");
  assert.equal(a.side, "below");
  assert.equal(a.top, 72);
  assert.ok(a.left + panel.width <= 788);
  assert.equal(a.left + a.tip, top.left + top.width / 2);
  const bottom = { ...top, top: 530 };
  const b = bubblePosition(bottom, panel, viewport, "below");
  assert.equal(b.side, "above");
  assert.equal(b.top + panel.height, bottom.top - 12);
  const tall = bubblePosition(
    bottom,
    { ...panel, height: 700 },
    viewport,
    "above",
  );
  assert.equal(tall.top, 12);
  assert.equal(tall.side, undefined);
  const centered = bubblePosition(undefined, panel, viewport, "above");
  assert.equal(centered.left, 160);
  assert.equal(centered.top, 160);
});

const file = new URL("../src/components/SettingsBubble.vue", import.meta.url);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "settings-bubble",
  }).content,
  {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  },
).outputText;
code = code.replace(
  /from ["']([^"']+)["']/g,
  (_, name) =>
    `from ${JSON.stringify(name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Bubble = (
  await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  )
).default;

async function fixture() {
  const keys = [
    "Node",
    "HTMLElement",
    "HTMLDialogElement",
    "document",
    "window",
    "ResizeObserver",
  ];
  const old = Object.fromEntries(
    keys.map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  const listeners = new Map(),
    observed = new Set(),
    events = [];
  let resize;
  class Element {
    children = [];
    dataset = {};
    isConnected = true;
    offsetWidth = 480;
    offsetHeight = 280;
    contains(target) {
      return target === this || this.children.includes(target);
    }
    closest() {
      return null;
    }
    focus() {
      document.activeElement = this;
    }
    blur() {
      events.push("blur");
      document.activeElement = null;
    }
    show() {
      events.push("show");
    }
    close() {}
    getBoundingClientRect() {
      return { left: 300, top: 24, width: 36, height: 36 };
    }
  }
  const anchor = markRaw(new Element()),
    dialog = markRaw(new Element()),
    frame = markRaw(new Element());
  const input = new Element(),
    menu = new Element(),
    heading = new Element(),
    outside = new Element();
  dialog.children = [frame, input, menu, heading];
  frame.querySelector = () => heading;
  const document = {
    activeElement: anchor,
    querySelector: () => null,
    addEventListener: (key, fn) => listeners.set(key, fn),
    removeEventListener: (key) => listeners.delete(key),
  };
  const window = {
    innerWidth: 800,
    innerHeight: 600,
    addEventListener: (key, fn) => listeners.set(key, fn),
    removeEventListener: (key) => listeners.delete(key),
  };
  Object.assign(globalThis, {
    Node: Element,
    HTMLElement: Element,
    HTMLDialogElement: Element,
    document,
    window,
    ResizeObserver: class {
      constructor(fn) {
        resize = fn;
      }
      observe(el) {
        observed.add(el);
      }
      unobserve(el) {
        observed.delete(el);
      }
      disconnect() {
        observed.clear();
      }
    },
  });
  const props = reactive({ anchor, label: "Settings", locked: false });
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
      ui = Bubble.setup(props, {
        expose() {},
        emit: () => events.push("close"),
      });
      ui.dialog.value = dialog;
      ui.frame.value = frame;
      return () => null;
    },
  });
  app.mount({});
  await nextTick();
  return {
    ui,
    props,
    dialog,
    frame,
    input,
    menu,
    heading,
    anchor,
    outside,
    document,
    window,
    listeners,
    observed,
    events,
    resize: () => resize(),
    dispose() {
      app.unmount();
      for (const key of keys) {
        if (old[key]) Object.defineProperty(globalThis, key, old[key]);
        else delete globalThis[key];
      }
    },
  };
}

test("nonmodal bubbles allow floating menus; outside dismissal commits input without stealing focus", async () => {
  const f = await fixture();
  try {
    assert.deepEqual(f.events, ["show"]);
    assert.equal(f.document.activeElement, f.heading);
    f.ui.outside({ target: f.menu });
    f.ui.outside({ target: f.anchor });
    assert.equal(f.events.length, 1);
    f.document.activeElement = f.input;
    f.ui.outside({ target: f.outside });
    assert.deepEqual(f.events, ["show", "blur", "close"]);
    f.document.activeElement = f.outside;
    finishDialogLeave(f.dialog);
    assert.equal(f.document.activeElement, f.outside);
  } finally {
    f.dispose();
  }
  assert.equal(f.listeners.size, 0);
  assert.equal(f.observed.size, 0);
});

test("Escape belongs to the current surface; geometry follows content and viewport changes", async () => {
  const f = await fixture();
  try {
    const event = {
      key: "Escape",
      preventDefault() {
        this.defaultPrevented = true;
      },
    };
    f.ui.keyboard({ ...event, defaultPrevented: true });
    const remove = registerDialog(f.outside);
    f.ui.keyboard({ ...event });
    remove();
    f.props.locked = true;
    f.ui.outside({ target: f.outside });
    assert.deepEqual(f.events, ["show"]);
    f.props.locked = false;
    f.window.innerWidth = 620;
    f.listeners.get("resize")();
    assert.equal(f.ui.position.value.left, 128);
    f.frame.offsetHeight = 700;
    f.resize();
    assert.equal(f.ui.position.value.top, 12);
    f.ui.keyboard({ ...event });
    assert.equal(f.events.at(-1), "close");
    finishDialogLeave(f.dialog);
    assert.equal(f.document.activeElement, f.anchor);
  } finally {
    f.dispose();
  }
});
