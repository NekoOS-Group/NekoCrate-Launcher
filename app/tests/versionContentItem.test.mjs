import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive } from "vue";
import { deferred, flush } from "./helpers/appSettingsFixture.mjs";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL(
  "../src/components/VersionContentItem.vue",
  import.meta.url,
);
async function compileItem(inlineTemplate = false) {
  let code = ts.transpileModule(
    compileScript(parse(readFileSync(file, "utf8")).descriptor, {
      id: "content-detail-test",
      inlineTemplate,
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
      `from ${JSON.stringify(name.endsWith(".vue") ? moduleUrl("export default {setup:()=>()=>null}") : name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
  );
  return (await import(moduleUrl(code))).default;
}
const Item = await compileItem();
const RenderedItem = await compileItem(true);
const details = (path) => ({
  path,
  size: 123,
  sources: [
    {
      source: "fabric.mod.json",
      revealable: false,
      title: null,
      description: null,
      fields: [],
      dependencies: [],
      issue: null,
    },
  ],
});
function item(options = {}, component = Item) {
  const events = [];
  const props = reactive({
    instanceId: "a",
    version: "pack",
    kind: "mods",
    icon: {},
    file: {
      name: "Example",
      file_name: "example.jar",
      version: "1",
      enabled: true,
      size: 123,
    },
    loadMetadata: async () => details("/mods/example.jar"),
    openFolder: async () => {},
    loadSource: async () => "{}",
    dependencyTargets: new Map(),
    ...options,
  });
  props.file = { has_metadata: true, ...props.file };
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
      ui = component.setup(props, {
        expose() {},
        emit: (...event) => events.push(event),
      });
      return () => null;
    },
  });
  app.mount({});
  const cache = [];
  return {
    props,
    ui,
    events,
    render: () => ui({}, cache),
    dispose: () => app.unmount(),
  };
}

test("all four content types load only on expansion and reuse pending or cached details", async () => {
  for (const kind of ["mods", "datapacks", "resourcepacks", "saves"]) {
    const pending = deferred(),
      calls = [];
    const h = item({
      kind,
      loadMetadata: (...args) => {
        calls.push(args);
        return pending.promise;
      },
    });
    try {
      await flush();
      assert.deepEqual(calls, []);
      const loading = h.ui.toggle();
      assert.equal(h.ui.loading.value, true);
      await h.ui.toggle();
      await h.ui.toggle();
      assert.deepEqual(calls, [["a", "pack", kind, "example.jar"]]);
      pending.resolve(details(`/example/${kind}`));
      await loading;
      await h.ui.toggle();
      await h.ui.toggle();
      assert.equal(calls.length, 1);
      assert.equal(h.ui.metadata.value.path, `/example/${kind}`);
    } finally {
      h.dispose();
    }
  }
});

test("identity changes and unmount discard old data and failures", async () => {
  const pending = deferred();
  const h = item({ loadMetadata: () => pending.promise });
  try {
    const loading = h.ui.toggle();
    h.props.instanceId = "b";
    h.props.kind = "saves";
    await flush();
    assert.equal(h.ui.expanded.value, false);
    h.props.loadMetadata = async () => details("/b/world");
    await h.ui.toggle();
    pending.reject(new Error("old file removed"));
    await loading;
    assert.equal(h.ui.metadata.value.path, "/b/world");
    assert.equal(h.ui.failed.value, false);
    assert.deepEqual(h.events, []);
  } finally {
    h.dispose();
  }
  const later = deferred();
  const closed = item({ loadMetadata: () => later.promise });
  const loading = closed.ui.toggle();
  closed.dispose();
  later.reject(new Error("closed"));
  await loading;
  assert.deepEqual(closed.events, []);
});

test("a failed detail read can be retried and 64-bit seeds remain exact", async () => {
  const h = item({
    loadMetadata: async () => {
      throw new Error("temporarily unavailable");
    },
  });
  try {
    await h.ui.toggle();
    assert.equal(h.ui.failed.value, true);
    assert.equal(h.events.length, 1);
    await h.ui.toggle();
    h.props.loadMetadata = async () => details("/retry");
    await h.ui.toggle();
    assert.equal(h.ui.failed.value, false);
    assert.equal(h.ui.metadata.value.path, "/retry");
    assert.equal(
      h.ui.displayValue({
        label: "世界种子",
        value: "9223372036854775806",
        format: null,
      }),
      "9223372036854775806",
    );
    assert.notEqual(
      h.ui.displayValue({
        label: "最后游玩",
        value: "1730000000000",
        format: "timestamp",
      }),
      "1730000000000",
    );
  } finally {
    h.dispose();
  }
});

function nodes(vnode) {
  if (!vnode) return [];
  if (Array.isArray(vnode)) return vnode.flatMap(nodes);
  const children =
    typeof vnode.children?.default === "function"
      ? vnode.children.default()
      : vnode.children;
  return [vnode, ...(typeof children === "object" ? nodes(children) : [])];
}

test("filenames are plain text; expanding a row does not open a file manager", async () => {
  for (const name of ["Example", "example.jar"]) {
    const folders = [],
      reads = [];
    const h = item(
      {
        file: {
          name,
          file_name: "example.jar",
          version: "1",
          enabled: true,
          size: 123,
        },
        openFolder: async (...args) => folders.push(args),
        loadMetadata: async (...args) => {
          reads.push(args);
          return details("/mods/example.jar");
        },
      },
      RenderedItem,
    );
    try {
      const find = (cls) =>
        nodes(h.render()).find((n) => n.props?.class === cls);
      let stopped = false;
      const event = {
        stopPropagation() {
          stopped = true;
        },
      };
      const filename = find("content-file-name");
      assert.equal(filename.type, "span");
      assert.equal(filename.props.onClick, undefined);
      assert.deepEqual(folders, []);
      assert.deepEqual(reads, []);
      assert.equal(find("content-item-expand").props["aria-expanded"], false);
      await find("content-item-expand").props.onClick(event);
      assert.equal(find("content-item-expand").props["aria-expanded"], true);
      assert.equal(reads.length, 1);
      assert.equal(folders.length, 0);
      assert.ok(
        nodes(h.render()).some(
          (n) => n.type === "small" && n.children === "123 B",
        ),
      );
      assert.ok(
        !nodes(h.render()).some(
          (n) =>
            typeof n.children === "string" &&
            n.children.includes("/mods/example.jar"),
        ),
      );
      for (const button of nodes(h.render()).filter(
        (n) => n.type === "button",
      )) {
        assert.ok(
          !nodes(button.children).some((n) => n.type === "button"),
          "no nested buttons",
        );
      }
    } finally {
      h.dispose();
    }
  }
});

test("folder failures use the existing error channel without affecting expansion", async () => {
  const pending = deferred(),
    calls = [];
  const h = item({
    openFolder: (...args) => {
      calls.push(args);
      return pending.promise;
    },
  });
  try {
    const opening = h.ui.openFileFolder();
    await h.ui.openFileFolder();
    assert.equal(
      calls.length,
      1,
      "repeated clicks do not spawn duplicate file managers",
    );
    pending.reject(new Error("file was removed"));
    await opening;
    assert.equal(h.events[0][0], "error");
    assert.equal(h.events[0][1].message, "file was removed");
    assert.equal(h.ui.expanded.value, false);
    assert.equal(h.ui.openingFolder.value, false);
  } finally {
    h.dispose();
  }
});

const source = (options = {}) => ({
  source: "mcmod.info",
  revealable: false,
  title: "Example",
  description: null,
  fields: [],
  dependencies: [{ id: "fish", requirement: "[1,)", relation: "必需" }],
  issue: null,
  ...options,
});

test("resource pack filenames remain small plain text while titles expand metadata", async () => {
  const calls = [],
    reads = [];
  const h = item(
    {
      kind: "resourcepacks",
      file: {
        name: "Cat Pack.zip",
        file_name: "Cat Pack.zip",
        size: 512,
        enabled: true,
        version: null,
      },
      openFolder: async (...args) => calls.push(args),
      loadMetadata: async (...args) => {
        reads.push(args);
        return details("/packs/Cat Pack.zip");
      },
    },
    RenderedItem,
  );
  try {
    const all = () => nodes(h.render());
    const button = (cls) => all().find((n) => n.props?.class === cls);
    assert.ok(
      all().some((n) => n.type === "strong" && n.children === "Cat Pack"),
    );
    assert.ok(
      nodes(button("content-item-toggle")).every((n) => n.type !== "small"),
    );
    await button("content-item-toggle").props.onClick({ stopPropagation() {} });
    assert.equal(reads.length, 1);
    assert.equal(calls.length, 0);
    assert.equal(button("content-file-name").type, "span");
    assert.equal(button("content-file-name").props.onClick, undefined);
    assert.deepEqual(calls, []);
  } finally {
    h.dispose();
  }
});

test("dependency links navigate by Mod ID; archived metadata is inert and raw text loads on demand", async () => {
  const reads = [],
    located = [],
    pending = deferred();
  const h = item(
    {
      dependencyTargets: new Map([["fish", "fish-2.jar"]]),
      loadMetadata: async () => ({
        ...details("/mods/example.jar"),
        sources: [source()],
      }),
      loadSource: (...args) => {
        reads.push(args);
        return pending.promise;
      },
      openFolder: async (...args) => located.push(args),
    },
    RenderedItem,
  );
  try {
    const find = (cls) => nodes(h.render()).find((n) => n.props?.class === cls);
    await find("content-item-expand").props.onClick({ stopPropagation() {} });
    find("content-dependency-link").props.onClick();
    assert.deepEqual(h.events, [["show-mod", "a", "pack", "fish-2.jar"]]);
    const header = find("content-source-file");
    assert.equal(header.type, "span");
    header.props.onClick();
    assert.deepEqual(located, []);
    assert.deepEqual(reads, []);
    const loading = find("content-raw").props.onToggle({
      target: { open: true },
    });
    await find("content-raw").props.onToggle({ target: { open: true } });
    assert.deepEqual(reads, [
      ["a", "pack", "mods", "example.jar", "mcmod.info"],
    ]);
    pending.resolve('{"name":"<script>literal text</script>"}');
    await loading;
    await find("content-raw").props.onToggle({ target: { open: false } });
    await find("content-raw").props.onToggle({ target: { open: true } });
    assert.equal(reads.length, 1);
    const raw = nodes(h.render()).find((n) => n.type === "pre");
    assert.equal(raw.children, '{"name":"<script>literal text</script>"}');
    assert.equal(raw.props?.innerHTML, undefined);
  } finally {
    h.dispose();
  }
});

test("standalone metadata locates its own file and stale raw responses cannot cross content items", async () => {
  const locations = [];
  const h = item(
    {
      kind: "resourcepacks",
      loadMetadata: async () => ({
        ...details("/packs/Cat"),
        sources: [source({ source: "pack.mcmeta", revealable: true })],
      }),
      openFolder: async (...args) => locations.push(args),
    },
    RenderedItem,
  );
  try {
    await nodes(h.render())
      .find((n) => n.props?.class === "content-item-expand")
      .props.onClick({ stopPropagation() {} });
    const link = nodes(h.render()).find(
      (n) => n.props?.class === "content-source-file is-link",
    );
    assert.equal(link.type, "button");
    await link.props.onClick();
    assert.deepEqual(locations, [
      ["a", "pack", "resourcepacks", "example.jar", "pack.mcmeta"],
    ]);
  } finally {
    h.dispose();
  }
  for (const unmount of [false, true]) {
    const pending = deferred();
    const h = item({ loadSource: () => pending.promise });
    const loading = h.ui.toggleSource("mcmod.info", { target: { open: true } });
    if (unmount) h.dispose();
    else {
      h.props.instanceId = "b";
      await flush();
    }
    pending.reject(new Error("old raw read"));
    await loading;
    assert.deepEqual(h.events, []);
    if (!unmount) {
      assert.deepEqual(h.ui.rawSources.value, {});
      h.dispose();
    }
  }
});

test("revealing a dependency expands it, focuses and scrolls to it without locating a disk file", async () => {
  const h = item();
  let focus, scroll;
  try {
    h.ui.row.value = {
      focus: (options) => {
        focus = options;
      },
      scrollIntoView: (options) => {
        scroll = options;
      },
    };
    await h.ui.reveal();
    assert.equal(h.ui.expanded.value, true);
    assert.equal(h.ui.highlighted.value, true);
    assert.deepEqual(focus, { preventScroll: true });
    assert.deepEqual(scroll, { behavior: "smooth", block: "nearest" });
    assert.deepEqual(h.events, []);
  } finally {
    h.dispose();
  }
});

const declaredFile = (status = "pending", downloaded = 0) => ({
  has_metadata: true,
  file_name: "example.jar",
  name: "Example",
  size: 100,
  enabled: true,
  mod_ids: [],
  declaration: {
    key: "12:34",
    source: "CurseForge",
    project: 12,
    file: 34,
    path: "mods/example.jar",
    status,
    downloaded,
    total: 100,
    optional: false,
  },
});

test("pending files expand without touching disk; progress refresh preserves expansion and completion enables metadata", async () => {
  const reads = [],
    folders = [];
  const h = item({
    file: declaredFile(),
    loadMetadata: async (...args) => {
      reads.push(args);
      return details("/mods/example.jar");
    },
    openFolder: async (...args) => folders.push(args),
  });
  try {
    await h.ui.toggle();
    await h.ui.openFileFolder();
    assert.equal(h.ui.expanded.value, true);
    assert.deepEqual(reads, []);
    assert.deepEqual(folders, []);
    h.props.file = declaredFile("partial", 45);
    await flush();
    assert.equal(
      h.ui.expanded.value,
      true,
      "Same file stays expanded during progress updates",
    );
    assert.equal(h.ui.progress.value, 45);
    assert.equal(h.ui.statusLabel.value, "部分下载");
    h.props.file = declaredFile("local", 100);
    await flush();
    assert.equal(h.ui.expanded.value, true);
    assert.equal(reads.length, 1);
    assert.equal(h.ui.metadata.value.path, "/mods/example.jar");
    await h.ui.openFileFolder();
    assert.equal(folders.length, 1);
    // A polling refresh must not clear cached expanded metadata either.
    h.props.file = declaredFile("local", 100);
    await flush();
    assert.equal(reads.length, 1);
    assert.equal(h.ui.expanded.value, true);
  } finally {
    h.dispose();
  }
});

test("unresolved rows display project and file IDs; partial rows render progress without a folder link", () => {
  for (const status of ["unresolved", "partial"]) {
    const file = declaredFile(status, 45);
    if (status === "unresolved") {
      file.name = "CurseForge #12";
      file.declaration.path = null;
    }
    const h = item(
      { file, kind: status === "unresolved" ? "unresolved" : "mods" },
      RenderedItem,
    );
    try {
      const all = nodes(h.render());
      assert.ok(!all.some((n) => n.props?.class === "content-file-link"));
      assert.ok(all.some((n) => n.props?.class === "content-planned-file"));
      assert.equal(
        all.find((n) => n.props?.role === "progressbar")?.props[
          "aria-valuenow"
        ],
        status === "partial" ? 45 : undefined,
      );
    } finally {
      h.dispose();
    }
  }
});

test("mod controls preserve config drafts across closing and respect write locks", async () => {
  const { configDrafts } = await import("../src/modManagement.ts");
  configDrafts.clear();
  const calls = [];
  const doc = {
    path: "config/cat.toml",
    text: "# comment\nvalue=1",
    revision: "original",
    format: "toml",
  };
  const actions = {
    setEnabled: async (...args) => {
      calls.push(["toggle", ...args]);
      return "example.jar.disabled";
    },
    list: async () => [{ path: doc.path, size: 20 }],
    read: async () => ({ ...doc }),
    save: async (...args) => {
      calls.push(["save", ...args]);
      return { ...doc, text: args[5], revision: "saved" };
    },
  };
  let h = item({ modActions: actions });
  await h.ui.showConfig();
  h.ui.configText.value = "# comment\nvalue=2";
  h.ui.rememberDraft();
  h.dispose();
  h = item({ modActions: actions });
  try {
    await h.ui.showConfig();
    assert.equal(h.ui.configText.value, "# comment\nvalue=2");
    h.props.locked = true;
    await h.ui.saveConfig();
    await h.ui.changeEnabled();
    assert.equal(calls.length, 0);
    h.props.locked = false;
    await h.ui.saveConfig();
    assert.deepEqual(calls[0], [
      "save",
      "a",
      "pack",
      "example.jar",
      "config/cat.toml",
      "original",
      "# comment\nvalue=2",
    ]);
    assert.equal(h.ui.configDirty.value, false);
    assert.equal(configDrafts.size, 0);
    await h.ui.changeEnabled();
    assert.ok(
      h.events.some(
        ([name, file]) => name === "changed" && file === "example.jar.disabled",
      ),
    );
    const pending = deferred();
    h.props.modActions = { ...actions, read: () => pending.promise };
    const request = h.ui.chooseConfig("config/another.toml");
    h.props.instanceId = "other";
    await flush();
    pending.resolve({ ...doc, path: "config/another.toml" });
    await request;
    assert.equal(h.ui.configDocument.value, null);
  } finally {
    h.dispose();
    configDrafts.clear();
  }
});

test("failed config save retains the editable draft and original revision", async () => {
  const doc = {
    path: "config/cat.json",
    text: "{}",
    revision: "old",
    format: "json",
  };
  const h = item({
    modActions: {
      list: async () => [{ path: doc.path, size: 2 }],
      read: async () => doc,
      save: async () => {
        throw new Error("changed externally");
      },
      setEnabled: async () => "",
    },
  });
  try {
    await h.ui.showConfig();
    h.ui.configText.value = '{"value":2}';
    h.ui.rememberDraft();
    await h.ui.saveConfig();
    assert.equal(h.ui.configDirty.value, true);
    assert.equal(h.ui.configDocument.value.revision, "old");
    assert.ok(
      h.events.some(
        ([event, error]) =>
          event === "error" && error.message === "changed externally",
      ),
    );
    h.ui.configText.value = "{}";
    h.ui.rememberDraft();
  } finally {
    h.dispose();
  }
});

test("mod state icon toggles without opening details and preserves state on failure", async () => {
  const calls = [];
  const h = item(
    {
      modActions: {
        setEnabled: async (...args) => {
          calls.push(args);
          return "example.jar.disabled";
        },
      },
    },
    RenderedItem,
  );
  try {
    const state = () =>
      nodes(h.render()).find((n) =>
        n.props?.class?.includes?.("content-state-button"),
      );
    assert.equal(state().props["data-state"], "enabled");
    assert.equal(
      nodes(h.render()).some((n) => n.props?.role === "switch"),
      false,
    );
    let stopped = false;
    await state().props.onClick({
      stopPropagation() {
        stopped = true;
      },
    });
    assert.equal(stopped, true);
    assert.deepEqual(calls, [["a", "pack", "example.jar", false]]);
    assert.equal(state().props["data-state"], "disabled");
    assert.equal(
      nodes(h.render()).some((n) => n.props?.class === "content-item-expanded"),
      false,
    );
    h.props.modActions.setEnabled = async () => {
      throw new Error("busy");
    };
    await state().props.onClick({ stopPropagation() {} });
    assert.equal(state().props["data-state"], "disabled");
    assert.ok(h.events.some(([kind]) => kind === "error"));
  } finally {
    h.dispose();
  }
});

test("missing mod icon downloads only its declaration, suppresses duplicate clicks, and does not toggle", async () => {
  const pending = deferred();
  const calls = [];
  const h = item({
    file: declaredFile(),
    modActions: {
      download: (...args) => {
        calls.push(args);
        return pending.promise;
      },
      setEnabled: async () => {
        throw new Error("must not toggle an undownloaded file");
      },
    },
  });
  try {
    h.props.locked = true;
    await h.ui.activateMod();
    assert.equal(calls.length, 0);
    h.props.locked = false;
    const result = h.ui.activateMod();
    await h.ui.activateMod();
    assert.equal(h.ui.modSaving.value, true);
    assert.deepEqual(calls, [["a", "pack", "12:34"]]);
    assert.equal(h.ui.expanded.value, false);
    pending.resolve("example.jar");
    await result;
    assert.equal(h.ui.modSaving.value, false);
    assert.ok(h.events.some(([kind]) => kind === "changed"));
  } finally {
    pending.resolve();
    h.dispose();
  }
});

test("renaming a Mod to its disabled form keeps its expanded details mounted", async () => {
  const h = item();
  try {
    await h.ui.toggle();
    const details = h.ui.metadata.value;
    h.props.file = {
      ...h.props.file,
      file_name: "example.jar.disabled",
      enabled: false,
    };
    await flush();
    assert.equal(h.ui.expanded.value, true);
    assert.equal(h.ui.metadata.value, details);
    assert.equal(h.ui.modState.value, "disabled");
    h.props.file = { ...h.props.file, file_name: "other.jar" };
    await flush();
    assert.equal(h.ui.expanded.value, false);
  } finally {
    h.dispose();
  }
});

test("disabled file suffix is presented as a localized status without changing the actual file identity", async () => {
  const { setLanguage, languagePreference, locale } =
    await import("../src/i18n.ts");
  const previous = [languagePreference.value, locale.value];
  setLanguage("zh-CN", "zh-CN");
  const h = item(
    {
      file: {
        file_name: "cat.jar.disabled",
        name: "猫咪 Mod",
        enabled: false,
        size: 123,
        version: "1.0",
      },
    },
    RenderedItem,
  );
  try {
    const all = nodes(h.render());
    const filename = all.find((n) => n.props?.class === "content-file-name");
    assert.equal(filename.children.trim(), "cat.jar");
    assert.equal(filename.props.title, "cat.jar · 已禁用");
    assert.equal(
      all.find((n) => n.props?.class === "content-disabled-label").children,
      "已禁用",
    );
    assert.equal(h.props.file.file_name, "cat.jar.disabled");
    setLanguage("en-US", "zh-CN");
    const translated = nodes(h.render());
    assert.equal(
      translated.find((n) => n.props?.class === "content-file-name").props
        .title,
      "cat.jar · Disabled",
    );
    assert.equal(
      translated.find((n) => n.props?.class === "content-disabled-label")
        .children,
      "Disabled",
    );
  } finally {
    h.dispose();
    setLanguage(...previous);
  }
});

test("empty rows hide expansion and config controls without eager reads; state actions still work", async () => {
  for (const kind of [
    "mods",
    "resourcepacks",
    "datapacks",
    "shaderpacks",
    "saves",
  ]) {
    let reads = 0;
    const h = item(
      {
        kind,
        file: {
          file_name: "empty.jar",
          name: "Empty",
          enabled: true,
          size: 5,
          has_metadata: false,
          has_config: false,
        },
        loadMetadata: async () => {
          reads++;
          return { ...details("empty"), sources: [] };
        },
        modActions: {
          list: async () => {
            throw new Error("must stay lazy");
          },
          setEnabled: async () => "empty.jar.disabled",
        },
      },
      RenderedItem,
    );
    try {
      await flush();
      const all = () => nodes(h.render());
      assert.equal(
        all().some((n) => n.props?.class === "content-item-expand"),
        false,
      );
      const title = all().find((n) => n.props?.class === "content-item-toggle");
      assert.equal(title.type, "span");
      assert.equal(title.props["aria-expanded"], undefined);
      await all()
        .find((n) => n.props?.class?.includes?.("content-item-heading"))
        .props.onClick();
      await title.props.onClick({ stopPropagation() {} });
      assert.equal(
        all().some((n) => n.props?.class === "content-item-expanded"),
        false,
      );
      assert.equal(reads, 0);
      if (kind === "mods") {
        await all()
          .find((n) => n.props?.class?.includes?.("content-state-button"))
          .props.onClick({ stopPropagation() {} });
        assert.ok(
          h.events.some(
            ([event, file]) =>
              event === "changed" && file === "empty.jar.disabled",
          ),
        );
      }
    } finally {
      h.dispose();
    }
  }
});

test("configuration-only Mods keep the editor without a metadata arrow, and a missing config closes cleanly", async () => {
  const doc = {
    path: "config/example.toml",
    text: "x=1",
    revision: "a",
    format: "toml",
  };
  const h = item(
    {
      file: {
        file_name: "example.jar",
        name: "Example",
        enabled: true,
        size: 5,
        has_metadata: false,
        has_config: true,
      },
      modActions: {
        list: async () => [{ path: doc.path, size: 3 }],
        read: async () => doc,
      },
      loadMetadata: async () => {
        throw new Error("no metadata");
      },
    },
    RenderedItem,
  );
  try {
    const buttons = () =>
      nodes(h.render()).filter((n) => n.props?.class === "content-item-expand");
    assert.equal(buttons().length, 1);
    await buttons()[0].props.onClick({ stopPropagation() {} });
    assert.ok(nodes(h.render()).some((n) => n.props?.class === "mod-config"));
    await buttons()[0].props.onClick({ stopPropagation() {} });
    assert.equal(
      nodes(h.render()).some((n) => n.props?.class === "content-item-expanded"),
      false,
    );
    h.props.modActions.list = async () => [];
    await buttons()[0].props.onClick({ stopPropagation() {} });
    assert.equal(
      nodes(h.render()).some((n) => n.props?.class === "content-item-expanded"),
      false,
    );
    assert.deepEqual(h.events, [["changed", "example.jar"]]);
    h.props.file.has_config = false;
    assert.equal(buttons().length, 0);
  } finally {
    h.dispose();
  }
});

test("empty metadata results close the panel, while refresh can expose metadata again", async () => {
  const h = item({
    loadMetadata: async () => ({ ...details("empty"), sources: [] }),
  });
  try {
    await h.ui.toggle();
    await flush();
    assert.equal(h.ui.canExpand.value, false);
    assert.equal(h.ui.expanded.value, false);
    await h.ui.toggle();
    assert.equal(h.ui.expanded.value, false);
    await h.ui.reveal();
    assert.equal(h.ui.highlighted.value, true);
    assert.equal(h.ui.expanded.value, false);
    // A new file identity resets the absence result.
    h.props.file = { ...h.props.file, file_name: "with-metadata.jar" };
    h.props.loadMetadata = async () => details("with-metadata");
    await flush();
    await h.ui.toggle();
    assert.equal(h.ui.expanded.value, true);
    h.props.file.has_metadata = false;
    await flush();
    assert.equal(h.ui.expanded.value, false);
  } finally {
    h.dispose();
  }
});
