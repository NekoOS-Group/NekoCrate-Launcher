import assert from "node:assert/strict";
import test from "node:test";
import { recoveryActions, automaticRepairs } from "../src/launchRecovery.ts";
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

test("only supported, actionable findings mark a version for automatic repair", () => {
  assert.deepEqual(
    automaticRepairs(
      report(["files", "missing_dependencies"], { repairable: true }),
    ),
    ["repair", "dependencies"],
  );
  assert.deepEqual(
    automaticRepairs(report(["files"], { repairable: false })),
    [],
  );
  assert.deepEqual(
    automaticRepairs(report(["java", "heap", "duplicate", "mixin"])),
    [],
  );
});

test("repair and launch keeps its target and hex progress through both repair phases", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions[0].installed = true;
    instance.versions.push({ id: "other", installed: true });
    await f.context.sync();
    f.context.launchReports[instance.id] = report(["missing_dependencies"]);
    assert.equal(f.context.selectedNeedsRepair.value, true);
    f.context.clearRecovery(instance.id);
    assert.equal(
      f.context.selectedNeedsRepair.value,
      true,
      "clearing logs must not clear required repairs",
    );
    const prepare = deferred(),
      install = deferred();
    f.handlers.prepare_missing_dependencies = () => prepare.promise;
    f.handlers.install_resources = () => install.promise;
    f.handlers.select_version = ({ version }) => {
      instance.selected_version = version;
      return true;
    };
    f.handlers.launch = () => 123;
    const pending = f.context.startGame();
    await flush();
    assert.equal(f.context.selectedRepairing.value, true);
    assert.equal(f.context.instanceProgress.value.instance_id, instance.id);
    assert.equal(f.context.instanceProgress.value.indeterminate, true);
    assert.equal(
      f.context.current.value.operation,
      "prepare_missing_dependencies",
    );
    await f.context.openInstance(f.context.selectedInstance.value);
    assert.equal(
      f.context.modal.value,
      null,
      "busy instance must not open settings",
    );
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      false,
    );
    instance.selected_version = "other";
    await f.context.sync();
    assert.equal(f.context.selectedNeedsRepair.value, false);
    prepare.resolve({
      id: "repair-task",
      target: { instance: instance.id, version: "1.21.1" },
      entries: [
        {
          kind: "mods",
          destination: "mods/aci.jar",
          origin: { platform: "local" },
        },
      ],
    });
    await flush();
    assert.equal(f.context.current.value.operation, "install_resources");
    f.fixture.event({
      payload: {
        kind: "progress",
        instance_id: instance.id,
        total: 10,
        completed: 4,
        bytes: 0,
        total_bytes: 0,
        message: "Downloading",
      },
    });
    assert.equal(f.context.instanceProgress.value.progress, 40);
    assert.equal(f.context.instanceProgress.value.indeterminate, false);
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      false,
    );
    install.resolve(true);
    await pending;
    assert.equal(instance.selected_version, "1.21.1");
    assert.equal(f.calls.filter((c) => c.op === "launch").length, 1);
    assert.equal(f.context.selectedNeedsRepair.value, false);
    assert.equal(f.context.instanceProgress.value, null);
    assert.equal(f.context.selectedRepairing.value, false);
  } finally {
    f.dispose();
  }
});

for (const failure of [
  false,
  new Error("操作已取消"),
  new Error("download failed"),
]) {
  test(`unsuccessful repair never launches or clears the repair state: ${failure}`, async () => {
    const f = await settingsFixture();
    try {
      const instance = f.state.database.instances[0];
      instance.versions[0].installed = true;
      await f.context.sync();
      f.context.launchReports[instance.id] = report(["files"], {
        repairable: true,
      });
      f.handlers.install = () => {
        if (failure instanceof Error) throw failure;
        return failure;
      };
      await f.context.startGame();
      assert.equal(
        f.calls.some((c) => c.op === "launch"),
        false,
      );
      assert.equal(f.context.selectedNeedsRepair.value, true);
      assert.equal(f.context.selectedRepairing.value, false);
      assert.equal(f.context.instanceProgress.value, null);
      assert.equal(f.context.showLogs.value, true);
    } finally {
      f.dispose();
    }
  });
}

