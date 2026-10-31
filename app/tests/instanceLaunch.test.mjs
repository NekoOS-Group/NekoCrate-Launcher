import assert from "node:assert/strict";
import test from "node:test";
import {
  instanceLaunchOptions,
  validLaunchSize,
  createLaunchWindowBehavior,
  shouldOpenGameLogs,
} from "../src/instanceLaunch.ts";
import { settingsFixture, deferred } from "./helpers/appSettingsFixture.mjs";
function windowFixture(initiallyMinimized = false) {
  let minimized = initiallyMinimized;
  const calls = [],
    errors = [];
  const win = {
    isMinimized: async () => minimized,
    minimize: async () => {
      calls.push("minimize");
      minimized = true;
    },
    unminimize: async () => {
      calls.push("restore");
      minimized = false;
    },
    setFocus: async () => {
      calls.push("focus");
    },
  };
  return {
    win,
    calls,
    errors,
    event: createLaunchWindowBehavior(
      () => win,
      (e) => errors.push(e),
    ),
  };
}
test("default launch leaves the window alone; opted-in launches wait for the last game to exit", async () => {
  const f = windowFixture();
  await f.event("game_started", "a", instanceLaunchOptions());
  await f.event("game_exit", "a");
  assert.deepEqual(f.calls, []);
  const config = instanceLaunchOptions({ minimize_launcher: true });
  await f.event("game_started", "a", config);
  await f.event("game_started", "b", config);
  await f.event("game_exit", "a");
  assert.deepEqual(f.calls, ["minimize"]);
  await f.event("game_exit", "b");
  assert.deepEqual(f.calls, ["minimize", "restore", "focus"]);
});
test("manual minimize is respected, crashes restore for diagnosis", async () => {
  const f = windowFixture(true);
  await f.event(
    "game_started",
    "a",
    instanceLaunchOptions({ minimize_launcher: true }),
  );
  await f.event("game_exit", "a");
  assert.deepEqual(f.calls, []);
  const g = windowFixture(true);
  await g.event("game_crash", "b");
  assert.deepEqual(g.calls, ["restore", "focus"]);
});
test("a quick game exit is serialized after a pending minimize", async () => {
  const f = windowFixture(),
    wait = deferred();
  f.win.minimize = async () => {
    f.calls.push("minimize");
    await wait.promise;
  };
  const start = f.event(
    "game_started",
    "a",
    instanceLaunchOptions({ minimize_launcher: true }),
  );
  const exit = f.event("game_exit", "a");
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(f.calls, ["minimize"]);
  wait.resolve();
  await Promise.all([start, exit]);
  assert.deepEqual(f.calls, ["minimize", "restore", "focus"]);
});
test("instance settings save all options, trim lines, and remain scoped to the instance", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.java = () => [];
    f.handlers.instance_directory = () => "/games";
    f.handlers.update = () => true;
    await f.context.edit(f.state.database.instances[0]);
    assert.equal(f.context.form.launch.window_mode, "default");
    Object.assign(f.context.form.launch, {
      window_mode: "windowed",
      width: 1440,
      height: 900,
      minimize_launcher: true,
      jvm_args: [
        " -Dpack=My Pack ",
        "",
        " --add-opens=java.base/java.lang=ALL-UNNAMED ",
      ],
    });
    await f.context.submit();
    const update = f.calls.find((x) => x.op === "update");
    assert.equal(update.id, "instance-a");
    assert.deepEqual(update.launch, {
      isolation: null,
      log_behavior: "crash",
      game_args: [],
      environment: [],
      window_mode: "windowed",
      width: 1440,
      height: 900,
      minimize_launcher: true,
      jvm_args: [
        "-Dpack=My Pack",
        "--add-opens=java.base/java.lang=ALL-UNNAMED",
      ],
    });
    assert.equal(validLaunchSize({ ...update.launch, width: 0 }), false);
  } finally {
    f.dispose();
  }
});

