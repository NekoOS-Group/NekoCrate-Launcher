import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick } from "vue";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const vueUrl = import.meta.resolve("vue");
const packageVersion = JSON.parse(
  readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
).version;
const stubs = {
  "@tauri-apps/api/core": `export const isTauri = () => true; export const invoke = (...args) => globalThis.__settingsFixture.invoke(...args);`,
  "@tauri-apps/api/window": `export const getCurrentWindow = () => ({ setTitle: async () => {} });`,
  "@tauri-apps/api/event": `export const listen = async (_, callback) => { globalThis.__settingsFixture.event = callback; return () => {}; };`,
  "@tauri-apps/plugin-dialog": `export const open = async () => null;`,
  "@tauri-apps/plugin-opener": `export const openUrl = async () => {};`,
  "./instanceIcons": `export const resolveInstanceIcon = () => 0;`,
  "../package.json": `export const version = ${JSON.stringify(packageVersion)};`,
  "./useInstanceSorting": `import { ref } from ${JSON.stringify(vueUrl)};
    export const useInstanceSorting = instances => ({ strip: ref(), ghost: ref(),
      draggedId: ref(null), draggedInstance: ref(), displayInstances: instances,
      ghostPosition: ref({}), ghostArtStyle: ref({}) });`,
};
const file = new URL("../../src/App.vue", import.meta.url);
const compiled = compileScript(parse(readFileSync(file, "utf8")).descriptor, {
  id: "settings-regression",
});
let code = ts.transpileModule(compiled.content, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
  },
}).outputText;
code = code.replace(/from ["']([^"']+)["']/g, (_, name) => {
  const url =
    name in stubs
      ? moduleUrl(stubs[name])
      : name.endsWith(".vue")
        ? moduleUrl("export default {};")
        : name.startsWith(".")
          ? new URL(`${name}.ts`, file).href
          : import.meta.resolve(name);
  return `from ${JSON.stringify(url)}`;
});
const App = (await import(moduleUrl(code))).default;

export function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export const flush = async () => {
  await new Promise(setImmediate);
  await nextTick();
};

/** Run App's real setup/actions against controlled IPC; no browser or live data. */
export async function settingsFixture() {
  const previous = {
    document: globalThis.document,
    __settingsFixture: globalThis.__settingsFixture,
  };
  globalThis.document = {
    hidden: false,
    documentElement: {},
    getElementById: () => null,
    addEventListener() {},
    removeEventListener() {},
  };
  const fixture = {
    handlers: {},
    calls: [],
    state: {
      root: "/fixture",
      system_locale: "zh-CN",
      running: [],
      database: {
        settings: {
          microsoft_client_id: "",
          download_source: "official",
          concurrency: 8,
          retries: 2,
          auto_java: true,
          language: "zh-CN",
          apply_language_to_game: false,
        },
        accounts: [],
        selected_account: null,
        instances: [
          {
            id: "instance-a",
            name: "World",
            versions: [{ id: "1.21.1", installed: false }],
            selected_version: "1.21.1",
            memory_mb: 2048,
            java_path: null,
            created_at: 0,
            last_played: null,
          },
        ],
      },
    },
    snapshot() {
      return {
        state: structuredClone(this.state),
        manifest: null,
        busy: false,
      };
    },
    async invoke(_, { request }) {
      this.calls.push(request);
      if (this.handlers[request.op]) return this.handlers[request.op](request);
      if (request.op === "snapshot") return this.snapshot();
      if (request.op === "save_settings") {
        this.state.database.settings = structuredClone(request.settings);
        return true;
      }
      if (request.op === "check_update")
        return { available: false, latest_version: "dev0.1", download_url: "" };
      throw new Error(`Unexpected IPC: ${request.op}`);
    },
  };
  globalThis.__settingsFixture = fixture;
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  let context;
  const app = renderer.createApp({
    setup() {
      context = App.setup({}, { expose() {} });
      return () => null;
    },
  });
  app.mount({});
  await flush();
  return {
    ...fixture,
    fixture,
    context,
    dispose() {
      app.unmount();
      Object.assign(globalThis, previous);
    },
  };
}
