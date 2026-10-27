import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive } from "vue";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";
import { versionName } from "../src/versionDescription.ts";

const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL("../src/components/VersionManager.vue", import.meta.url);
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
const Manager = (await import(url(code))).default;
function manager(options = {}) {
  const events = [];
  const props = reactive({
    instance: {
      id: "instance-a",
      name: "World",
      versions: [
        { id: "first", installed: true },
        { id: "second", installed: true },
      ],
      selected_version: "first",
    },
    locked: false,
    loadContent: async () => ({ directory: "/mods", files: [] }),
    loadCounts: async () => ({
      mods: 0,
      datapacks: 0,
      resourcepacks: 0,
      saves: 0,
    }),
    rename: async () => {},
    remove: async () => {},
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
      ui = Manager.setup(props, {
        expose() {},
        emit: (...args) => events.push(args),
      });
      return () => null;
    },
  });
  app.mount({});
  return { ui, props, events, dispose: () => app.unmount() };
}

test("zero-count tabs hide and an emptied active tab moves to available content", async () => {
  let populated = true;
  const h = manager({
    loadCounts: async () => ({
      mods: 0,
      datapacks: 0,
      resourcepacks: 1,
      saves: 0,
      libraries: 0,
      unresolved: 0,
    }),
    loadContent: async () => ({
      directory: "/packs",
      files: populated ? [{ name: "Style", file_name: "style.zip" }] : [],
    }),
  });
  try {
    await flush();
    assert.deepEqual(
      h.ui.visibleTabs.value.map((item) => item.id),
      ["metadata", "resourcepacks"],
    );
    await h.ui.showContent("resourcepacks");
    await flush();
    assert.equal(h.ui.tab.value, "resourcepacks");
    populated = false;
    await h.ui.reload(true);
    await flush();
    assert.equal(h.ui.tab.value, "metadata");
    assert.deepEqual(
      h.ui.visibleTabs.value.map((item) => item.id),
      ["metadata"],
    );
  } finally {
    h.dispose();
  }
});

test("runtime libraries use resolved metadata, support filtering and discard stale version results", async () => {
  const old = deferred(),
    contentCalls = [];
  const h = manager({
    loadCounts: async () => ({
      mods: 0,
      datapacks: 0,
      resourcepacks: 0,
      saves: 0,
      libraries: 2,
      unresolved: 0,
    }),
    loadContent: async (...args) => {
      contentCalls.push(args);
      return { directory: "/", files: [] };
    },
    loadMetadata: async (version) =>
      version === "first"
        ? old.promise
        : {
            libraries: [
              {
                name: "org.lwjgl:lwjgl:3.3.3",
                detail: "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
              },
              { name: "com.example:codec:2.0", detail: null },
            ],
          },
  });
  try {
    await flush();
    await h.ui.showContent("libraries");
    h.ui.selected.value = "second";
    await flush();
    assert.equal(h.ui.counts.value.libraries, 2);
    h.ui.query.value = "LWJGL 3.3";
    assert.equal(h.ui.filteredLibraries.value.length, 1);
    old.resolve({ libraries: [{ name: "old", detail: null }] });
    await flush();
    assert.equal(h.ui.libraries.value.length, 2);
    assert.deepEqual(contentCalls, []);
    assert.equal(h.ui.tab.value, "libraries");
  } finally {
    h.dispose();
  }
});

test("manifest links open their own content category and empty tabs disappear", async () => {
  const calls = [];
  const h = manager({
    loadCounts: async () => ({
      mods: 2,
      datapacks: 1,
      resourcepacks: 1,
      saves: 0,
      unresolved: 435,
    }),
    loadContent: async (version, kind) => {
      calls.push([version, kind]);
      return {
        directory: "/pack",
        files: [{ file_name: kind + "-file", name: "Entry" }],
      };
    },
  });
  try {
    await flush();
    assert.ok(h.ui.visibleTabs.value.some((tab) => tab.id === "unresolved"));
    for (const kind of ["mods", "resourcepacks", "datapacks", "unresolved"]) {
      await h.ui.showContent(kind);
      await flush();
      assert.equal(h.ui.tab.value, kind);
      assert.deepEqual(calls.at(-1), ["first", kind]);
      assert.equal(h.props.instance.id, "instance-a");
    }
    h.ui.counts.value.unresolved = 0;
    assert.ok(!h.ui.visibleTabs.value.some((tab) => tab.id === "unresolved"));
    h.ui.ensureVisibleTab();
    await h.ui.showContent("mods");
    assert.ok(!h.ui.visibleTabs.value.some((tab) => tab.id === "unresolved"));
  } finally {
    h.dispose();
  }
});

