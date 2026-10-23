import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, h, nextTick, ref, toRaw } from "vue";

// Exercise the actual Vue handlers without a browser or desktop automation.
const source = readFileSync(
  new URL("../src/components/ThemePicker.vue", import.meta.url),
  "utf8",
);
const compiled = compileScript(parse(source).descriptor, {
  id: "theme-regression",
  inlineTemplate: true,
});
let code = ts.transpileModule(compiled.content, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
  },
}).outputText;
code = code.replace(/from ["']([^"']+)["']/g, (_, name) => {
  const url = name.startsWith(".")
    ? new URL(`../src/components/${name}.ts`, import.meta.url).href
    : import.meta.resolve(name);
  return `from ${JSON.stringify(url)}`;
});
const ThemePicker = (
  await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  )
).default;

class Element {
  constructor(tag = "") {
    this.tag = tag;
    this.props = {};
    this.children = [];
    this.parent = null;
    this.capture = null;
  }
  contains(node) {
    return node === this || this.children.some((child) => child.contains(node));
  }
  focus() {
    document.activeElement = this;
  }
  matches() {
    return false;
  }
  getBoundingClientRect() {
    return { left: 40, top: 12, right: 76, bottom: 76 };
  }
  setPointerCapture(id) {
    this.capture = id;
  }
  hasPointerCapture(id) {
    return this.capture === id;
  }
  releasePointerCapture() {
    this.capture = null;
  }
}

test("主题选择跨过 Safari 空焦点事件后仍在松手时提交一次，并支持取消与键盘", async () => {
  const previousDocument = globalThis.document;
  const previousNode = globalThis.Node;
  globalThis.Node = Element;
  globalThis.document = {
    activeElement: null,
    addEventListener() {},
    removeEventListener() {},
  };
  const renderer = createRenderer({
    createElement: (tag) => new Element(tag),
    createText: () => new Element("text"),
    createComment: () => new Element("comment"),
    setText() {},
    setElementText() {},
    parentNode: (element) => element.parent,
    nextSibling: (element) =>
      element.parent?.children[element.parent.children.indexOf(element) + 1],
    patchProp: (element, key, _, value) => {
      element.props[key] = value;
    },
    insert(element, parent, anchor) {
      if (element.parent)
        element.parent.children.splice(
          element.parent.children.indexOf(element),
          1,
        );
      const index = anchor ? parent.children.indexOf(anchor) : -1;
      parent.children.splice(
        index < 0 ? parent.children.length : index,
        0,
        element,
      );
      element.parent = parent;
    },
    remove(element) {
      element.parent?.children.splice(
        element.parent.children.indexOf(element),
        1,
      );
    },
  });
  const selected = ref("tide");
  const choices = [];
  const app = renderer.createApp({
    setup: () => () =>
      h(ThemePicker, {
        modelValue: selected.value,
        "onUpdate:modelValue": (value) => {
          choices.push(value);
          selected.value = value;
        },
      }),
  });
  try {
    const container = new Element();
    app.mount(container);
    const root = container.children[0];
    const options = root.children.find(
      (child) => child.props.class === "theme-options",
    );
    const buttons = () =>
      options.children.filter((child) => child.tag === "button");
    const pointer = (target) => ({
      currentTarget: target,
      isPrimary: true,
      button: 0,
      pointerId: 4,
      clientX: 58,
      clientY: 44,
      preventDefault() {},
    });
    root.props.onPointerenter({ pointerType: "mouse" });
    await nextTick();
    const first = buttons()[0];
    first.props.onPointerdown(pointer(first));
    root.props.onFocusout({ relatedTarget: null });
    root.props.onPointerleave();
    await nextTick();
    assert.equal(options.props.inert, false);
    first.props.onPointerup(pointer(first));
    first.props.onClick({ detail: 1 });
    await nextTick();
    assert.deepEqual(choices, ["sage"]);
    assert.equal(options.props.inert, true);
    assert.equal(document.activeElement, null, "鼠标选择不应强制聚焦触发按钮");
    root.props.onPointerenter({ pointerType: "mouse" });
    await nextTick();
    const next = buttons()[0];
    next.props.onPointerdown(pointer(next));
    next.props.onPointercancel();
    next.props.onPointerup(pointer(next));
    assert.equal(choices.length, 1);
    next.props.onPointerdown(pointer(next));
    next.props.onPointerup({ ...pointer(next), clientX: 200 });
    assert.equal(choices.length, 1);
    next.props.onClick({ detail: 0 });
    await nextTick();
    assert.deepEqual(choices, ["sage", "tide"]);
    assert.equal(
      toRaw(document.activeElement),
      root.children[0],
      "键盘选择后焦点返回触发按钮",
    );
  } finally {
    app.unmount();
    globalThis.document = previousDocument;
    globalThis.Node = previousNode;
  }
});
