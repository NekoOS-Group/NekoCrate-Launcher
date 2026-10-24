import assert from "node:assert/strict";
import test from "node:test";
import { ref, watch } from "vue";
import { useFolderDrop } from "../src/useFolderDrop.ts";
import {
  settingsFixture,
  flush,
  deferred,
} from "./helpers/appSettingsFixture.mjs";

const candidate = (path, extra = {}) => ({
  path,
  name: path.split("/").at(-2),
  versions: [
    { id: "1.20.1", installed: true },
    { id: "Forge", installed: true },
  ],
  existing_id: null,
  blocked: null,
  ...extra,
});
const report = (...candidates) => ({ candidates, skipped: 0, issues: [] });
const instance = (c, id = "new") => ({
  id,
  name: c.name,
  game_directory: c.path,
  versions: c.versions,
});
function fixture() {
  const calls = [],
    errors = [],
    selected = [],
    handlers = {};
  const busy = ref(false);
  let syncs = 0;
  const ui = useFolderDrop({
    rpc: async (op, data) => {
      calls.push({ op, ...data });
      return handlers[op]?.(data);
    },
    sync: async () => {
      syncs++;
    },
    select: (id) => selected.push(id),
    busy: () => busy.value,
    error: (text) => errors.push(text),
  });
  return {
    ui,
    calls,
    errors,
    selected,
    handlers,
    busy,
    get syncs() {
      return syncs;
    },
  };
}

test("one dropped directory links its versions as one instance and reports its name and path", async () => {
  const f = fixture();
  const c = candidate("/Games/RLCraft/.minecraft");
  f.handlers.scan_folders = () => report(c);
  f.handlers.import_folders = ({ paths }) => {
    assert.deepEqual(paths, [c.path]);
    return { added: [instance(c)], existing: [], failures: [] };
  };
  f.ui.handle({ type: "enter", paths: ["/Games"] });
  assert.equal(f.ui.state.dragging, true);
  f.ui.handle({ type: "drop", paths: ["/Games"] });
  await flush();
  assert.equal(f.ui.state.dragging, false);
  assert.equal(f.ui.state.phase, null);
  assert.deepEqual(f.selected, ["new"]);
  assert.deepEqual(f.ui.state.notice, [{ name: "RLCraft", path: c.path }]);
  assert.equal(
    f.calls.some((c) => c.op === "delete"),
    false,
  );
  assert.equal(f.syncs, 1);
  f.ui.dispose();
});

test("multiple folders preview before linking and exclude existing or conflicting directories", async () => {
  const f = fixture();
  const a = candidate("/Games/A/.minecraft"),
    b = candidate("/Games/B/.minecraft");
  f.handlers.scan_folders = () =>
    report(
      a,
      b,
      candidate("/Games/C/.minecraft", { existing_id: "old" }),
      candidate("/Games/D/.minecraft", { blocked: "overlap" }),
    );
  await f.ui.scan(["/Games"]);
  assert.equal(f.ui.state.phase, "preview");
  assert.equal(f.calls.length, 1);
  assert.deepEqual(
    f.ui.selected.value.map((c) => c.name),
    ["A", "B"],
  );
  f.ui.toggle(b.path);
  f.handlers.import_folders = () => ({
    added: [instance(a)],
    existing: [],
    failures: [],
  });
  await f.ui.importSelected();
  assert.deepEqual(f.calls.at(-1).paths, [a.path]);
  f.ui.dispose();
});

test("an existing directory selects the existing instance without importing again", async () => {
  const f = fixture();
  f.handlers.scan_folders = () =>
    report(candidate("/Games/A/.minecraft", { existing_id: "old" }));
  await f.ui.scan(["/Games"]);
  assert.deepEqual(f.selected, ["old"]);
  assert.equal(
    f.calls.some((c) => c.op === "import_folders"),
    false,
  );
  assert.deepEqual(f.ui.state.notice, [
    { name: "A", path: "/Games/A/.minecraft" },
  ]);
  f.ui.dispose();
});

test("late scans cannot replace new previews; cancellation is separate from downloads", async () => {
  const f = fixture(),
    first = deferred();
  f.handlers.scan_folders = () => first.promise;
  const old = f.ui.scan(["/Old"]);
  f.handlers.scan_folders = () =>
    report(candidate("/New/A/.minecraft"), candidate("/New/B/.minecraft"));
  await f.ui.scan(["/New"]);
  first.resolve(report(candidate("/Old/.minecraft")));
  await old;
  assert.equal(f.ui.state.scan.candidates[0].name, "A");
  assert.equal(f.calls.filter((c) => c.op === "cancel_folder_scan").length, 1);
  assert.equal(
    f.calls.some((c) => c.op === "cancel" || c.op === "import_folders"),
    false,
  );
  const last = deferred();
  f.handlers.scan_folders = () => last.promise;
  const pending = f.ui.scan(["/Cancelled"]);
  f.ui.close();
  last.resolve(report(candidate("/Cancelled/.minecraft")));
  await pending;
  assert.equal(f.ui.state.phase, null);
  assert.equal(
    f.calls.some((c) => c.op === "import_folders"),
    false,
  );
  f.ui.dispose();
});

