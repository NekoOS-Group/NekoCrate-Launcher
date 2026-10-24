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

test("late mod lists and failures never overwrite the newly selected version", async () => {
  const first = deferred(),
    second = deferred();
  const h = manager({
    loadContent: (version) =>
      version === "first" ? first.promise : second.promise,
  });
  try {
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
    assert.equal(h.ui.tab.value, "mods");
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
    assert.equal(h.ui.tab.value, "mods"); // No other tab has been opened.
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
