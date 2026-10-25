import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive } from "vue";
import { deferred, flush } from "./helpers/appSettingsFixture.mjs";

const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL(
  "../src/components/VersionMetadataPanel.vue",
  import.meta.url,
);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "version-manager-test",
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
      name.endsWith(".vue")
        ? url("export default {};")
        : name === "../instanceIcons"
          ? url("export const resolveInstanceIcon = () => 0;")
          : name.startsWith(".")
            ? new URL(`${name}.ts`, file).href
            : import.meta.resolve(name),
    )}`,
);
const Panel = (await import(url(code))).default;

function panel(options = {}) {
  const events = [];
  const props = reactive({
    version: "first",
    refresh: 0,
    loadMetadata: async () => ({ documents: [], minecraft_version: "1.20.1" }),
    ...options,
  });
  const host = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  let ui;
  const app = host.createApp({
    setup() {
      ui = Panel.setup(props, {
        expose() {},
        emit: (...args) => events.push(args),
      });
      return () => null;
    },
  });
  app.mount({});
  return { props, ui, events, dispose: () => app.unmount() };
}

const document = (id, extra = {}) => ({
  id,
  name: `${id}.json`,
  path: `/game/versions/${id}/${id}.json`,
  kind: "版本配置",
  json: '{"javaVersion":{"majorVersion":17}}',
  issue: null,
  fields: [{ label: "Java 版本", value: "17" }],
  sections: [],
  ...extra,
});
const result = (id) => ({ game_directory: "/game", documents: [document(id)] });

test("switching versions ignores stale results and opens the new profile by default", async () => {
  const old = deferred();
  const h = panel({
    loadMetadata: (id) =>
      id === "first" ? old.promise : Promise.resolve(result(id)),
  });
  try {
    h.props.version = "second";
    await flush();
    old.reject(new Error("old version unavailable"));
    await flush();
    assert.equal(h.ui.metadata.value.documents[0].id, "second");
    assert.equal(h.ui.opened.value, document("second").path);
    assert.equal(
      h.ui.location(document("second")),
      "versions/second/second.json",
    );
    assert.equal(h.ui.failed.value, false);
    assert.deepEqual(h.events, []);
  } finally {
    h.dispose();
  }
});

test("file accordion keeps parsed fields primary, lazily formats raw JSON and preserves refresh selection", async () => {
  const h = panel({
    loadMetadata: async () => ({
      ...result("first"),
      documents: [
        document("first"),
        document("manifest", { json: "{broken", issue: "invalid JSON" }),
      ],
    }),
  });
  try {
    await flush();
    assert.equal(h.ui.opened.value, document("first").path);
    assert.equal(h.ui.raw.value, false);
    assert.equal(h.ui.sourceText.value, "");
    h.ui.raw.value = true;
    assert.equal(
      JSON.parse(h.ui.sourceText.value).javaVersion.majorVersion,
      17,
    );
    h.ui.toggle(document("manifest"));
    assert.equal(h.ui.raw.value, false);
    h.ui.raw.value = true;
    assert.equal(h.ui.sourceText.value, "{broken");
    h.props.refresh++;
    await flush();
    assert.equal(h.ui.opened.value, document("manifest").path);
    h.ui.toggle(document("manifest"));
    h.props.refresh++;
    await flush();
    assert.equal(h.ui.opened.value, null); // Refresh must not reopen a deliberately closed file.
    h.props.version = "second";
    h.props.loadMetadata = async () => result("second");
    await flush();
    assert.equal(h.ui.opened.value, document("second").path);
    assert.equal(h.ui.raw.value, false);
  } finally {
    h.dispose();
  }
});

test("unmount discards pending metadata failures", async () => {
  const pending = deferred();
  const h = panel({ loadMetadata: () => pending.promise });
  h.dispose();
  pending.reject(new Error("old directory unavailable"));
  await flush();
  assert.deepEqual(h.events, []);
});
