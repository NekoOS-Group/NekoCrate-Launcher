import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, shallowReactive, nextTick } from "vue";
const file = new URL("../src/components/ResourceBrowser.vue", import.meta.url);
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "resource-browser",
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
const Component = (await import(moduleUrl(code))).default;
const flush = async () => {
  await new Promise(setImmediate);
  await nextTick();
};
const versions = [
  { id: "beta", number: "beta", release_type: "beta" },
  { id: "stable", number: "stable", release_type: "release" },
];
function fixture(overrides = {}) {
  const calls = [],
    events = [];
  const props = shallowReactive({
    library: {
      minecraft: "1.20.1",
      loader: { kind: "fabric", version: "" },
      kind: "mods",
    },
    busy: false,
    rpc: async (op, data) => {
      calls.push({ op, data });
      if (op.endsWith("versions")) return versions;
      if (op === "resource_target")
        return {
          minecraft: "1.20.1",
          loader: { kind: "fabric" },
          worlds: ["World"],
          directory: "/game",
        };
      if (op === "resource_plans") return [];
      return { hits: [], total: 0 };
    },
    ...overrides,
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
      ui = Component.setup(props, {
        expose() {},
        emit: (...args) => events.push(args),
      });
      return () => null;
    },
  });
  app.mount({});
  return { props, ui, calls, events, dispose: () => app.unmount() };
}

test("the shared browser searches and downloads to the library without querying an instance", async () => {
  const downloads = [];
  const f = fixture({
    download: async (...args) => {
      downloads.push(args);
      return true;
    },
  });
  try {
    await flush();
    assert.equal(f.calls[0].op, "search_resource_catalog");
    await f.ui.selectProject({ id: "cat", name: "Cat" });
    assert.equal(f.ui.chosen.value, "stable");
    f.ui.chosen.value = "beta";
    await f.ui.preview();
    assert.deepEqual(downloads[0], [
      {
        minecraft: "1.20.1",
        loader: { kind: "fabric", version: "" },
        platform: "modrinth",
        kind: "mods",
      },
      "cat",
      "beta",
    ]);
    assert.equal(f.ui.complete.value, true);
    assert.equal(
      f.calls.some((c) =>
        ["resource_target", "resource_plans", "prepare_resource"].includes(
          c.op,
        ),
      ),
      false,
    );
    assert.equal(
      f.ui.categories.value.some((c) => c.value === "saves"),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("library imports keep detected types and datapacks do not require a world", async () => {
  const imports = [];
  const f = fixture({
    importResources: async (inputs) => {
      imports.push(inputs);
      return true;
    },
  });
  try {
    await flush();
    f.ui.setFiles([
      {
        path: "/pack.zip",
        name: "pack.zip",
        format: "resource",
        kinds: ["datapacks"],
        issue: null,
      },
    ]);
    await flush();
    assert.equal(f.ui.worldRequired.value, false);
    assert.equal(f.ui.canPreview.value, true);
    await f.ui.preview();
    assert.deepEqual(imports, [[{ path: "/pack.zip", kind: "datapacks" }]]);
    f.ui.setFiles([
      {
        path: "/map.zip",
        name: "map.zip",
        format: "resource",
        kinds: ["saves"],
        issue: null,
      },
    ]);
    await flush();
    assert.equal(f.ui.canPreview.value, false);
    f.ui.setFiles([
      {
        path: "/both.zip",
        name: "both.zip",
        format: "resource",
        kinds: ["resourcepacks", "datapacks"],
        issue: null,
      },
    ]);
    assert.equal(f.ui.canPreview.value, false);
    f.ui.selectedKinds.value["/both.zip"] = "resourcepacks";
    assert.equal(f.ui.canPreview.value, true);
  } finally {
    f.dispose();
  }
});

test("version management retains its target, world selection and preview/install workflow", async () => {
  const preparations = [],
    installs = [];
  const plan = {
    id: "plan",
    target: { instance: "a", version: "1.20.1" },
    entries: [],
    conflicts: [],
    warnings: [],
  };
  const f = fixture({
    library: undefined,
    session: {
      instance: { id: "a", name: "Games", versions: [{ id: "1.20.1" }] },
      version: "1.20.1",
      kind: "datapacks",
    },
    prepare: async (...args) => {
      preparations.push(args);
      return plan;
    },
    install: async (p) => {
      installs.push(p);
      return true;
    },
  });
  try {
    await flush();
    assert.equal(f.calls[0].op, "resource_target");
    await f.ui.selectProject({ id: "cat", name: "Cat" });
    await f.ui.preview();
    assert.deepEqual(preparations[0], [
      "prepare_resource",
      {
        scope: {
          id: "a",
          version: "1.20.1",
          platform: "modrinth",
          kind: "datapacks",
        },
        project: "cat",
        version: "stable",
        world: "World",
      },
    ]);
    await f.ui.install();
    assert.deepEqual(installs, [plan]);
    assert.deepEqual(
      f.events.find((e) => e[0] === "installed"),
      ["installed", plan],
    );
  } finally {
    f.dispose();
  }
});

test("changing library compatibility discards stale project-version responses", async () => {
  let resolve;
  const pending = new Promise((r) => (resolve = r));
  const f = fixture({
    rpc: async (op) =>
      op === "catalog_resource_versions" ? pending : { hits: [], total: 0 },
  });
  try {
    await flush();
    const selecting = f.ui.selectProject({ id: "cat", name: "Cat" });
    f.props.library = { ...f.props.library, minecraft: "1.21.1" };
    await flush();
    resolve(versions);
    await selecting;
    assert.equal(f.ui.project.value, undefined);
    assert.equal(f.ui.chosen.value, "");
    assert.equal(f.ui.loadingVersions.value, false);
  } finally {
    f.dispose();
  }
});
