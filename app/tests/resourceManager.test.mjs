import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive, nextTick } from "vue";
import { resourceRemovalKey } from "../src/useResourceTransfer.ts";

const file = new URL("../src/components/ResourceManager.vue", import.meta.url);
const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "library-manager",
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
    `from ${JSON.stringify(
      name === "../builtinSkins"
        ? moduleUrl("export const builtinSkins = []")
        : name.endsWith(".vue")
          ? moduleUrl("export default {}")
          : name.startsWith(".")
            ? new URL(`${name}.ts`, file).href
            : import.meta.resolve(name),
    )}`,
);
const Component = (await import(moduleUrl(code))).default;
const flush = async () => {
  await new Promise(setImmediate);
  await nextTick();
};
function fixture(handlers = {}, remove = async () => {}) {
  const calls = [],
    events = [];
  const rpc = async (op, data) => {
    calls.push({ op, data });
    if (handlers[op]) return handlers[op](data);
    if (data?.action?.kind === "groups")
      return {
        groups: [
          { directory: "/library", global: true, owners: [] },
          {
            directory: "/game",
            global: false,
            owners: [{ id: "a", version: "v", label: "Game" }],
          },
        ],
        issues: [],
      };
    if (data?.action?.kind === "cached_resources")
      return { directory: "/library/mods", files: [] };
    if (data?.action?.kind === "cache_resources") return 1;
    return [];
  };
  const props = reactive({
    rpc,
    manifest: null,
    busy: false,
    running: [],
    modActions: {},
    installJava: async () => {},
    accounts: [],
    selectedAccount: null,
    guestSkin: "builtin:steve",
    guestName: "Guest",
    selectSkin: async () => null,
    loadAccountSkin: async () => null,
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
  app.provide(resourceRemovalKey, remove);
  app.mount({});
  return { ui, props, calls, events, dispose: () => app.unmount() };
}

test("only cached library resources can be deleted, never a mixed selection", async () => {
  const removed = [];
  const h = fixture({}, async (sources) => removed.push(sources));
  try {
    await flush();
    const cached = {
      group: "library",
      key: "cache",
      global: true,
      directory: "/library/mods",
      owners: [],
      file: { name: "Cat", file_name: "cat.jar", enabled: true },
    };
    const installed = {
      ...cached,
      group: "game",
      key: "local",
      global: false,
      owners: [{ id: "a", version: "v", label: "Game" }],
    };
    h.ui.resources.value = [cached, installed];
    h.ui.readyGroups.value = new Set(["library", "game"]);
    h.ui.selection.click({ ctrlKey: true }, installed);
    assert.equal(h.ui.canRemoveSelected.value, false);
    await h.ui.removeSelectedResources();
    h.ui.selection.click({ ctrlKey: true }, cached);
    assert.equal(h.ui.canRemoveSelected.value, false);
    await h.ui.removeSelectedResources();
    assert.deepEqual(removed, []);
    assert.equal(h.ui.resourceSource(installed).removable, false);
    h.ui.selection.click({ ctrlKey: true }, installed);
    assert.equal(h.ui.canRemoveSelected.value, true);
    await h.ui.removeSelectedResources();
    assert.equal(removed.length, 1);
    assert.deepEqual(
      removed[0].map((source) => [source.global, source.removable]),
      [[true, true]],
    );
  } finally {
    h.dispose();
  }
});

test("Java removal accepts only managed inventory entries and rechecks them at confirmation time", async () => {
  const h = fixture();
  try {
    await flush();
    const runtime = {
      path: "/cache/jdk-21/bin/java",
      source: "managed",
      major: 21,
      version: "21.0.1",
      vendor: "Temurin",
    };
    h.ui.runtimes.value = [runtime];
    for (const source of ["system", "registered", "configured"]) {
      h.ui.removeJava({ ...runtime, path: "/system/bin/java", source });
      assert.equal(h.ui.confirm.value, undefined);
    }
    h.ui.removeJava(runtime);
    assert.ok(h.ui.confirm.value);
    h.ui.runtimes.value = [];
    await h.ui.accept();
    assert.equal(
      h.calls.some((call) => call.data?.action?.kind === "remove_java"),
      false,
    );
    h.ui.runtimes.value = [runtime];
    h.props.running = ["game"];
    h.ui.removeJava(runtime);
    assert.equal(h.ui.confirm.value, undefined);
    h.props.running = [];
    h.ui.removeJava(runtime);
    await h.ui.accept();
    assert.deepEqual(
      h.calls.find((call) => call.data?.action?.kind === "remove_java").data
        .action,
      { kind: "remove_java", path: runtime.path },
    );
    assert.ok(h.events.some((event) => event[0] === "java-changed"));
  } finally {
    h.dispose();
  }
});
test("cache navigation clears stale filters and opens only the requested collected category", async () => {
  const h = fixture();
  try {
    await flush();
    h.ui.query.value = "stale";
    h.ui.browsing.value = true;
    h.ui.showCachePart("datapacks");
    await flush();
    assert.equal(h.ui.tab.value, "resources");
    assert.equal(h.ui.resourceKind.value, "datapacks");
    assert.equal(h.ui.cachedOnly.value, true);
    assert.equal(h.ui.browsing.value, false);
    assert.equal(h.ui.query.value, "");
    assert.equal(
      h.calls.some((c) => c.op === "version_content"),
      false,
    );
    assert.equal(
      h.calls.find((c) => c.data?.action?.kind === "cached_resources").data
        .action.content,
      "datapacks",
    );
    h.ui.showCachePart("java");
    await flush();
    assert.equal(h.ui.tab.value, "java");
    h.ui.showCachePart("skins");
    await flush();
    assert.equal(h.ui.tab.value, "skins");
  } finally {
    h.dispose();
  }
});
test("external import targets the selected library category and refreshes the collected view", async () => {
  const h = fixture();
  try {
    await flush();
    await h.ui.importResources(["/shader.zip"], "shaderpacks");
    const call = h.calls.find(
      (c) => c.data?.action?.kind === "cache_resources",
    );
    assert.deepEqual(call.data.action.inputs, [
      { path: "/shader.zip", kind: "shaderpacks" },
    ]);
    assert.equal(h.ui.tab.value, "resources");
    assert.equal(h.ui.cachedOnly.value, true);
    assert.equal(h.ui.resourceKind.value, "shaderpacks");
    assert.equal(h.ui.mutating.value, false);
    assert.deepEqual(
      h.events.filter((e) => e[0] === "busy"),
      [
        ["busy", true],
        ["busy", false],
      ],
    );
  } finally {
    h.dispose();
  }
});

test("cache filtering retains the indexed list and updates the counter without another scan", async () => {
  const file = (name) => ({ name, file_name: name, enabled: true });
  const h = fixture({
    resource_manager: async ({ action }) =>
      action.kind === "groups"
        ? {
            groups: [
              { directory: "/library", global: true, owners: [] },
              {
                directory: "/game",
                global: false,
                owners: [{ id: "a", version: "v", label: "Game" }],
              },
            ],
            issues: [],
          }
        : action.kind === "cached_resources"
          ? { directory: "/library/mods", files: [file("cat.jar")] }
          : [],
    version_content_index: async () => ({
      directory: "/game/mods",
      files: [file("one.jar"), file("two.jar")],
      pending: [],
    }),
  });
  try {
    await flush();
    h.ui.tab.value = "resources";
    await flush();
    assert.equal(h.ui.resourceCount.value, 3);
    const calls = h.calls.length;
    h.ui.cachedOnly.value = true;
    await flush();
    assert.equal(h.ui.resourceCount.value, 1);
    assert.equal(h.ui.resources.value.length, 3);
    h.ui.cachedOnly.value = false;
    await flush();
    assert.equal(h.ui.resourceCount.value, 3);
    assert.equal(h.calls.length, calls);
    h.ui.browseResources();
    assert.equal(h.ui.browsing.value, true);
    assert.equal(h.ui.tab.value, "resources");
    h.ui.libraryDownloading.value = true;
    await h.ui.closeBrowser();
    assert.equal(h.ui.browsing.value, true);
  } finally {
    h.dispose();
  }
});
