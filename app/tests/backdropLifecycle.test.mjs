import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick } from "vue";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL("../src/components/WorldBackdrop.vue", import.meta.url);
let source = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "backdrop-lifecycle",
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
    `from ${JSON.stringify(name === "../flowRenderer" ? moduleUrl("export const createFlowRenderer=(...args)=>globalThis.__backdropFixture.renderer(...args);") : name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Backdrop = (await import(moduleUrl(source))).default;
function target() {
  return {
    listeners: new Map(),
    addEventListener(type, fn) {
      this.listeners.set(type, fn);
    },
    removeEventListener(type, fn) {
      if (this.listeners.get(type) === fn) this.listeners.delete(type);
    },
    dispatch(type, event = {}) {
      this.listeners.get(type)?.({ type, ...event });
    },
  };
}
function node() {
  return {
    ...target(),
    children: [],
    replaceChildren(...children) {
      this.children = children;
    },
  };
}

test("真实背景组件保留逐帧时钟、隐藏恢复、减少动效和显卡回退，并清理所有资源", async (t) => {
  const keys = [
    "window",
    "document",
    "matchMedia",
    "getComputedStyle",
    "ResizeObserver",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "__backdropFixture",
  ];
  const previous = new Map(
    keys.map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  t.after(() => {
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  const frames = new Map(),
    media = [],
    painters = [];
  let nextFrame = 0,
    observer;
  globalThis.window = { devicePixelRatio: 2 };
  globalThis.document = { ...target(), hidden: false };
  globalThis.matchMedia = (query) => {
    const m = { ...target(), query, matches: false };
    media.push(m);
    return m;
  };
  globalThis.getComputedStyle = () => ({
    getPropertyValue: (name) =>
      ({ "--cyan": "#007ea7", "--blue": "#80ced7", "--aqua": "#9ad1d4" })[name],
  });
  globalThis.ResizeObserver = class {
    constructor(callback) {
      this.callback = callback;
      observer = this;
    }
    observe() {
      this.callback([{ contentRect: { width: 900, height: 600 } }]);
    }
    disconnect() {
      this.disconnected = true;
    }
  };
  globalThis.requestAnimationFrame = (callback) => {
    const id = ++nextFrame;
    frames.set(id, callback);
    return id;
  };
  globalThis.cancelAnimationFrame = (id) => frames.delete(id);
  globalThis.__backdropFixture = {
    renderer(graph, palette, fallback) {
      const p = {
        canvas: node(),
        kind: fallback ? "2d" : "webgl",
        paintCalls: [],
        sizes: [],
        resize(...size) {
          this.sizes.push(size);
          return true;
        },
        paint(flow, drift, still) {
          this.paintCalls.push({ time: flow.time, drift, still });
        },
        restore() {
          if (this.failRestore) throw new Error("lost GPU");
          this.restored = true;
        },
        dispose() {
          this.disposed = true;
        },
      };
      painters.push(p);
      return p;
    },
  };
  const host = createRenderer({
    createElement: node,
    createText: node,
    createComment: node,
    insert(child, parent) {
      parent.children.push(child);
      child.parent = parent;
    },
    remove(child) {
      child.parent.children = child.parent.children.filter((c) => c !== child);
    },
    parentNode: (n) => n.parent,
    nextSibling: () => null,
    setText() {},
    setElementText() {},
    patchProp() {},
  });
  const app = host.createApp(Backdrop, { theme: "tide", paused: false });
  app.mount(node());
  function frame(now) {
    const [id, callback] = frames.entries().next().value;
    frames.delete(id);
    callback(now);
  }
  const painter = painters[0];
  assert.deepEqual(painter.sizes.at(-1), [900, 600, 2]);
  assert.equal(frames.size, 1);
  frame(1000);
  frame(1016);
  assert.equal(painter.paintCalls.at(-1).time, 0.016);
  document.hidden = true;
  document.dispatch("visibilitychange");
  assert.equal(frames.size, 0);
  document.hidden = false;
  document.dispatch("visibilitychange");
  frame(5000);
  frame(5016);
  assert.equal(painter.paintCalls.at(-1).time, 0.032);
  assert.equal(frames.size, 1);
  const motion = media.find((m) => m.query.includes("prefers-reduced-motion"));
  motion.matches = true;
  motion.dispatch("change");
  assert.equal(frames.size, 0);
  assert.equal(painter.paintCalls.at(-1).still, true);
  assert.equal(painter.paintCalls.at(-1).drift, 0);
  motion.matches = false;
  motion.dispatch("change");
  assert.equal(frames.size, 1);
  let prevented = false;
  painter.canvas.dispatch("webglcontextlost", {
    preventDefault() {
      prevented = true;
    },
  });
  assert.equal(prevented, true);
  assert.equal(frames.size, 0);
  document.dispatch("visibilitychange");
  assert.equal(frames.size, 0);
  painter.canvas.dispatch("webglcontextrestored");
  assert.equal(painter.restored, true);
  assert.equal(frames.size, 1);
  painter.failRestore = true;
  painter.canvas.dispatch("webglcontextlost", { preventDefault() {} });
  painter.canvas.dispatch("webglcontextrestored");
  assert.equal(painters[1].kind, "2d");
  assert.equal(painter.disposed, true);
  assert.equal(painter.canvas.listeners.size, 0);
  assert.equal(frames.size, 1);
  window.devicePixelRatio = 1.25;
  media.find((m) => m.query.includes("resolution")).dispatch("change");
  assert.deepEqual(painters[1].sizes.at(-1), [900, 600, 1.25]);
  app.unmount();
  await nextTick();
  assert.equal(frames.size, 0);
  assert.equal(observer.disconnected, true);
  assert.ok(painters.every((p) => p.disposed && p.canvas.listeners.size === 0));
  assert.equal(document.listeners.size, 0);
  assert.ok(media.every((m) => m.listeners.size === 0));
});