test("busy downloads allow discovery but defer linking; partial failure keeps only failed paths selected", async () => {
  const f = fixture();
  const a = candidate("/Games/A/.minecraft"),
    b = candidate("/Games/B/.minecraft");
  f.busy.value = true;
  f.handlers.scan_folders = () => report(a);
  await f.ui.scan(["/Games/A"]);
  assert.equal(f.ui.state.phase, null);
  assert.equal(f.ui.previewVisible.value, false);
  await f.ui.importSelected();
  assert.equal(f.calls.length, 1);
  f.busy.value = false;
  f.handlers.scan_folders = () => report(a, b);
  await f.ui.scan(["/Games"]);
  f.handlers.import_folders = () => ({
    added: [instance(a)],
    existing: [],
    failures: [{ path: b.path, message: "gone" }],
  });
  await f.ui.importSelected();
  assert.equal(f.ui.state.phase, "preview");
  assert.deepEqual(
    f.ui.selected.value.map((c) => c.path),
    [b.path],
  );
  assert.deepEqual(f.ui.state.notice, [{ name: "A", path: a.path }]);
  assert.ok(f.errors.at(-1).includes("gone"));
  f.ui.dispose();
});

test("disposal cancels scans and ignores late replies", async () => {
  const f = fixture(),
    pending = deferred();
  f.handlers.scan_folders = () => pending.promise;
  const scanning = f.ui.scan(["/Games"]);
  f.ui.dispose();
  pending.resolve(report(candidate("/Games/A/.minecraft")));
  await scanning;
  assert.equal(f.calls.at(-1).op, "cancel_folder_scan");
  assert.deepEqual(f.selected, []);
  assert.equal(
    f.calls.some((c) => c.op === "import_folders"),
    false,
  );
});

test("App connects native folder drops and removes the listener on unmount", async () => {
  const { fixture: f, context: ui, dispose } = await settingsFixture();
  try {
    const c = candidate("/Games/Pack/.minecraft");
    f.handlers.scan_folders = () => report(c);
    f.handlers.import_folders = () => {
      const added = instance(c, "linked");
      f.state.database.instances.push(added);
      return { added: [added], existing: [], failures: [] };
    };
    f.fileDrop({ payload: { type: "drop", paths: ["/Games"] } });
    await flush();
    assert.equal(ui.selectedInstanceId.value, "linked");
    assert.equal(ui.folderDrop.state.phase, null);
  } finally {
    dispose();
  }
  assert.equal(f.dropUnlistened, true);
});

test("linked info disappears after two seconds and a newer info resets its lifetime", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const f = fixture();
  f.ui.showLinked([{ name: "A", path: "/A/.minecraft" }]);
  t.mock.timers.tick(1999);
  assert.ok(f.ui.state.notice);
  f.ui.showLinked([{ name: "B", path: "/B/.minecraft" }]);
  t.mock.timers.tick(1);
  assert.equal(f.ui.state.notice[0].name, "B");
  t.mock.timers.tick(1999);
  assert.equal(f.ui.state.notice, null);
  f.ui.dispose();
});

test("empty discovery uses a two-second corner info with no central preview", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const f = fixture(),
    waiting = deferred();
  f.handlers.scan_folders = () => waiting.promise;
  const scanning = f.ui.scan(["/Empty"]);
  assert.equal(f.ui.previewVisible.value, false);
  assert.equal(f.ui.cornerBusy.value, true);
  waiting.resolve(report());
  await scanning;
  assert.equal(f.ui.state.phase, null);
  assert.equal(f.ui.previewVisible.value, false);
  assert.deepEqual(f.ui.state.notice, []);
  t.mock.timers.tick(2000);
  assert.equal(f.ui.state.notice, null);
  f.ui.dispose();
});

test("a single candidate never opens a preview while scanning, linking or refreshing", async () => {
  const f = fixture(),
    scanning = deferred(),
    linking = deferred();
  const c = candidate("/Games/One/.minecraft");
  const visible = [];
  const stop = watch(f.ui.previewVisible, (value) => visible.push(value), {
    flush: "sync",
    immediate: true,
  });
  f.handlers.scan_folders = () => scanning.promise;
  f.handlers.import_folders = () => linking.promise;
  const done = f.ui.scan(["/Games"]);
  assert.equal(f.ui.previewVisible.value, false);
  scanning.resolve({
    ...report(c),
    skipped: 1,
    issues: [{ path: "/Games/unreadable", message: "denied" }],
  });
  await flush();
  assert.equal(f.ui.state.phase, "importing");
  assert.equal(f.ui.previewVisible.value, false);
  linking.resolve({ added: [instance(c)], existing: [], failures: [] });
  await done;
  assert.equal(visible.includes(true), false);
  assert.deepEqual(f.selected, ["new"]);
  stop();
  f.ui.dispose();
});
