import assert from "node:assert/strict";
import test from "node:test";
import {
  dashboard,
  readDashboard,
  saveDashboard,
  addFavorite,
  normalizeServerAddress,
  loadLocalWidgets,
  localWidgetData,
  loadServerStatus,
  serverStatuses,
  pixelTimePath,
} from "../src/homeDashboard.ts";
import {
  readClockLayout,
  readHomeWidgets,
  homeWidgets,
} from "../src/homeWidgets.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("address normalization, old preferences, deduplication and bounded notes", () => {
  for (const [address, expected] of [
    ["localhost:80", "localhost:80"],
    ["EXAMPLE.org:25565", "example.org"],
    ["minecraft://localhost", "localhost"],
    ["::1", "[::1]"],
    ["[::1]:80", "[::1]:80"],
  ])
    assert.equal(normalizeServerAddress(address), expected);
  for (const address of [
    "",
    "a/b",
    "https://a",
    "a@b",
    "a:",
    "a:0",
    "a:65536",
    "a\nb",
  ])
    assert.equal(normalizeServerAddress(address), null);
  assert.deepEqual(readDashboard("bad"), {
    favorites: [],
    servers: [],
    hiddenServers: [],
    todos: [],
  });
  const data = readDashboard(
    JSON.stringify({
      version: 1,
      favorites: [
        { instance: "a", version: "b" },
        { instance: "a", version: "b" },
      ],
      todos: [
        { id: "c", text: " hello ", done: true },
        { id: "c", text: "duplicate" },
        { id: "d", text: "  " },
      ],
    }),
  );
  assert.deepEqual(data.favorites, [{ instance: "a", version: "b" }]);
  assert.deepEqual(data.todos, [{ id: "c", text: "hello", done: true }]);
  assert.deepEqual(
    readClockLayout(
      '{"version":1,"clock":{"scale":500,"angle":-90,"date":false}}',
    ),
    { scale: 160, angle: -45, date: false },
  );
  assert.deepEqual(
    readHomeWidgets(JSON.stringify({ version: 1, widgets: homeWidgets })),
    homeWidgets,
  );
  assert.ok(pixelTimePath("12:34").startsWith("M"));
  assert.notEqual(pixelTimePath("12:34"), pixelTimePath("12:35"));
});

test("favorites and notes persist, deduplicate and do not disappear if storage fails", () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  const storage = new Map();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (k) => storage.get(k),
      setItem: (k, v) => storage.set(k, v),
    },
  });
  try {
    saveDashboard({
      favorites: [],
      servers: [],
      hiddenServers: [],
      todos: [{ id: "todo", text: "build", done: false }],
    });
    assert.equal(addFavorite({ instance: "a", version: "b" }), true);
    assert.equal(addFavorite({ instance: "a", version: "b" }), false);
    assert.equal(readDashboard([...storage.values()][0]).todos.length, 1);
    localStorage.setItem = () => {
      throw Error("disk unavailable");
    };
    assert.throws(() => addFavorite({ instance: "c", version: "d" }));
    assert.equal(dashboard.value.favorites.length, 1);
  } finally {
    if (previous) Object.defineProperty(globalThis, "localStorage", previous);
    else delete globalThis.localStorage;
  }
});

test("local/status requests coalesce and keep cached data; unavailable servers are cached too", async () => {
  const task = deferred();
  let calls = 0;
  const rpc = () => {
    calls++;
    return task.promise;
  };
  const a = loadLocalWidgets(rpc, true),
    b = loadLocalWidgets(rpc, true);
  assert.equal(a, b);
  task.resolve({ servers: [], worlds: [{ name: "world" }] });
  await a;
  await loadLocalWidgets(rpc);
  assert.equal(calls, 1);
  await assert.rejects(
    loadLocalWidgets(() => Promise.reject(Error("unavailable")), true),
  );
  assert.equal(localWidgetData.value.worlds[0].name, "world");
  const status = deferred();
  let queries = 0;
  const request = () => {
    queries++;
    return status.promise;
  };
  const p = loadServerStatus(request, "localhost"),
    q = loadServerStatus(request, "localhost");
  assert.equal(p, q);
  status.reject(Error("offline"));
  await p;
  await loadServerStatus(request, "localhost");
  assert.equal(queries, 1);
  assert.match(serverStatuses.value.localhost.error, /offline/);
});

test("shortcut launch holds the clicked version and server through preparation, and stops on failed selection", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions[0].installed = true;
    instance.versions.push({ id: "favorite", installed: true });
    f.state.database.instances.push({
      ...instance,
      id: "other",
      name: "Other",
    });
    await f.context.sync();
    f.handlers.select_version = ({ version }) => {
      instance.selected_version = version;
      return true;
    };
    f.handlers.launch = () => 123;
    f.context.launchReports[instance.id] = null;
    const select = deferred();
    f.handlers.select_version = async ({ version }) => {
      await select.promise;
      instance.selected_version = version;
      return true;
    };
    const work = f.context.startShortcut(
      { instance: instance.id, version: "favorite" },
      "localhost:25566",
    );
    await flush();
    assert.equal(f.context.startRequested.value, true);
    f.context.selectInstance(f.state.database.instances[1].id);
    select.resolve();
    await work;
    assert.deepEqual(
      f.calls.find((c) => c.op === "launch"),
      { op: "launch", id: instance.id, server: "localhost:25566" },
    );
    f.calls.length = 0;
    instance.selected_version = instance.versions[0].id;
    await f.context.sync();
    f.handlers.select_version = () => false;
    await f.context.startShortcut(
      { instance: instance.id, version: "favorite" },
      "localhost",
    );
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      false,
    );
  } finally {
    f.dispose();
  }
});