test("log policy opens only on the chosen events and startup logs take priority over minimize", async () => {
  for (const [log_behavior, started, crashed] of [
    ["crash", false, true],
    ["startup", true, true],
    ["manual", false, false],
  ]) {
    const options = instanceLaunchOptions({
      log_behavior,
      minimize_launcher: true,
    });
    assert.equal(shouldOpenGameLogs("game_started", options), started);
    assert.equal(shouldOpenGameLogs("game_crash", options), crashed);
    assert.equal(shouldOpenGameLogs("game_exit", options), false);
  }
  const f = windowFixture(true);
  await f.event(
    "game_started",
    "a",
    instanceLaunchOptions({ log_behavior: "startup", minimize_launcher: true }),
  );
  assert.deepEqual(f.calls, ["restore", "focus"]);
  await f.event("game_exit", "a");
  assert.deepEqual(f.calls, ["restore", "focus"]);
  const manual = windowFixture(true);
  await manual.event(
    "game_crash",
    "a",
    instanceLaunchOptions({ log_behavior: "manual" }),
  );
  assert.deepEqual(manual.calls, []);
  const owned = windowFixture();
  const config = instanceLaunchOptions({
    log_behavior: "manual",
    minimize_launcher: true,
  });
  await owned.event("game_started", "a", config);
  await owned.event("game_crash", "a", config);
  assert.deepEqual(owned.calls, ["minimize", "restore", "focus"]);
});

test("isolation preview ignores stale results; launch arguments and environment survive saving", async () => {
  const f = await settingsFixture();
  try {
    const old = deferred();
    f.handlers.java = () => [];
    f.handlers.instance_directory = () => "/games";
    f.handlers.launch_directory = ({ isolation }) =>
      isolation === "version"
        ? {
            path: "/games/versions/1.21.1",
            mode: "version",
            changed: true,
            protected_pack: false,
          }
        : old.promise;
    f.handlers.update = () => true;
    await f.context.edit(f.state.database.instances[0]);
    const { flush } = await import("./helpers/appSettingsFixture.mjs");
    await flush();
    f.context.form.launch.isolation = "version";
    f.context.form.launch.log_behavior = "manual";
    f.context.form.launch.game_args = [" --server ", "", "mc.example.org"];
    f.context.form.launch.environment = ["PACK=one=two  ", "", "EMPTY="];
    await flush();
    old.resolve({
      path: "/games",
      mode: "shared",
      changed: false,
      protected_pack: false,
    });
    await flush();
    assert.equal(
      f.context.launchDirectory.value.path,
      "/games/versions/1.21.1",
    );
    await f.context.submit();
    const { launch } = f.calls.find((call) => call.op === "update");
    assert.equal(launch.isolation, "version");
    assert.equal(launch.log_behavior, "manual");
    assert.deepEqual(launch.game_args, ["--server", "mc.example.org"]);
    assert.deepEqual(launch.environment, ["PACK=one=two  ", "EMPTY="]);
  } finally {
    f.dispose();
  }
});

test("game events route the log window according to instance settings", async () => {
  const f = await settingsFixture();
  try {
    const { flush } = await import("./helpers/appSettingsFixture.mjs");
    for (const [log_behavior, kind, opened] of [
      ["manual", "game_crash", false],
      ["crash", "game_started", false],
      ["crash", "game_crash", true],
      ["startup", "game_started", true],
    ]) {
      f.state.database.instances[0].launch = instanceLaunchOptions({
        log_behavior,
      });
      await f.context.sync();
      f.context.showLogs.value = false;
      f.context.onEvent({
        kind,
        instance_id: "instance-a",
        message: "fixture",
      });
      await flush();
      assert.equal(
        f.context.showLogs.value,
        opened,
        `${log_behavior}: ${kind}`,
      );
      if (opened) assert.equal(f.context.logScope.value, "instance-a");
    }
  } finally {
    f.dispose();
  }
});
