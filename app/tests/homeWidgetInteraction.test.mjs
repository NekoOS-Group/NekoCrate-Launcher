import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive, nextTick } from "vue";
import { HITOKOTO_URL, WIDGET_CACHE_KEY } from "../src/homeWidgets.ts";

const file = new URL("../src/components/HomeWidget.vue", import.meta.url);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "quote-startup",
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
const Component = (
  await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  )
).default;
const oldQuote = {
  hitokoto: "这是重启前保存的一句话。",
  type: "a",
  from: "旧句子",
};
const newQuote = {
  hitokoto: "这是重新启动后的一句话。",
  type: "a",
  from: "新句子",
};
const response = (quote) => ({
  url: HITOKOTO_URL,
  text: JSON.stringify(quote),
});
const flush = async () => {
  await new Promise(setImmediate);
  await nextTick();
};
function fixture({ editing = false, hidden = false, rpc } = {}) {
  const keys = ["document", "ResizeObserver", "localStorage"];
  const previous = Object.fromEntries(
    keys.map((k) => [k, Object.getOwnPropertyDescriptor(globalThis, k)]),
  );
  const listeners = new Map(),
    storage = new Map([
      [
        WIDGET_CACHE_KEY,
        JSON.stringify({
          [HITOKOTO_URL]: { time: Date.now(), data: response(oldQuote) },
        }),
      ],
    ]);
  Object.defineProperties(globalThis, {
    document: {
      configurable: true,
      value: {
        hidden,
        addEventListener: (name, fn) => listeners.set(name, fn),
        removeEventListener: (name) => listeners.delete(name),
      },
    },
    ResizeObserver: {
      configurable: true,
      value: class {
        observe() {}
        disconnect() {}
      },
    },
    localStorage: {
      configurable: true,
      value: {
        getItem: (k) => storage.get(k),
        setItem: (k, v) => storage.set(k, v),
      },
    },
  });
  const calls = [];
  let resolveRequest;
  const props = reactive({
    kind: "quote",
    quoteStyle: "simple",
    editing,
    rpc: async (op, data) => {
      calls.push({ op, ...data });
      return rpc
        ? rpc()
        : new Promise((resolve) => {
            resolveRequest = resolve;
          });
    },
  });
  let ui;
  const app = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  }).createApp({
    setup() {
      ui = Component.setup(props, { expose() {}, emit() {} });
      return () => null;
    },
  });
  app.mount({});
  return {
    ui,
    props,
    calls,
    storage,
    listeners,
    resolve: () => resolveRequest(response(newQuote)),
    dispose() {
      app.unmount();
      for (const key of keys)
        if (previous[key])
          Object.defineProperty(globalThis, key, previous[key]);
        else delete globalThis[key];
    },
  };
}

test("startup paints a cached quote then requests a new one, without re-fetching on focus/style/edit changes", async () => {
  const f = fixture();
  try {
    assert.equal(f.ui.quote.value.text, oldQuote.hitokoto);
    assert.deepEqual(f.calls, [{ op: "home_widget_feed", url: HITOKOTO_URL }]);
    f.listeners.get("visibilitychange")();
    assert.equal(f.calls.length, 1);
    f.resolve();
    await flush();
    assert.equal(f.ui.quote.value.text, newQuote.hitokoto);
    f.listeners.get("visibilitychange")();
    f.props.quoteStyle = "splash";
    f.props.editing = true;
    await flush();
    f.props.editing = false;
    await flush();
    assert.equal(f.calls.length, 1);
    assert.equal(
      JSON.parse(
        JSON.parse(f.storage.get(WIDGET_CACHE_KEY))[HITOKOTO_URL].data.text,
      ).hitokoto,
      newQuote.hitokoto,
    );
  } finally {
    f.dispose();
  }
  assert.equal(f.listeners.size, 0);
  // A new component lifecycle represents a new launcher window with a fresh disk cache.
  const restarted = fixture();
  try {
    assert.equal(restarted.calls.length, 1);
    restarted.resolve();
    await flush();
  } finally {
    restarted.dispose();
  }
});

test("hidden or editing startup postpones the first refresh; offline launch keeps its cached quote", async () => {
  const f = fixture({ editing: true, hidden: true });
  try {
    assert.equal(f.calls.length, 0);
    f.props.editing = false;
    await flush();
    assert.equal(f.calls.length, 0);
    document.hidden = false;
    f.listeners.get("visibilitychange")();
    assert.equal(f.calls.length, 1);
    f.resolve();
    await flush();
  } finally {
    f.dispose();
  }
  const offline = fixture({ rpc: () => Promise.reject(Error("offline")) });
  try {
    await flush();
    assert.equal(offline.ui.quote.value.text, oldQuote.hitokoto);
    assert.equal(offline.ui.quoteProblem.value, true);
    assert.equal(offline.calls.length, 1);
    assert.equal(
      JSON.parse(
        JSON.parse(offline.storage.get(WIDGET_CACHE_KEY))[HITOKOTO_URL].data
          .text,
      ).hitokoto,
      oldQuote.hitokoto,
    );
  } finally {
    offline.dispose();
  }
});