test("background content refresh keeps visible rows and ignores late responses after a version switch", async () => {
  const pending = deferred();
  const h = manager({
    loadContent: async () => ({
      directory: "/mods",
      files: [{ name: "Mod", file_name: "mod.jar" }],
    }),
  });
  try {
    await h.ui.showContent("mods");
    await flush();
    const before = h.ui.content.value;
    let options;
    h.props.loadContent = async (_version, _kind, opts) => {
      options = opts;
      return pending.promise;
    };
    const refresh = h.ui.reload(true);
    assert.equal(options.fresh, true);
    assert.equal(
      h.ui.content.value,
      before,
      "Do not clear visible rows during polling",
    );
    h.props.loadContent = async () => ({
      directory: "/second/mods",
      files: [{ name: "New Mod", file_name: "new.jar" }],
    });
    h.ui.selected.value = "second";
    await flush();
    pending.resolve({ directory: "/first/mods", files: [] });
    await refresh;
    assert.equal(h.ui.content.value.directory, "/second/mods");
  } finally {
    h.dispose();
  }
});

test("switching from unresolved dependencies to an installed pack removes the empty unresolved tab", async () => {
  const h = manager({
    loadContent: async (version, kind) => ({
      directory: "/pack",
      files:
        version === "first" || kind === "mods"
          ? [{ name: "File", file_name: "file.jar" }]
          : [],
    }),
    loadCounts: async (version) => ({
      mods: version === "first" ? 0 : 177,
      resourcepacks: version === "first" ? 0 : 10,
      unresolved: version === "first" ? 435 : 0,
      datapacks: 0,
      saves: 0,
    }),
  });
  try {
    await flush();
    await h.ui.showContent("unresolved");
    h.ui.selected.value = "second";
    await flush();
    assert.equal(h.ui.tab.value, "mods");
    assert.ok(!h.ui.visibleTabs.value.some((tab) => tab.id === "unresolved"));
  } finally {
    h.dispose();
  }
});

test("only the active download is polled and stopping it reconciles the final state", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  let reads = 0;
  const h = manager({
    downloading: true,
    downloadVersion: "first",
    loadContent: async () => {
      reads++;
      return {
        directory: "/mods",
        files: [{ name: "Mod", file_name: "mod.jar" }],
      };
    },
  });
  try {
    await h.ui.showContent("mods");
    await flush();
    const initial = reads;
    context.mock.timers.tick(500);
    await flush();
    assert.equal(reads, initial + 1);
    h.props.downloading = false;
    await flush();
    assert.equal(
      reads,
      initial + 2,
      "Pause/completion triggers one final refresh",
    );
    context.mock.timers.tick(5000);
    await flush();
    assert.equal(reads, initial + 2, "No polling while paused");
    h.props.downloading = true;
    h.props.downloadVersion = "second";
    await flush();
    context.mock.timers.tick(5000);
    await flush();
    assert.equal(reads, initial + 2, "Do not poll an unrelated version");
  } finally {
    h.dispose();
    context.mock.timers.reset();
  }
});

test("late mod lists and failures never overwrite the newly selected version", async () => {
  const first = deferred(),
    second = deferred();
  const h = manager({
    loadContent: (version) =>
      version === "first" ? first.promise : second.promise,
  });
  try {
    h.ui.tab.value = "mods";
    await flush();
    h.ui.selected.value = "second";
    await flush();
    second.resolve({
      directory: "/second/mods",
      files: [
        {
          name: "Fabric API",
          file_name: "fabric-api.jar.disabled",
          version: "0.1",
          enabled: false,
        },
      ],
    });
    await flush();
    first.reject(new Error("old read failed"));
    await flush();
    assert.equal(h.ui.content.value.directory, "/second/mods");
    assert.deepEqual(h.ui.counts.value, {
      mods: 1,
      datapacks: 0,
      resourcepacks: 0,
      saves: 0,
    });
    assert.equal(h.ui.loading.value, false);
    assert.equal(
      h.events.some((e) => e[0] === "error"),
      false,
    );
    h.ui.query.value = "  FABRIC disabled ";
    assert.equal(h.ui.filteredFiles.value.length, 1);
    h.ui.query.value = "nonexistent";
    assert.equal(h.ui.filteredFiles.value.length, 0);
  } finally {
    h.dispose();
  }
});

