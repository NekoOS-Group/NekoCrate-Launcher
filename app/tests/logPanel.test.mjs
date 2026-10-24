import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick, h, shallowReactive } from "vue";
import { createLogStore, logRefreshMs } from "../src/logStore.ts";
import { setLanguage } from "../src/i18n.ts";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL("../src/components/LogPanel.vue", import.meta.url);
const selectStub =
  moduleUrl(`import {h} from ${JSON.stringify(import.meta.resolve("vue"))};
export default {props:["id","modelValue","options"],emits:["update:modelValue"],
setup(p,{emit}) {return ()=>h("select-fixture",{id:p.id,value:p.modelValue,
onChange:value=>emit("update:modelValue",value)});}};`);
let source = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "log-panel-test",
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
    `from ${JSON.stringify(name === "./GlassSelect.vue" ? selectStub : name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const LogPanel = (await import(moduleUrl(source))).default;
const all = (node) => [node, ...node.children.flatMap(all)];
function node(tag, text = "") {
  return {
    tag,
    text,
    children: [],
    props: {},
    dataset: {},
    listeners: new Map(),
    scrollTop: 0,
    offsetHeight: 28,
    getRootNode() {
      return globalThis.document;
    },
    addEventListener(type, callback) {
      this.listeners.set(type, callback);
    },
    removeEventListener(type) {
      this.listeners.delete(type);
    },
    setAttribute(key, value) {
      this.props[key] = value;
    },
    removeAttribute(key) {
      delete this.props[key];
    },
    querySelectorAll() {
      return all(this).filter((n) => n.dataset.logId !== undefined);
    },
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
  insert(child, parent, anchor) {
    detach(child);
    const index = anchor
      ? parent.children.indexOf(anchor)
      : parent.children.length;
    parent.children.splice(index, 0, child);
    child.parent = parent;
  },
  remove: detach,
  parentNode: (n) => n.parent,
  nextSibling: (n) => n.parent?.children[n.parent.children.indexOf(n) + 1],
  setText: (n, text) => {
    n.text = text;
  },
  setElementText: (n, text) => {
    n.text = text;
    n.children = [];
  },
  patchProp(n, key, old, value) {
    n.props[key] = value;
    if (key === "data-log-id") n.dataset.logId = String(value);
  },
});
const settle = async () => {
  await nextTick();
  await nextTick();
};

test("real log panel browses all session entries with bounded DOM, filters/copies full history, and cleans up", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  setLanguage("zh-CN", "zh-CN");
  const oldObserver = globalThis.ResizeObserver,
    oldDocument = globalThis.document,
    oldDocumentClass = globalThis.Document,
    oldShadowRoot = globalThis.ShadowRoot;
  t.after(() => {
    globalThis.ResizeObserver = oldObserver;
    globalThis.document = oldDocument;
    globalThis.Document = oldDocumentClass;
    globalThis.ShadowRoot = oldShadowRoot;
  });
  globalThis.Document = class {
    activeElement = null;
  };
  globalThis.ShadowRoot = class {};
  globalThis.document = new Document();
  let observer;
  globalThis.ResizeObserver = class {
    observed = new Set();
    constructor(callback) {
      this.callback = callback;
      observer = this;
    }
    observe(element) {
      this.observed.add(element);
    }
    unobserve(element) {
      this.observed.delete(element);
    }
    disconnect() {
      this.observed.clear();
      this.disconnected = true;
    }
  };
  const store = createLogStore();
  const entry = (i) => ({
    time: "12:00:00",
    kind: "log",
    instanceId: i % 2 ? "b" : "a",
    message: `[INFO] line ${i}`,
  });
  for (let i = 0; i < 10000; i++) store.append(entry(i));
  let snapshots = 0,
    copied;
  const props = shallowReactive({
    store: {
      ...store,
      snapshot() {
        snapshots++;
        return store.snapshot();
      },
    },
    instances: [
      { id: "a", name: "A" },
      { id: "b", name: "B" },
    ],
    initialScope: "all",
    session: 1,
    onCopy(text) {
      copied = text;
    },
  });
  const root = node("root");
  const app = host.createApp({ setup: () => () => h(LogPanel, props) });
  app.mount(root);
  const find = (predicate) => all(root).find(predicate);
  const rows = () => all(root).filter((n) => n.props.role === "listitem");
  const content = find((n) => n.props.class === "log-content");
  observer.callback([
    { target: content, contentRect: { width: 700, height: 280 } },
  ]);
  await settle();
  assert.ok(rows().length <= 17);
  assert.equal(rows().at(-1).props["aria-posinset"], 10000);
  content.scrollTop = 0;
  content.props.onScrollPassive();
  await settle();
  assert.equal(
    rows()[0].props["aria-posinset"],
    1,
    "earliest logs remain accessible after 10,000 entries",
  );
  assert.ok(find((n) => n.props.class === "log-latest text-button"));
  const before = snapshots;
  for (let i = 10000; i < 12000; i++) store.append(entry(i));
  await settle();
  assert.equal(snapshots, before);
  t.mock.timers.tick(logRefreshMs);
  await settle();
  assert.equal(snapshots, before + 1);
  assert.equal(
    content.scrollTop,
    0,
    "new logs do not move a reader away from history",
  );
  assert.equal(rows()[0].props["aria-setsize"], 12000);
  assert.ok(rows().length <= 17);
  find((n) => n.props.id === "log-scope").props.onChange("a");
  await settle();
  assert.equal(rows().at(-1).props["aria-setsize"], 6000);
  find((n) => n.props["aria-label"] === "复制筛选后的日志").props.onClick();
  assert.equal(copied.split("\n").length, 6000);
  assert.ok(copied.startsWith("[12:00:00] [INFO] line 0\n"));
  find((n) => n.tag === "input").props["onUpdate:modelValue"]("line 11998");
  await settle();
  assert.equal(rows().length, 1);
  find((n) => n.props.id === "log-level").props.onChange("error");
  await settle();
  assert.equal(rows().length, 0);
  props.initialScope = "b";
  props.session++;
  await settle();
  assert.equal(
    rows().at(-1).props["aria-setsize"],
    6000,
    "new sessions reset stale search/severity filters",
  );
  find((n) => n.props["aria-label"] === "清空此实例日志").props.onClick();
  await settle();
  assert.equal(rows().length, 0);
  assert.equal(store.snapshot().length, 6000);
  assert.ok(store.snapshot().every((line) => line.instanceId === "a"));
  app.unmount();
  await settle();
  assert.equal(observer.disconnected, true);
  assert.equal(observer.observed.size, 0);
  const closed = snapshots;
  store.append(entry(12000));
  t.mock.timers.tick(logRefreshMs);
  await settle();
  assert.equal(snapshots, closed);
  store.dispose();
});