test("a persisted diagnosis is loaded once and file repair precedes dependency repair", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions[0].installed = true;
    await f.context.sync();
    delete f.context.launchReports[instance.id];
    const gate = deferred();
    f.handlers.crash_report = () => gate.promise;
    f.handlers.install = () => true;
    f.handlers.prepare_missing_dependencies = () => ({
      id: "repair",
      target: { instance: instance.id, version: "1.21.1" },
      entries: [
        {
          kind: "mods",
          destination: "mods/aci.jar",
          origin: { platform: "local" },
        },
      ],
    });
    f.handlers.install_resources = () => true;
    f.handlers.launch = () => 123;
    const before = f.calls.length;
    const loading = f.context.loadLaunchReport(instance.id);
    const pending = f.context.startGame();
    await flush();
    assert.deepEqual(
      f.calls.slice(before).map((c) => c.op),
      ["crash_report"],
    );
    gate.resolve(
      report(["files", "missing_dependencies"], { repairable: true }),
    );
    await loading;
    await pending;
    assert.deepEqual(
      f.calls
        .slice(before)
        .map((c) => c.op)
        .filter((op) =>
          [
            "install",
            "prepare_missing_dependencies",
            "install_resources",
            "launch",
          ].includes(op),
        ),
      [
        "install",
        "prepare_missing_dependencies",
        "install_resources",
        "launch",
      ],
    );
    assert.equal(f.context.launchReports[instance.id], null);
  } finally {
    f.dispose();
  }
});
test("recovery actions follow evidence, deduplicate, and respect repair support", () => {
  assert.deepEqual(recoveryActions(report(["missing_dependencies"])), [
    "dependencies",
    "mods",
  ]);
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

test("missing dependency repair uses the failed version and installs only a validated plan", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions.push({ id: "another", installed: true });
    instance.selected_version = "another";
    await f.context.sync();
    const plan = {
      id: "repair-task",
      target: { instance: instance.id, version: "1.21.1" },
      entries: [
        {
          kind: "mods",
          destination: "mods/aci.jar",
          origin: { platform: "local" },
        },
      ],
    };
    f.handlers.prepare_missing_dependencies = () => plan;
    f.handlers.install_resources = () => true;
    const recovery = {
      instanceId: instance.id,
      report: report(["missing_dependencies"]),
    };
    f.context.launchReports[instance.id] = recovery.report;
    await f.context.recoverLaunch(recovery, "dependencies");
    const prepare = f.calls.find(
      (c) => c.op === "prepare_missing_dependencies",
    );
    assert.equal(prepare.version, "1.21.1");
    assert.equal(prepare.time, 10);
    const installed = f.calls.find((c) => c.op === "install_resources");
    assert.equal(installed.version, "1.21.1");
    assert.equal(installed.task, "repair-task");
    assert.equal(f.context.launchReports[instance.id], null);
    f.handlers.prepare_missing_dependencies = () => {
      throw Error("No verified match for aci");
    };
    await f.context.recoverLaunch(recovery, "dependencies");
    assert.equal(f.calls.filter((c) => c.op === "install_resources").length, 1);
    assert.match(f.context.error.value, /No verified match/);
  } finally {
    f.dispose();
  }
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
    assert.equal(f.context.managedWindows.value[0].initialVersion, "1.21.1");
    assert.equal(f.context.managedWindows.value[0].initialTab, "mods");
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

test("already satisfied dependencies clear stale diagnosis and launch without reinstalling", async () => {
  const f = await settingsFixture();
  try {
    const instance = f.state.database.instances[0];
    instance.versions[0].installed = true;
    await f.context.sync();
    f.context.launchReports[instance.id] = report(["missing_dependencies"]);
    f.handlers.prepare_missing_dependencies = () => ({
      id: "noop",
      target: { instance: instance.id, version: "1.21.1" },
      entries: [],
    });
    f.handlers.launch = () => 123;
    await f.context.startGame();
    assert.equal(
      f.calls.some((c) => c.op === "install_resources"),
      false,
    );
    assert.equal(f.calls.filter((c) => c.op === "launch").length, 1);
    assert.equal(f.context.selectedNeedsRepair.value, false);
  } finally {
    f.dispose();
  }
});

test("local component audit opens Mods management without automatically replacing pack contents", () => {
  for (const code of [
    "registry",
    "pack_disabled",
    "pack_missing",
    "pack_changed",
    "component_versions",
    "component_dependencies",
    "component_duplicate",
    "component_corrupt",
  ]) {
    assert.deepEqual(recoveryActions(report([code])), ["mods"], code);
    assert.deepEqual(
      automaticRepairs(report([code], { repairable: true })),
      [],
      code,
    );
  }
});