test("instance changes retain previous counts until the new scan completes", async () => {
  const pending = deferred();
  const h = manager({
    loadCounts: async () => ({
      mods: 199,
      datapacks: 0,
      resourcepacks: 10,
      saves: 0,
    }),
  });
  try {
    await flush();
    h.props.loadCounts = () => pending.promise;
    h.props.instance = {
      id: "instance-b",
      name: "Next",
      versions: [{ id: "next" }],
      selected_version: "next",
    };
    await flush();
    assert.deepEqual(h.ui.counts.value, {
      mods: 199,
      datapacks: 0,
      resourcepacks: 10,
      saves: 0,
    });
    pending.resolve({ mods: 5, datapacks: 1, resourcepacks: 2, saves: 3 });
    await flush();
    assert.deepEqual(h.ui.counts.value, {
      mods: 5,
      datapacks: 1,
      resourcepacks: 2,
      saves: 3,
    });
  } finally {
    h.dispose();
  }
});

test("progressive lists render while loading and stale callbacks cannot cross instance switches", async () => {
  const first = deferred(),
    second = deferred();
  const options = [];
  const h = manager({
    loadContent: (version, kind, request) => {
      options.push(request);
      return options.length === 1 ? first.promise : second.promise;
    },
  });
  try {
    h.ui.tab.value = "mods";
    await flush();
    options[0].onProgress({
      directory: "/first/mods",
      files: [{ file_name: "one.jar", name: "One" }],
    });
    assert.equal(h.ui.loading.value, true);
    assert.equal(h.ui.filteredFiles.value[0].name, "One");
    h.ui.selected.value = "second";
    await flush();
    assert.equal(options[0].signal.aborted, true);
    options[1].onProgress({
      directory: "/second/mods",
      files: [{ file_name: "two.jar", name: "Two" }],
    });
    options[0].onProgress({ directory: "/wrong", files: [] });
    first.resolve({ directory: "/wrong", files: [] });
    await flush();
    assert.equal(h.ui.content.value.directory, "/second/mods");
    second.resolve({
      directory: "/second/mods",
      files: [{ file_name: "two.jar", name: "Two" }],
    });
    await flush();
    assert.equal(h.ui.loading.value, false);
  } finally {
    h.dispose();
  }
  assert.equal(options[1].signal.aborted, true);
});

test("renaming retains drafts on failure; deletion is guarded and updates the remaining selection", async () => {
  const calls = [];
  const h = manager({
    rename: async (...args) => {
      calls.push(args);
      throw new Error("disk full");
    },
  });
  try {
    await flush();
    h.ui.name.value = "  My pack  ";
    await h.ui.change();
    assert.deepEqual(calls, [["first", "My pack"]]);
    assert.equal(h.ui.name.value, "  My pack  ");
    assert.equal(h.ui.saving.value, false);
    assert.equal(h.events.at(-1)[0], "busy");
    h.props.rename = async (id, name) => {
      h.props.instance.versions.find((v) => v.id === id).name = name;
    };
    await h.ui.change();
    await flush();
    assert.equal(versionName(h.props.instance.versions[0]), "My pack");
    h.props.locked = true;
    h.props.remove = async (id) => {
      h.props.instance.versions = h.props.instance.versions.filter(
        (v) => v.id !== id,
      );
    };
    await h.ui.change(true);
    assert.equal(h.props.instance.versions.length, 2);
    h.props.locked = false;
    await h.ui.change(true);
    await flush();
    assert.equal(h.ui.selected.value, "second");
    await h.ui.change(true);
    await flush();
    assert.equal(h.ui.selected.value, "");
    assert.equal(h.ui.content.value, null);
    assert.deepEqual(h.ui.counts.value, {
      mods: 0,
      datapacks: 0,
      resourcepacks: 0,
      saves: 0,
      unresolved: 0,
      libraries: 0,
    });
  } finally {
    h.dispose();
  }
});

