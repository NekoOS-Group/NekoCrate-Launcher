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
      `from ${JSON.stringify(name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
  );
  return (await import(moduleUrl(code))).default;
}
const Item = await compileItem();
const RenderedItem = await compileItem(true);
const details = (path) => ({ path, size: 123, sources: [] });
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
