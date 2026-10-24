import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, h, nextTick, shallowReactive } from "vue";
import { setLanguage } from "../src/i18n.ts";

const file = new URL("../src/components/VersionBrowser.vue", import.meta.url);
function componentUrl(file) {
  let source = ts.transpileModule(
    compileScript(parse(readFileSync(file, "utf8")).descriptor, {
      id: file.pathname,
      inlineTemplate: true,
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
      `from ${JSON.stringify(
        name.endsWith(".vue")
          ? componentUrl(new URL(name, file))
          : name.startsWith(".")
            ? new URL(`${name}.ts`, file).href
            : import.meta.resolve(name),
      )}`,
  );
  return `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
}
const Browser = (await import(componentUrl(file))).default;
const all = (node) => [node, ...node.children.flatMap(all)];
function node(tag, text = "") {
  return {
    tag,
    text,
    props: {},
    children: [],
    scrollTop: 0,
    clientHeight: 270,
    getRootNode() {
      return globalThis.document;
    },
    addEventListener() {},
    removeEventListener() {},
  };
}
function detach(child) {
  if (child.parent)
    child.parent.children.splice(child.parent.children.indexOf(child), 1);
}
const host = createRenderer({
  createElement: node,
  createText: (text) => node("text", text),
  createComment: (text) => node("comment", text),
  parentNode: (n) => n.parent,
  nextSibling: (n) => n.parent?.children[n.parent.children.indexOf(n) + 1],
  insert(child, parent, anchor) {
    detach(child);
    parent.children.splice(
      anchor ? parent.children.indexOf(anchor) : parent.children.length,
      0,
      child,
    );
    child.parent = parent;
  },
  remove: detach,
  setText: (n, text) => {
    n.text = text;
  },
  setElementText: (n, text) => {
    n.text = text;
    n.children = [];
  },
  patchProp: (n, key, _, value) => {
    n.props[key] = value;
  },
});
const settle = async () => {
  await nextTick();
  await nextTick();
};

test("version browser reaches the entire catalog with bounded rows, keyboard navigation and category/search filtering", async (t) => {
  const oldObserver = globalThis.ResizeObserver;
  const previousDocument = globalThis.document;
  const previousDocumentClass = globalThis.Document;
  const previousShadowRoot = globalThis.ShadowRoot;
  globalThis.Document = class {
    activeElement = null;
  };
  globalThis.ShadowRoot = class {};
  globalThis.document = new Document();
  let observer;
  globalThis.ResizeObserver = class {
    constructor(callback) {
      this.callback = callback;
      observer = this;
    }
    observe() {
      this.callback();
    }
    disconnect() {
      this.disconnected = true;
    }
  };
  t.after(() => {
    globalThis.ResizeObserver = oldObserver;
    globalThis.document = previousDocument;
    globalThis.Document = previousDocumentClass;
    globalThis.ShadowRoot = previousShadowRoot;
  });
  setLanguage("zh-CN", "zh-CN");
  const versions = Array.from({ length: 1200 }, (_, i) => ({
    id: `release-${i}`,
    type: "release",
    url: "",
    sha1: "",
  }));
  versions.push({ id: "26w14a", type: "snapshot", url: "", sha1: "" });
  versions.push({ id: "26.2-snapshot-1", type: "snapshot", url: "", sha1: "" });
  const props = shallowReactive({
    modelValue: "release-0",
    manifest: { latest: { release: "release-0" }, versions },
    loading: false,
    disabled: false,
    "onUpdate:modelValue": (value) => {
      props.modelValue = value;
    },
  });
  const root = node("root");
  const app = host.createApp({ setup: () => () => h(Browser, props) });
  app.mount(root);
  t.after(() => {
    app.unmount();
    assert.equal(observer.disconnected, true);
  });
  await settle();
  const find = (predicate) => all(root).find(predicate);
  const rows = () => all(root).filter((n) => n.props.role === "option");
  const list = find((n) => n.props.role === "listbox");
  const key = (key) => list.props.onKeydown({ key, preventDefault() {} });
  assert.ok(rows().length < 30);
  assert.equal(rows()[0].props["aria-setsize"], 1200);
  key("End");
  await settle();
  assert.ok(rows().length < 30);
  assert.equal(rows().at(-1).props["aria-posinset"], 1200);
  assert.ok(find((n) => n.props.id === list.props["aria-activedescendant"]));
  key("Enter");
  await settle();
  assert.equal(props.modelValue, "release-1199");
  key("Home");
  key("ArrowRight");
  key("ArrowDown");
  key(" ");
  await settle();
  assert.equal(props.modelValue, "release-3");
  props.disabled = true;
  await settle();
  rows()[0].props.onClick();
  assert.equal(props.modelValue, "release-3");
  props.disabled = false;
  const search = find((n) => n.tag === "input");
  search.props["onUpdate:modelValue"]("release-1199");
  await settle();
  assert.equal(rows().length, 1);
  assert.equal(list.scrollTop, 0);
  search.props["onUpdate:modelValue"]("");
  find(
    (n) => n.tag === "button" && n.props["aria-label"] === "愚人节版",
  ).props.onClick();
  await settle();
  assert.equal(rows().length, 1);
  rows()[0].props.onClick();
  await settle();
  assert.equal(props.modelValue, "26w14a");
  find(
    (n) => n.tag === "button" && n.props["aria-label"] === "快照版",
  ).props.onClick();
  await settle();
  assert.equal(rows().length, 1);
  assert.equal(rows()[0].props.title, "26.2-snapshot-1");
  search.props["onUpdate:modelValue"]("missing");
  await settle();
  assert.equal(rows().length, 0);
  assert.equal(list.props["aria-activedescendant"], undefined);
  assert.equal(
    props.modelValue,
    "26w14a",
    "filters never silently change the installation selection",
  );
});