test("manager IPC addresses the displayed directory and keeps the dialog open after edits", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const instance = fixture.state.database.instances[0];
    fixture.handlers.version_content_counts = ({ id, version }) => {
      assert.equal(id, instance.id);
      assert.equal(version, "1.21.1");
      return { mods: 2, datapacks: 3, resourcepacks: 4, saves: 5 };
    };
    fixture.handlers.version_content = ({ id, version, kind }) => {
      assert.equal(kind, "saves");
      assert.equal(id, instance.id);
      assert.equal(version, "1.21.1");
      return { directory: "/linked/saves", files: [] };
    };
    fixture.handlers.rename_version = ({ version, name }) => {
      instance.versions.find((v) => v.id === version).name = name;
      return true;
    };
    fixture.handlers.delete_version = ({ version }) => {
      instance.versions = instance.versions.filter((v) => v.id !== version);
      instance.selected_version = null;
      return instance;
    };
    ui.manageVersions(instance);
    assert.equal(ui.modal.value, "manage");
    assert.equal((await ui.readVersionContentCounts("1.21.1")).datapacks, 3);
    assert.equal(
      (await ui.readVersionContent("1.21.1", "saves")).directory,
      "/linked/saves",
    );
    await ui.renameVersion("1.21.1", "My Fabric");
    assert.equal(
      versionName(ui.managedInstance.value.versions[0]),
      "My Fabric",
    );
    assert.equal(ui.modal.value, "manage");
    await ui.deleteVersion("1.21.1");
    assert.equal(ui.managedInstance.value.versions.length, 0);
    assert.equal(ui.modal.value, "manage");
    assert.equal(
      fixture.calls.some((c) => ["install", "launch"].includes(c.op)),
      false,
    );
  } finally {
    dispose();
  }
});

test("switching content tabs drops stale results and resets the filter without resetting version edits", async () => {
  const mods = deferred(),
    packs = deferred(),
    worlds = deferred();
  const calls = [];
  const h = manager({
    loadContent: (version, kind) => {
      calls.push([version, kind]);
      return { mods, datapacks: packs, saves: worlds }[kind].promise;
    },
  });
  try {
    h.ui.tab.value = "mods";
    await flush();
    await h.ui.startRename();
    h.ui.name.value = "Draft name";
    h.ui.query.value = "stale search";
    h.ui.tab.value = "datapacks";
    await flush();
    assert.equal(h.ui.query.value, "");
    h.ui.tab.value = "saves";
    await flush();
    worlds.resolve({
      directory: "/isolated/saves",
      files: [{ name: "World", file_name: "World", enabled: true }],
    });
    await flush();
    packs.reject(new Error("stale pack error"));
    mods.resolve({ directory: "/mods", files: [] });
    await flush();
    assert.deepEqual(calls, [
      ["first", "mods"],
      ["first", "datapacks"],
      ["first", "saves"],
    ]);
    assert.equal(h.ui.content.value.directory, "/isolated/saves");
    assert.deepEqual(h.ui.counts.value, {
      mods: 0,
      datapacks: 0,
      resourcepacks: 0,
      saves: 1,
    });
    assert.equal(h.ui.filteredFiles.value[0].name, "World");
    assert.equal(h.ui.name.value, "Draft name");
    assert.equal(h.ui.editingName.value, true);
    assert.equal(
      h.events.some((event) => event[0] === "error"),
      false,
    );
    h.ui.cancelRename();
    assert.equal(h.ui.name.value, "first");
    assert.equal(h.ui.editingName.value, false);
  } finally {
    h.dispose();
  }
});

test("all tab counts load before opening those tabs, refresh together, and ignore stale version counts", async () => {
  const first = deferred(),
    second = deferred();
  const calls = [];
  const h = manager({
    loadCounts: (version) => {
      calls.push(version);
      return version === "first" ? first.promise : second.promise;
    },
  });
  try {
    assert.deepEqual(calls, ["first"]);
    assert.equal(h.ui.tab.value, "metadata");
    h.ui.selected.value = "second";
    await flush();
    second.resolve({ mods: 0, datapacks: 4, resourcepacks: 6, saves: 2 });
    await flush();
    first.resolve({
      mods: 999,
      datapacks: 999,
      resourcepacks: 999,
      saves: 999,
    });
    await flush();
    assert.deepEqual(h.ui.counts.value, {
      mods: 0,
      datapacks: 4,
      resourcepacks: 6,
      saves: 2,
    });
    assert.equal(h.ui.tab.value, "metadata"); // No other tab has been opened.
    h.ui.tab.value = "saves";
    await flush();
    assert.deepEqual(calls, ["first", "second"]);
    h.props.loadCounts = async (version) => {
      calls.push(version);
      return { mods: 1, datapacks: 5, resourcepacks: 7, saves: 0 };
    };
    h.ui.refresh();
    await flush();
    assert.deepEqual(h.ui.counts.value, {
      mods: 1,
      datapacks: 5,
      resourcepacks: 7,
      saves: 0,
    });
    assert.deepEqual(calls, ["first", "second", "second"]);
  } finally {
    h.dispose();
  }
});

