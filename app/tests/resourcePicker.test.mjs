import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, shallowReactive, nextTick } from "vue";
const file = new URL("../src/components/ResourcePicker.vue", import.meta.url);
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "resource-picker",
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
    `from ${JSON.stringify(name.endsWith(".vue") ? moduleUrl("export default {}") : name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Picker = (await import(moduleUrl(code))).default;
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve };
};
function fixture(rpc) {
  const events = [];
  const props = shallowReactive({
    minecraft: "1.20.1",
    loader: { kind: "fabric", version: "0.16.10" },
    modelValue: [],
    platform: "modrinth",
    disabled: false,
    rpc,
  });
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
      ui = Picker.setup(props, {
        expose() {},
        emit(event, value) {
          events.push({ event, value });
          if (event === "update:modelValue") props.modelValue = value;
        },
      });
      return () => null;
    },
  });
  app.mount({});
  return { ui, props, events, dispose: () => app.unmount() };
}
const versions = (project) => [
  { id: `${project}-beta`, project, number: "beta", release_type: "beta" },
  {
    id: `${project}-stable`,
    project,
    number: "stable",
    release_type: "release",
  },
];

test("picker searches a future game without an instance and accumulates concurrent resource choices", async () => {
  const calls = [],
    cat = deferred(),
    ice = deferred();
  const h = fixture((op, data) => {
    calls.push({ op, data });
    if (op === "search_resource_catalog")
      return Promise.resolve({ hits: [], total: 0 });
    return data.project === "cat" ? cat.promise : ice.promise;
  });
  try {
    await h.ui.search();
    assert.deepEqual(calls[0].data.scope, {
      minecraft: "1.20.1",
      loader: h.props.loader,
      kind: "mods",
      platform: "modrinth",
    });
    const a = h.ui.toggle({ id: "cat", name: "Cat" });
    const b = h.ui.toggle({ id: "ice", name: "Ice" });
    cat.resolve(versions("cat"));
    await a;
    ice.resolve(versions("ice"));
    await b;
    assert.deepEqual(
      h.props.modelValue.map((p) => p.version),
      ["cat-stable", "ice-stable"],
    );
    await h.ui.toggle({ id: "cat", name: "Cat" });
    assert.deepEqual(
      h.props.modelValue.map((p) => p.project),
      ["ice"],
    );
    assert.equal(
      calls.some((c) => c.op.includes("install") || c.op === "resource_target"),
      false,
    );
  } finally {
    h.dispose();
  }
});

test("picker ignores late incompatible responses and never commits after it is closed", async () => {
  const pending = deferred();
  const h = fixture(() => pending.promise);
  const adding = h.ui.toggle({ id: "cat", name: "Cat" });
  h.props.minecraft = "1.21.1";
  await nextTick();
  pending.resolve(versions("cat"));
  await adding;
  assert.equal(h.props.modelValue.length, 0);
  h.dispose();
  const afterClose = deferred();
  const other = fixture(() => afterClose.promise);
  const late = other.ui.toggle({ id: "ice", name: "Ice" });
  other.dispose();
  afterClose.resolve(versions("ice"));
  await late;
  assert.equal(other.props.modelValue.length, 0);
});

test("local drags arriving during an online lookup cannot exceed the combined selection limit", async () => {
  const request = deferred();
  const h = fixture(() => request.promise);
  try {
    const adding = h.ui.toggle({ id: "cat", name: "Cat" });
    h.props.localResources = Array.from({ length: 128 }, (_, i) => ({
      file: `${i}.zip`,
    }));
    request.resolve(versions("cat"));
    await adding;
    assert.equal(h.props.modelValue.length, 0);
    assert.ok(
      h.events.some(
        (event) => event.event === "error" && event.value.includes("128"),
      ),
    );
  } finally {
    h.dispose();
  }
});
