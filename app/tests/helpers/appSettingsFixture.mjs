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
  "@tauri-apps/api/window": `export const getCurrentWindow = () => ({ setTitle: async () => {}, isMinimized: async () => false, minimize: async () => {}, unminimize: async () => {}, setFocus: async () => {}, onDragDropEvent: async callback => { const fixture = globalThis.__settingsFixture; fixture.fileDrop = callback; return () => { fixture.dropUnlistened = true; }; } });`,
  "@tauri-apps/api/event": `export const listen = async (name, callback) => { if (name === 'launcher-event') globalThis.__settingsFixture.event = callback; else globalThis.__settingsFixture.updateEvent = callback; return () => {}; };`,
  "@tauri-apps/plugin-dialog": `export const open = async (...args) => globalThis.__settingsFixture.pickFile?.(...args) ?? null;`,
  "@tauri-apps/plugin-opener": `export const openUrl = async () => {};`,
  "./instanceIcons": `export const resolveInstanceIcon = () => 0;`,
  "./builtinSkins": `export const builtinSkins = [
    { id: "builtin:steve", name: "steve", slim: false, texture: "bundled:steve", builtin: true },
    { id: "builtin:cirno", name: "cirno", slim: true, texture: "bundled:cirno", builtin: true }
  ];`,
  "../package.json": `export const version = ${JSON.stringify(packageVersion)};`,
  "./useInstanceSorting": `import { ref } from ${JSON.stringify(vueUrl)};
    export const useInstanceSorting = instances => ({ strip: ref(), toggle: ref(), expanded: ref(false), ghost: ref(),
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
    window: globalThis.window,
    __settingsFixture: globalThis.__settingsFixture,
  };
  globalThis.window = {
    innerWidth: 900,
    innerHeight: 600,
    devicePixelRatio: 1,
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.document = {
    hidden: false,
    documentElement: { classList: { toggle() {}, remove() {}, add() {} } },
    elementFromPoint: () => globalThis.__settingsFixture.hovered || null,
    getElementById: () => null,
    addEventListener() {},
    removeEventListener() {},
  };
  const fixture = {
    handlers: {},
    calls: [],
    proxyPassword: "",
    passwordRevision: 0,
    state: {
      root: "/fixture",
      system_locale: "zh-CN",
      running: [],
      database: {
        settings: {
          microsoft_client_id: "",
          download_source: "official",
          mod_download_source: "auto",
          proxy_mode: "system",
          proxy_url: "",
          proxy_username: "",
          proxy_password_key: null,
          concurrency: 8,
          adaptive_concurrency: false,
          retries: 2,
          auto_java: true,
          language: "zh-CN",
          apply_language_to_game: false,
        },
        accounts: [],
        selected_account: null,
        guest_name: "Player",
        guest_skin: "builtin:steve",
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
      if (request.op === "copy_text") {
        this.clipboard = request.text;
        return true;
      }
      if (request.op === "snapshot") return this.snapshot();
      if (request.op === "inspect_resources")
        return request.paths.map((path) => ({
          path,
          name: path.split("/").pop(),
          kinds: [],
          format: /\.(mrpack|zip)$/i.test(path) ? "modpack" : "directory",
          issue: null,
        }));
      if (request.op === "proxy_password") return this.proxyPassword;
      if (request.op === "launch_directory")
        return {
          path: "/games",
          mode: request.isolation || "shared",
          changed: false,
          protected_pack: false,
        };
      if (request.op === "memory_options")
        return {
          auto_mb: 6144,
          minimum_mb: 2048,
          recommended_mb: 4096,
          declared_minimum_mb: null,
          total_mb: 16384,
          available_mb: 8192,
        };
      if (request.op === "save_settings") {
        const settings = structuredClone(request.settings);
        settings.proxy_password_key =
          this.state.database.settings.proxy_password_key;
        if (settings.proxy_password !== undefined) {
          this.proxyPassword = settings.proxy_password;
          settings.proxy_password_key = this.proxyPassword
            ? `fixture-${++this.passwordRevision}`
            : null;
        }
        delete settings.proxy_password;
        this.state.database.settings = settings;
        return true;
      }
      if (request.op === "check_update")
        return { available: false, latest_version: "dev0.1", download_url: "" };
      if (request.op === "launcher_update_state")
        return {
          stage: "idle",
          downloaded: 0,
          total: null,
          version: null,
          error: null,
        };
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
