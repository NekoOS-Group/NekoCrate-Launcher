import assert from "node:assert/strict";
import test from "node:test";
import { createVersionContentLoader } from "../src/versionContentLoader.ts";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
const flush = () => new Promise(setImmediate);
const file = (file_name, name = file_name) => ({
  file_name,
  name,
  version: null,
  enabled: true,
  size: 5,
  mod_ids: [],
});

test("background refresh does not replay an older cached download status", async () => {
  const pending = deferred();
  let calls = 0;
  const row = (status) => ({ ...file("a.jar"), declaration: { status } });
  const load = createVersionContentLoader(async () =>
    ++calls === 1
      ? { directory: "/mods", files: [row("pending")], pending: [] }
      : pending.promise,
  );
  await load("a", "v", "mods");
  const shown = [];
  const refreshing = load("a", "v", "mods", {
    fresh: true,
    onProgress: (value) => shown.push(value),
  });
  assert.deepEqual(
    shown,
    [],
    "Keep the currently displayed progress until fresh data arrives",
  );
  pending.resolve({ directory: "/mods", files: [row("local")], pending: [] });
  const result = await refreshing;
  assert.equal(shown.length, 1);
  assert.equal(result.files[0].declaration.status, "local");
});

test("file list is available before any archive batch, then identities arrive progressively", async () => {
  const first = deferred(),
    second = deferred();
  const files = Array.from({ length: 20 }, (_, i) =>
    file(`${String(i).padStart(2, "0")}.jar`),
  );
  const batches = [],
    shown = [];
  const load = createVersionContentLoader(async (op, request) => {
    if (op === "version_content_index")
      return {
        directory: "/mods",
        files,
        pending: files.map((f) => f.file_name),
      };
    assert.equal(op, "version_content_summaries");
    batches.push(request.files);
    return batches.length === 1 ? first.promise : second.promise;
  });
  const ready = load("a", "v", "mods", {
    onProgress: (value) => shown.push(value),
  });
  await flush();
  assert.equal(shown[0].files.length, 20);
  assert.deepEqual(
    batches[0],
    files.slice(0, 8).map((f) => f.file_name),
  );
  first.resolve(
    files.slice(0, 8).map((f) => ({
      ...f,
      name: `Mod ${f.file_name}`,
      mod_ids: [f.file_name],
    })),
  );
  await flush();
  assert.equal(shown[1].files[0].name, "Mod 00.jar");
  assert.equal(shown[1].files[8].name, "08.jar");
  assert.equal(batches[1].length, 12);
  second.resolve(
    files.slice(8).map((f) => ({ ...f, name: `Mod ${f.file_name}` })),
  );
  const result = await ready;
  assert.equal(result.files.length, 20);
  assert.ok(result.files.every((f) => f.name.startsWith("Mod ")));
});

test("cached view is immediate, while a fresh index reconciles additions, replacements and removals", async () => {
  let next = {
    directory: "/mods",
    files: [file("a.jar", "Old"), file("removed.jar")],
    pending: [],
  };
  const load = createVersionContentLoader(async (op) => {
    if (op === "version_content_index") return await next;
    return [file("a.jar", "Updated"), file("new.jar", "Added")];
  });
  await load("a", "v", "mods");
  const fresh = deferred();
  next = fresh.promise;
  const shown = [];
  const ready = load("a", "v", "mods", {
    onProgress: (value) => shown.push(value),
  });
  assert.deepEqual(
    shown[0].files.map((f) => f.name),
    ["Old", "removed.jar"],
  );
  fresh.resolve({
    directory: "/mods",
    files: [file("a.jar"), file("new.jar")],
    pending: ["a.jar", "new.jar"],
  });
  const result = await ready;
  assert.deepEqual(
    result.files.map((f) => f.name),
    ["Updated", "Added"],
    "enrichment keeps visible rows anchored",
  );
  assert.ok(!result.files.some((f) => f.file_name === "removed.jar"));
  next = { directory: "/another", files: [], pending: [] };
  let early = false;
  const other = load("b", "v", "mods", {
    onProgress: () => {
      early = true;
    },
  });
  assert.equal(early, false, "another instance cannot see the cached list");
  await other;
});

test("leaving a page stops further batches and discards a late batch instead of caching it", async () => {
  const pending = deferred(),
    controller = new AbortController();
  const files = Array.from({ length: 40 }, (_, i) => file(`${i}.jar`));
  let batches = 0;
  const load = createVersionContentLoader(async (op) => {
    if (op === "version_content_index")
      return {
        directory: "/mods",
        files,
        pending: files.map((f) => f.file_name),
      };
    batches++;
    return pending.promise;
  });
  const shown = [];
  const ready = load("a", "v", "mods", {
    signal: controller.signal,
    onProgress: (value) => shown.push(value),
  });
  const rejected = assert.rejects(ready, { name: "AbortError" });
  await flush();
  controller.abort();
  pending.resolve(files.slice(0, 8));
  await rejected;
  assert.equal(batches, 1);
  assert.equal(shown.length, 1);
  const nextController = new AbortController();
  let early = false;
  const next = load("a", "v", "mods", {
    signal: nextController.signal,
    onProgress: () => {
      early = true;
    },
  });
  assert.equal(early, false);
  nextController.abort();
  await assert.rejects(next, { name: "AbortError" });
});

test("files deleted during a batch disappear and cache storage is bounded", async () => {
  const load = createVersionContentLoader(async (op, { id }) =>
    op === "version_content_index"
      ? {
          directory: `/${id}`,
          files: [file("gone.jar")],
          pending: ["gone.jar"],
        }
      : [],
  );
  assert.deepEqual((await load("oldest", "v", "mods")).files, []);
  for (let i = 0; i < 12; i++) await load(String(i), "v", "mods");
  let early = false;
  const old = load("oldest", "v", "mods", {
    onProgress: () => {
      early = true;
    },
  });
  assert.equal(early, false, "oldest view is evicted after 12 newer lists");
  await old;
});

test("preparing a tab reads only the index and supplies rows synchronously when clicked", async () => {
  const calls = [],
    fresh = deferred();
  const index = {
    directory: "/mods",
    files: [file("first.jar")],
    pending: ["first.jar"],
  };
  let indexes = 0;
  const load = createVersionContentLoader(async (op) => {
    calls.push(op);
    if (op === "version_content_index")
      return ++indexes === 1 ? index : fresh.promise;
    return [file("first.jar", "First mod")];
  });
  await load.preload("a", "v");
  assert.deepEqual(
    calls,
    ["version_content_index"],
    "preparing a hidden tab must not read archives",
  );
  const shown = [];
  const ready = load("a", "v", "mods", {
    onProgress: (result) => shown.push(result),
  });
  assert.equal(
    shown[0].files[0].file_name,
    "first.jar",
    "cached rows are available in the click's render turn",
  );
  fresh.resolve(index);
  assert.equal((await ready).files[0].name, "First mod");
});

test("clicking during prefetch shares the pending index instead of scanning twice", async () => {
  const pending = deferred();
  let scans = 0;
  const load = createVersionContentLoader(async () => {
    scans++;
    return pending.promise;
  });
  const prepare = load.preload("a", "v");
  const ready = load("a", "v", "mods");
  assert.equal(scans, 1);
  pending.resolve({ directory: "/mods", files: [], pending: [] });
  await Promise.all([prepare, ready]);
});
