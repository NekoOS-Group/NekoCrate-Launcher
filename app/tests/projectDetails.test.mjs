import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive } from "vue";
import { deferred, flush } from "./helpers/appSettingsFixture.mjs";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL("../src/components/ProjectDetails.vue", import.meta.url);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "project-details-test",
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
const project = (id) => ({
  id,
  name: id,
  description: "",
  downloads: 0,
  url: "",
});
const details = (id, body, stale = false) => ({
  project: project(id),
  body,
  refresh_required: stale,
  authors: [],
  links: [],
  gallery: [],
  categories: [],
});
function fixture(rpc) {
  const props = reactive({
    project: project("cat"),
    platform: "modrinth",
    rpc,
  });
  const events = [];
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
        emit: (...event) => events.push(event),
      });
      return () => null;
    },
  });
  app.mount({});
  return { props, ui, events, dispose: () => app.unmount() };
}

test("cached body remains visible without a loading state during revalidation and offline failure", async () => {
  const fresh = deferred();
  const calls = [];
  const h = fixture((_op, data) => {
    calls.push(data);
    return data.refresh
      ? fresh.promise
      : Promise.resolve(details("cat", "cached", true));
  });
  try {
    await flush();
    assert.equal(h.ui.detail.value.body, "cached");
    assert.equal(h.ui.loading.value, false);
    assert.deepEqual(
      calls.map((c) => c.refresh),
      [false, true],
    );
    fresh.reject(new Error("offline"));
    await flush();
    assert.equal(h.ui.detail.value.body, "cached");
    assert.equal(h.ui.failed.value, false);
  } finally {
    h.dispose();
  }
});

test("same-project updates retain the body and late refreshes cannot overwrite another project", async () => {
  const fresh = deferred();
  const h = fixture((_op, data) =>
    data.refresh
      ? fresh.promise
      : Promise.resolve({
          ...details(data.project, data.project),
          updated: "2026-11-01T00:00:00Z",
        }),
  );
  try {
    await flush();
    h.props.project.updated = "2026-11-02T00:00:00Z";
    await flush();
    assert.equal(h.ui.detail.value.body, "cat");
    assert.equal(h.ui.loading.value, false);
    h.props.project = project("dog");
    await flush();
    assert.equal(h.ui.detail.value.body, "dog");
    fresh.resolve(details("cat", "new cat"));
    await flush();
    assert.equal(h.ui.detail.value.body, "dog");
    assert.equal(h.events.at(-1)[1].project.id, "dog");
  } finally {
    h.dispose();
  }
});