test("metadata is a separate lazy tab and never becomes a content-kind request", async () => {
  const calls = [];
  const pending = deferred();
  const h = manager({
    loadContent: (version, kind) => {
      calls.push([version, kind]);
      return pending.promise;
    },
  });
  try {
    assert.equal(h.ui.tabs[0].id, "metadata");
    assert.equal(h.ui.tab.value, "metadata");
    assert.deepEqual(calls, []);
    h.ui.tab.value = "mods";
    await flush();
    h.ui.tab.value = "metadata";
    await flush();
    pending.resolve({ directory: "/stale", files: [{ name: "old" }] });
    await flush();
    assert.equal(h.ui.content.value, null);
    assert.equal(h.ui.loading.value, false);
    h.ui.refresh();
    await flush();
    assert.deepEqual(calls, [["first", "mods"]]);
    assert.equal(h.ui.metadataRefresh.value, 1);
    assert.deepEqual(Object.keys(h.ui.counts.value).sort(), [
      "datapacks",
      "mods",
      "resourcepacks",
      "saves",
    ]);
  } finally {
    h.dispose();
  }
});

test("changing instances reloads identical version IDs, retains the tab and ignores old results", async () => {
  const old = deferred();
  let reads = 0;
  const h = manager({
    loadContent: async () => {
      if (++reads === 1) return old.promise;
      return {
        directory: "/second/saves",
        files: [{ name: "World", file_name: "World" }],
      };
    },
  });
  try {
    h.ui.tab.value = "saves";
    await flush();
    h.ui.confirming.value = true;
    h.ui.chooseInstance("instance-b");
    assert.deepEqual(h.events.at(-1), ["select-instance", "instance-b"]);
    h.ui.saving.value = true;
    const count = h.events.length;
    h.ui.chooseInstance("instance-c");
    assert.equal(h.events.length, count);
    h.ui.saving.value = false;
    h.props.instance = {
      ...h.props.instance,
      id: "instance-b",
      name: "Second",
    };
    await flush();
    old.reject(new Error("old instance is gone"));
    await flush();
    assert.equal(h.ui.tab.value, "saves");
    assert.equal(h.ui.selected.value, "first");
    assert.equal(h.ui.content.value.directory, "/second/saves");
    assert.equal(h.ui.confirming.value, false);
    assert.equal(
      h.events.some((e) => e[0] === "error"),
      false,
    );
  } finally {
    h.dispose();
  }
});

test("management target switches without changing the main launch selection and scopes file requests", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const first = fixture.state.database.instances[0];
    const second = { ...first, id: "instance-b", name: "Other directory" };
    fixture.state.database.instances.push(second);
    await ui.sync();
    ui.manageVersions(first);
    const main = ui.selectedInstance.value?.id;
    ui.switchManagedInstance(second.id);
    assert.equal(ui.managedInstance.value.id, second.id);
    assert.equal(ui.selectedInstance.value?.id, main);
    for (const op of ["version_metadata"])
      fixture.handlers[op] = (request) => {
        assert.equal(request.id, second.id);
        return {};
      };
    await ui.readVersionMetadata("1.21.1");
    ui.managing.value = true;
    ui.switchManagedInstance(first.id);
    assert.equal(ui.managedInstance.value.id, second.id);
    ui.managing.value = false;
    ui.switchManagedInstance("missing");
    assert.equal(ui.managedInstance.value.id, second.id);
  } finally {
    dispose();
  }
});

test("metadata Mod links open the selected version's Mods tab without changing the instance", async () => {
  const calls = [];
  let focused = false;
  const h = manager({
    loadContent: async (version, kind) => {
      calls.push([version, kind]);
      return {
        directory: `/instance-a/${version}/mods`,
        files: [{ name: "Mod", file_name: "mod.jar" }],
      };
    },
  });
  try {
    h.ui.selected.value = "second";
    h.ui.tabBar.value = {
      querySelector: (selector) => {
        assert.equal(selector, "#version-content-tab-mods");
        return {
          focus: () => {
            focused = true;
          },
        };
      },
    };
    await flush();
    assert.deepEqual(calls, []);
    await h.ui.showContent("mods");
    await flush();
    assert.equal(h.ui.tab.value, "mods");
    assert.equal(h.ui.selected.value, "second");
    assert.equal(h.props.instance.id, "instance-a");
    assert.equal(h.ui.content.value.directory, "/instance-a/second/mods");
    assert.deepEqual(calls, [["second", "mods"]]);
    assert.equal(focused, true);
    assert.deepEqual(h.events, []);
  } finally {
    h.dispose();
  }
});

