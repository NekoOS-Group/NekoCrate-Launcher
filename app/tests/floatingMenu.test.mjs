import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, markRaw } from "vue";
import { floatingMenuPosition } from "../src/floatingMenu.ts";

const file = new URL("../src/components/GlassSelect.vue", import.meta.url);
const compiled = compileScript(parse(readFileSync(file, "utf8")).descriptor, {
  id: "floating-select",
});
let code = ts.transpileModule(compiled.content, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
  },
}).outputText;
code = code.replace(
  /from ["']([^"']+)["']/g,
  (_, name) =>
    `from ${JSON.stringify(name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Select = (
  await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  )
).default;

test("浮层跟随触发按钮且不超出窗口，窄窗口仍能滚动菜单", () => {
  assert.deepEqual(
    floatingMenuPosition(
      { left: 80, bottom: 200, width: 180 },
      { width: 640, height: 480 },
    ),
    {
      left: "80px",
      top: "208px",
      width: "180px",
      maxHeight: "256px",
    },
  );
  const position = floatingMenuPosition(
    { left: 300, bottom: 460, width: 600 },
    { width: 400, height: 480 },
  );
  assert.equal(position.left, "16px");
  assert.equal(position.width, "368px");
  assert.equal(parseFloat(position.top) + parseFloat(position.maxHeight), 464);
});

test("菜单留在原生 dialog 内，支持跨浮层焦点、键盘选择、点击外部关闭和动画位置跟踪", async () => {
  const names = [
    "document",
    "window",
    "Node",
    "requestAnimationFrame",
    "cancelAnimationFrame",
  ];
  const previous = Object.fromEntries(
    names.map((name) => [name, globalThis[name]]),
  );
  const listeners = new Map();
  const frames = new Map();
  let frameId = 0;
  class Element {
    children = [];
    scrolled = false;
    scrollIntoView() {
      this.scrolled = true;
    }
    contains(node) {
      return (
        node === this || this.children.some((child) => child.contains(node))
      );
    }
    focus() {
      document.activeElement = this;
    }
    querySelectorAll() {
      return this.children;
    }
  }
  const node = () => markRaw(new Element());
  const dialog = node(),
    panel = node(),
    root = node(),
    trigger = node(),
    list = node();
  root.children.push(trigger);
  panel.children.push(root);
  dialog.children.push(panel, list);
  root.closest = () => dialog;
  let bottom = 220;
  trigger.getBoundingClientRect = () => ({ left: 80, bottom, width: 180 });
  list.children.push(node(), node(), node());
  globalThis.Node = Element;
  globalThis.document = {
    body: node(),
    activeElement: null,
    addEventListener: (key, callback) => listeners.set(key, callback),
    removeEventListener: (key) => listeners.delete(key),
  };
  globalThis.window = {
    innerWidth: 640,
    innerHeight: 480,
    addEventListener: (key, callback) => listeners.set(key, callback),
    removeEventListener: (key) => listeners.delete(key),
  };
  globalThis.requestAnimationFrame = (callback) => {
    frames.set(++frameId, callback);
    return frameId;
  };
  globalThis.cancelAnimationFrame = (id) => frames.delete(id);
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const choices = [];
  let ui;
  const app = renderer.createApp({
    setup() {
      ui = Select.setup(
        {
          floating: true,
          id: "source",
          modelValue: "b",
          options: ["a", "b", "c"].map((value) => ({ value, label: value })),
        },
        { expose() {}, emit: (...args) => choices.push(args) },
      );
      return () => null;
    },
  });
  try {
    app.mount({});
    ui.root.value = root;
    ui.trigger.value = trigger;
    ui.list.value = list;
    await ui.open();
    assert.equal(ui.portalTarget.value, dialog);
    assert.equal(document.activeElement, list.children[1]);
    ui.leave({ relatedTarget: list.children[1] });
    listeners.get("pointerdown")({ target: list.children[1] });
    assert.equal(ui.expanded.value, true);
    assert.equal(frames.size, 1);
    bottom = 180;
    const [id, callback] = [...frames][0];
    frames.delete(id);
    callback();
    assert.equal(ui.floatingStyle.value.top, "188px");
    const key = (value) => ({
      key: value,
      preventDefault() {},
      stopPropagation() {},
    });
    ui.navigate(key("ArrowDown"));
    assert.equal(document.activeElement, list.children[2]);
    assert.equal(list.children[2].scrolled, true);
    ui.navigate(key("Home"));
    assert.equal(document.activeElement, list.children[0]);
    ui.choose("a");
    assert.deepEqual(choices, [["update:modelValue", "a"]]);
    assert.equal(document.activeElement, trigger);
    assert.equal(frames.size, 0);
    await ui.open();
    ui.navigate(key("Escape"));
    assert.equal(ui.expanded.value, false);
    await ui.open();
    listeners.get("pointerdown")({ target: panel });
    assert.equal(ui.expanded.value, false);
    await ui.open();
  } finally {
    app.unmount();
    assert.equal(frames.size, 0);
    assert.equal(listeners.size, 0);
    Object.assign(globalThis, previous);
  }
});
