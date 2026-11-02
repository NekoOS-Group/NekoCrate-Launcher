import assert from "node:assert/strict";
import test from "node:test";
import { recoveryActions } from "../src/launchRecovery.ts";
import { shouldOpenGameLogs } from "../src/instanceLaunch.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const report = (codes = [], overrides = {}) => ({
  version: "1.21.1",
  time: 10,
  exit: "failed",
  sources: [],
  findings: codes.map((code) => ({
    code,
    title: code,
    advice: "",
    evidence: [],
  })),
  ...overrides,
});
test("recovery actions follow evidence, deduplicate, and respect repair support", () => {
  assert.deepEqual(recoveryActions(report(["java", "launch_java"])), ["java"]);
  assert.deepEqual(recoveryActions(report(["heap", "duplicate"])), [
    "memory",
    "mods",
  ]);
  assert.deepEqual(recoveryActions(report(["files"], { repairable: false })), [
    "mods",
  ]);
  assert.deepEqual(recoveryActions(report(["files"], { repairable: true })), [
    "repair",
  ]);
  assert.deepEqual(
    recoveryActions(
      report([], { file: { path: "latest.log", launcher_log: true } }),
    ),
    ["settings", "report"],
  );
  assert.equal(shouldOpenGameLogs("launch_failed"), true);
  assert.equal(
    shouldOpenGameLogs("launch_failed", { log_behavior: "manual" }),
    false,
  );
});

test("launching state begins immediately, ends on failure, and recovery opens the failed version", async () => {
  const f = await settingsFixture();
  try {
    const gate = deferred();
    f.handlers.launch = () => gate.promise;
    f.handlers.java = () => [];
    f.handlers.instance_directory = () => "/games";
    const instance = f.state.database.instances[0];
    const pending = f.context.launch(instance);
    assert.equal(f.context.selectedStarting.value, true);
    await flush();
    f.fixture.event({
      payload: {
        kind: "launch_failed",
        instance_id: instance.id,
        message: "failed",
        crash: report(["launch_java"]),
      },
    });
    const logSession = f.context.logSession.value;
    gate.reject(new Error("failed"));
    await pending;
    assert.equal(f.context.selectedStarting.value, false);
    assert.equal(f.context.showLogs.value, true);
    assert.equal(f.context.logScope.value, instance.id);
    assert.equal(
      f.context.logSession.value,
      logSession,
      "IPC rejection must not restart the open animation",
    );
    assert.deepEqual(f.context.recoveries.value[0].actions, ["java"]);
    const recovery = f.context.recoveries.value[0];
    await f.context.recoverLaunch(recovery, "java");
    assert.equal(f.context.modal.value, "edit");
    assert.equal(f.context.editing.value.id, instance.id);
    await f.context.recoverLaunch(recovery, "mods");
    assert.equal(f.context.managedInitialVersion.value, "1.21.1");
    assert.equal(f.context.managedInitialTab.value, "mods");
  } finally {
    f.dispose();
  }
});

test("repair keeps the failed target even when a different version is selected", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions.push({ id: "1.20.1", installed: true });
    instance.selected_version = "1.20.1";
    await f.context.sync();
    f.handlers.install = () => true;
    await f.context.recoverLaunch(
      {
        instanceId: instance.id,
        report: report(["files"], { repairable: true }),
      },
      "repair",
    );
    const call = f.calls.find((c) => c.op === "install");
    assert.equal(call.version, "1.21.1");
    assert.equal(call.id, instance.id);
    assert.equal(call.repair, true);
    assert.equal(f.context.tasks.value[0].version_id, "1.21.1");
    assert.equal(f.context.selectedVersion.value.id, "1.20.1");
    assert.ok(!f.calls.some((c) => c.op === "select_version"));
  } finally {
    f.dispose();
  }
});

test("a stale report load cannot resurrect recovery from a previous launch", async () => {
  const f = await settingsFixture();
  try {
    const previous = deferred(),
      launch = deferred();
    f.handlers.crash_report = () => previous.promise;
    f.handlers.launch = () => launch.promise;
    const instance = f.state.database.instances[0];
    f.context.openLogs(instance.id);
    const pending = f.context.launch(instance);
    await flush();
    previous.resolve(report(["heap"]));
    await flush();
    assert.equal(f.context.launchReports[instance.id], null);
    f.state.running = [instance.id];
    f.fixture.event({
      payload: {
        kind: "game_started",
        instance_id: instance.id,
        message: "started",
      },
    });
    assert.equal(f.context.selectedStarting.value, false);
    assert.equal(f.context.selectedRunning.value, true);
    launch.resolve(123);
    await pending;
    assert.deepEqual(f.context.recoveries.value, []);
  } finally {
    f.dispose();
  }
});