test("content detail IPC uses the row's captured instance, version, type and relative path", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.version_content_metadata = (request) => {
      assert.equal(request.id, "row-instance");
      assert.equal(request.version, "row-version");
      assert.equal(request.kind, "datapacks");
      assert.equal(request.file, "saves/World/datapacks/Pack");
      return {
        path: "/row-instance/saves/World/datapacks/Pack",
        size: null,
        sources: [],
      };
    };
    ui.editing.value = { id: "different-current-instance" };
    const result = await ui.readContentMetadata(
      "row-instance",
      "row-version",
      "datapacks",
      "saves/World/datapacks/Pack",
    );
    assert.equal(result.path, "/row-instance/saves/World/datapacks/Pack");
  } finally {
    dispose();
  }
});

test("opening a content folder preserves the row context when another instance is selected", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    let opened = false;
    fixture.handlers.open_content_folder = (request) => {
      assert.equal(request.id, "row-instance");
      assert.equal(request.version, "row-version");
      assert.equal(request.kind, "mods");
      assert.equal(request.file, "1.12.2/cat.jar");
      opened = true;
      return true;
    };
    ui.editing.value = { id: "different-current-instance" };
    await ui.openContentFolder(
      "row-instance",
      "row-version",
      "mods",
      "1.12.2/cat.jar",
    );
    assert.equal(opened, true);
  } finally {
    dispose();
  }
});

test("the footer opens the selected content directory without a filename and reports opener errors", async () => {
  const calls = [];
  const h = manager({
    loadCounts: async () => ({
      mods: 1,
      datapacks: 0,
      resourcepacks: 0,
      saves: 0,
    }),
    loadContent: async () => ({
      directory: "/Games/versions/first/mods",
      files: [{ name: "Cat", file_name: "cat.jar" }],
    }),
    openContentFolder: async (...args) => {
      calls.push(args);
    },
  });
  try {
    await flush();
    await h.ui.showContent("mods");
    await flush();
    await h.ui.openDirectory();
    assert.deepEqual(calls, [["instance-a", "first", "mods"]]);
    h.props.openContentFolder = async () => {
      throw new Error("missing directory");
    };
    await h.ui.openDirectory();
    assert.equal(h.events.at(-1)[0], "error");
    assert.equal(h.events.at(-1)[1].message, "missing directory");
  } finally {
    h.dispose();
  }
});

test("Mod ID navigation clears filters, prefers enabled providers and stays within the selected instance", async () => {
  const files = [
    { name: "Cat", file_name: "cat.jar", mod_ids: ["cat"], enabled: true },
    {
      name: "Fish old",
      file_name: "fish.jar.disabled",
      mod_ids: ["fish"],
      enabled: false,
    },
    {
      name: "Fish",
      file_name: "fish.jar",
      mod_ids: ["fish", "fins"],
      enabled: true,
    },
  ];
  const h = manager({
    loadContent: async () => ({ directory: "/mods", files }),
  });
  let reveals = 0;
  try {
    h.ui.tab.value = "mods";
    await flush();
    assert.equal(h.ui.dependencyTargets.value.get("fish"), "fish.jar");
    assert.equal(h.ui.dependencyTargets.value.get("fins"), "fish.jar");
    assert.equal(h.ui.dependencyTargets.value.has("missing"), false);
    h.ui.query.value = "cat";
    h.ui.setContentRow("fish.jar", { reveal: async () => reveals++ });
    await h.ui.showMod("instance-a", "first", "fish.jar");
    assert.equal(h.ui.query.value, "");
    assert.equal(reveals, 1);
    h.ui.query.value = "fins";
    assert.deepEqual(
      h.ui.filteredFiles.value.map((f) => f.file_name),
      ["fish.jar"],
    );
    await h.ui.showMod("old-instance", "first", "fish.jar");
    assert.equal(h.ui.query.value, "fins");
    const stale = h.ui.showMod("instance-a", "first", "fish.jar");
    h.ui.selected.value = "second";
    await stale;
    assert.equal(reveals, 1);
  } finally {
    h.dispose();
  }
});
