import assert from "node:assert/strict";
import test from "node:test";
import { computed, reactive } from "vue";
import {
  createWorkflow,
  applyWorkflowEvent,
  finishWorkflow,
  fileWindow,
  filePercent,
} from "../src/downloadWorkflow.ts";
import { activeDialog, registerDialog } from "../src/dialogSurface.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const event = (kind, step, files = []) => ({
  kind,
  step,
  files,
  message: "",
  instance_id: "instance-a",
  completed: 0,
  total: 0,
  bytes: 0,
});
const file = (id, status = "queued", downloaded = 0) => ({
  id,
  name: "client.jar",
  status,
  downloaded,
  total: 100,
});

test("加载器的整体进度不被单个安装阶段的 100% 覆盖", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const download = deferred();
  fixture.handlers.install = () => download.promise;
  const instance = fixture.state.database.instances[0];
  instance.versions.find((v) => v.id === instance.selected_version).loader = {
    kind: "forge",
    version: "47.4.4",
  };
  const installing = ui.install(instance);
  try {
    fixture.event({
      payload: {
        ...event("progress", "loader"),
        completed: 1,
        total: 1,
        overall_progress: 45,
      },
    });
    assert.equal(ui.current.value.download_progress, 45);
    fixture.event({
      payload: { ...event("workflow", "files"), overall_progress: 45 },
    });
    fixture.event({
      payload: {
        ...event("progress", "files"),
        completed: 1,
        total: 2,
        overall_progress: 67.5,
      },
    });
    assert.equal(ui.current.value.progress, 67.5);
    fixture.event({
      payload: {
        ...event("progress", "files"),
        completed: 0,
        total: 2,
        reset_progress: true,
        overall_progress: 60,
      },
    });
    assert.equal(ui.current.value.progress, 67.5);
  } finally {
    download.resolve(true);
    await installing;
    dispose();
  }
});

test("普通安装复用校验记录，手动校验修复明确请求完整检查", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  fixture.handlers.install = () => true;
  try {
    const instance = fixture.state.database.instances[0];
    await ui.install(instance);
    await ui.install(instance, true);
    assert.deepEqual(
      fixture.calls
        .filter((call) => call.op === "install")
        .map((call) => ({ id: call.id, repair: call.repair })),
      [
        { id: instance.id, repair: false },
        { id: instance.id, repair: true },
      ],
    );
  } finally {
    dispose();
  }
});

test("续传列表初始化已有字节，校验时保留进度但不提前标为完成", () => {
  const state = createWorkflow("install", "");
  applyWorkflowEvent(state, event("workflow", "files"));
  applyWorkflowEvent(
    state,
    event("file_plan", "files", [
      file("cached", "queued", 100),
      file("partial", "queued", 65),
    ]),
  );
  assert.equal(filePercent(state.files.get("partial")), 65);
  assert.equal(filePercent(state.files.get("cached")), 100);
  assert.equal(state.steps[1].filesDone, 0);
  applyWorkflowEvent(
    state,
    event("file", "files", [file("partial", "verifying", 65)]),
  );
  assert.equal(filePercent(state.files.get("partial")), 65);
  applyWorkflowEvent(
    state,
    event("file", "files", [{ ...file("cached", "done", 100), cached: true }]),
  );
  assert.equal(state.steps[1].filesDone, 1);
  assert.equal(state.files.get("cached").cached, true);
});

test("暂停和继续保留同一任务与进度，完成后仍执行原来的启动请求", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const download = deferred();
  fixture.handlers.install = () => download.promise;
  fixture.handlers.pause = () => true;
  fixture.handlers.resume = () => true;
  fixture.handlers.launch = () => 123;
  const launching = ui.startGame();
  try {
    const task = ui.current.value;
    fixture.event({ payload: event("workflow", "files") });
    fixture.event({
      payload: {
        ...event("progress", "files"),
        completed: 1,
        total: 2,
        bytes: 60,
        total_bytes: 100,
      },
    });
    await ui.setDownloadPaused(true);
    assert.equal(ui.current.value, task);
    assert.equal(task.status, "paused");
    assert.equal(task.workflow.steps[1].status, "paused");
    assert.equal(task.download_progress, 60);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.visibleError.value, "");
    await ui.openInstance(fixture.state.database.instances[0]);
    assert.equal(ui.utility.value, "downloads");
    await ui.setDownloadPaused(true);
    assert.equal(fixture.calls.filter((call) => call.op === "pause").length, 1);
    await ui.setDownloadPaused(false);
    assert.equal(ui.current.value, task);
    assert.equal(task.status, "running");
    assert.equal(task.workflow.steps[1].status, "running");
    assert.equal(task.progress, 60);
    assert.equal(
      fixture.calls.filter((call) => call.op === "install").length,
      1,
    );
    download.resolve(true);
    await launching;
    assert.equal(
      fixture.calls.filter((call) => call.op === "launch").length,
      1,
    );
  } finally {
    download.resolve(true);
    await launching;
    dispose();
  }
});

test("安装流程提前列出后续步骤；文件按完整路径区分，重试和校验不会被提前标为完成", () => {
  const state = createWorkflow("install", "安装游戏");
  assert.deepEqual(
    state.steps.map((s) => s.id),
    ["metadata", "files", "natives", "java", "finalize"],
  );
  assert.deepEqual(
    state.steps.map((s) => s.status),
    ["running", "pending", "pending", "pending", "pending"],
  );
  applyWorkflowEvent(state, event("workflow", "files"));
  applyWorkflowEvent(
    state,
    event("file_plan", "files", [
      file("one/client.jar"),
      file("two/client.jar"),
    ]),
  );
  applyWorkflowEvent(
    state,
    event("file", "files", [file("one/client.jar", "downloading", 80)]),
  );
  applyWorkflowEvent(
    state,
    event("file_plan", "files", [file("one/client.jar")]),
  );
  assert.equal(state.files.get("one/client.jar").downloaded, 80);
  assert.equal(state.files.size, 2);
  applyWorkflowEvent(
    state,
    event("file", "files", [file("one/client.jar", "downloading", 0)]),
  );
  assert.equal(
    filePercent(state.files.get("one/client.jar")),
    0,
    "重试必须显示当前尝试的真实进度",
  );
  applyWorkflowEvent(
    state,
    event("file", "files", [file("one/client.jar", "verifying", 100)]),
  );
  assert.equal(state.steps[1].filesDone, 0);
  applyWorkflowEvent(
    state,
    event("file", "files", [file("one/client.jar", "done", 100)]),
  );
  assert.equal(state.steps[1].filesDone, 1);
  applyWorkflowEvent(
    state,
    event("file", "files", [file("one/client.jar", "done", 100)]),
  );
  assert.equal(state.steps[1].filesDone, 1);
  applyWorkflowEvent(state, event("workflow", "java"));
  assert.deepEqual(
    state.steps.map((s) => s.status),
    ["done", "done", "done", "running", "pending"],
  );
  finishWorkflow(state, "done");
  assert.ok(state.steps.every((s) => s.status === "done"));
  assert.equal(state.steps[1].filesDone, 2);
});

test("失败、取消保留已完成的步骤和字节数，未执行的步骤不会显示成功", () => {
  for (const result of ["error", "cancelled"]) {
    const state = createWorkflow("install", "安装游戏");
    applyWorkflowEvent(state, event("workflow", "files"));
    applyWorkflowEvent(
      state,
      event("file_plan", "files", [file("a"), file("b"), file("c")]),
    );
    applyWorkflowEvent(
      state,
      event("file", "files", [
        file("a", "done", 100),
        file("b", "downloading", 45),
      ]),
    );
    applyWorkflowEvent(state, event("file", "files", [file("b", result)]));
    finishWorkflow(state, result);
    assert.equal(state.files.get("b").downloaded, 45);
    assert.equal(state.files.get("c").status, "queued");
    assert.deepEqual(
      state.steps.map((s) => s.status),
      ["done", result, "pending", "pending", "pending"],
    );
    assert.equal(state.steps[1].filesDone, 1);
  }
});

test("一万个文件完整保留，可滚动到最后一个，每次最多渲染十行", () => {
  const files = Array.from({ length: 10000 }, (_, i) => file(`${i}`));
  assert.equal(fileWindow(files, 0).items.length, 10);
  const end = fileWindow(files, files.length * 48 - 192);
  assert.equal(end.items.at(-1).id, "9999");
  assert.ok(end.items.length <= 10);
  assert.equal(fileWindow([], 1000).height, 0);
});

test("右上角通知随原生弹窗切换层级，关闭弹窗后回到主界面", () => {
  const settings = {},
    logs = {};
  const closeSettings = registerDialog(settings);
  const closeLogs = registerDialog(logs);
  assert.equal(activeDialog.value, logs);
  closeSettings();
  assert.equal(activeDialog.value, logs);
  closeLogs();
  assert.equal(activeDialog.value, null);
});

test("IPC 文件事件只更新所属任务，失败统一提供给角落通知，关闭通知不改任务结果", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const download = deferred();
  fixture.handlers.install = () => download.promise;
  const installing = ui.install(fixture.state.database.instances[0]);
  try {
    await ui.openInstance(fixture.state.database.instances[0]);
    fixture.event({ payload: event("workflow", "files") });
    fixture.event({ payload: event("file_plan", "files", [file("a")]) });
    fixture.event({
      payload: {
        ...event("file", "files", [file("unrelated")]),
        instance_id: "instance-b",
      },
    });
    fixture.event({
      payload: event("file", "files", [file("a", "downloading", 55)]),
    });
    assert.equal(ui.displayedTask.value.workflow.files.size, 1);
    assert.equal(ui.displayedTask.value.workflow.files.get("a").downloaded, 55);
    assert.equal(
      ui.logStore.snapshot().filter((l) => l.message === "").length,
      1,
      "仅流程测试事件可写日志，文件事件不写日志",
    );
    download.reject(
      new Error("未找到该官方游戏版本；原型暂不支持自定义加载器版本"),
    );
    await installing;
    assert.equal(ui.utility.value, "downloads");
    assert.equal(ui.displayedTask.value.status, "error");
    assert.match(ui.visibleError.value, /未找到该官方游戏版本/);
    ui.dismissError();
    await flush();
    assert.equal(ui.visibleError.value, "");
    assert.equal(ui.displayedTask.value.workflow.steps[1].status, "error");
  } finally {
    download.resolve(true);
    await installing;
    dispose();
  }
});

test("retry updates the original file stage and clears stale failure state", () => {
  const state = createWorkflow("install", "");
  applyWorkflowEvent(state, event("workflow", "files"));
  applyWorkflowEvent(
    state,
    event("file", "files", [
      { ...file("bad", "error", 60), retryable: true, error: "network" },
    ]),
  );
  applyWorkflowEvent(state, event("workflow", "java"));
  applyWorkflowEvent(
    state,
    event("file", "java", [file("java", "downloading", 20)]),
  );
  applyWorkflowEvent(
    state,
    event("file", "java", [{ ...file("bad", "done", 100), cached: true }]),
  );
  assert.equal(state.steps.find((s) => s.id === "files").filesDone, 1);
  assert.equal(state.steps.find((s) => s.id === "java").filesDone, 0);
  assert.equal(state.steps.find((s) => s.id === "files").activity.reused, 1);
  assert.equal(state.files.get("bad").retryable, false);
  assert.equal(state.files.get("bad").error, undefined);
  finishWorkflow(state, "error");
  assert.equal(
    state.steps.find((s) => s.id === "java").activity.downloading,
    undefined,
  );
  assert.equal(state.steps.find((s) => s.id === "java").activity.cancelled, 1);
});

test("continuing a failed install retains completed files and partial bytes", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const pending = deferred();
  fixture.handlers.install = () => pending.promise;
  const instance = fixture.state.database.instances[0];
  try {
    const installing = ui.install(instance);
    await flush();
    fixture.event({ payload: event("workflow", "files") });
    fixture.event({
      payload: event("file_plan", "files", [
        file("cached", "done", 100),
        file("bad", "downloading", 50),
      ]),
    });
    pending.reject(new Error("network unavailable"));
    await installing;
    assert.equal(ui.tasks.value[0].workflow.files.get("cached").status, "done");
    const continuing = deferred();
    fixture.handlers.install = () => continuing.promise;
    const resumed = ui.install(instance);
    await flush();
    assert.equal(ui.current.value.workflow.files.get("cached").status, "done");
    assert.equal(ui.current.value.workflow.files.get("bad").downloaded, 50);
    fixture.event({
      payload: event("file_plan", "files", [
        file("cached", "queued", 0),
        file("bad", "queued", 0),
      ]),
    });
    assert.equal(ui.current.value.workflow.files.get("cached").downloaded, 100);
    assert.equal(ui.current.value.workflow.files.get("bad").downloaded, 50);
    continuing.resolve(true);
    await resumed;
  } finally {
    dispose();
  }
});

test("single-file retry stays on the requested task and does not replace whole-task progress", async () => {
  const f = await settingsFixture();
  try {
    const workflow = createWorkflow("install", "安装游戏");
    applyWorkflowEvent(workflow, event("workflow", "files"));
    applyWorkflowEvent(
      workflow,
      event("file_plan", "files", [
        { ...file("failed", "error", 40), retryable: true, error: "HTTP 503" },
        { ...file("cached", "done", 100), cached: true },
      ]),
    );
    finishWorkflow(workflow, "error");
    f.context.tasks.value = [
      {
        id: 1,
        instance_id: "instance-a",
        version_id: "1.21.1",
        status: "error",
        progress: 70,
        download_progress: 70,
        workflow,
      },
    ];
    const task = f.context.tasks.value[0];
    f.context.tasks.value.unshift({
      id: 2,
      status: "done",
      workflow: createWorkflow("versions", "同步版本目录"),
    });
    f.handlers.retry_download_file = ({ file: id }) => {
      assert.equal(id, "failed");
      f.fixture.event({
        payload: {
          ...event("progress", "files"),
          completed: 1,
          total: 1,
          overall_progress: 100,
        },
      });
      f.fixture.event({
        payload: event("file", "files", [file("failed", "done", 100)]),
      });
      return true;
    };
    await f.context.retryDownloadFile("failed", task);
    assert.equal(task.progress, 70);
    assert.equal(task.download_progress, 70);
    assert.equal(task.workflow.files.get("failed").status, "done");
    assert.equal(task.workflow.files.get("cached").cached, true);
    assert.equal(
      task.workflow.steps.find((s) => s.id === "files").status,
      "done",
    );
    assert.equal(task.status, "error"); // Remaining installation still needs Continue.
    assert.equal(f.context.current.value, null);
    assert.equal(f.context.busy.value, false);
  } finally {
    f.dispose();
  }
});

test("crash diagnosis stays in the scoped log without a separate report panel", async () => {
  const f = await settingsFixture();
  try {
    f.fixture.event({
      payload: {
        ...event("diagnosis"),
        message: "[ERROR] [Diagnosis] Java 模块冲突",
      },
    });
    f.fixture.event({ payload: event("game_crash") });
    await flush();
    assert.equal(f.context.showLogs.value, true);
    assert.equal(f.context.logScope.value, "instance-a");
    assert.equal(
      f.context.logStore.snapshot().at(-2).message,
      "[ERROR] [Diagnosis] Java 模块冲突",
    );
    assert.equal(
      f.fixture.calls.some((call) => call.op === "crash_report"),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("整合包解析进度与下载文件共享阶段，元数据完成不会伪造下载完成量", () => {
  const workflow = createWorkflow("modpack", "Pack");
  assert.equal(workflow.steps[0].id, "pack_files");
  assert.equal(
    workflow.steps.some((step) => step.id === "pack_resolve"),
    false,
  );
  applyWorkflowEvent(workflow, {
    ...event("workflow_progress", "pack_files"),
    completed: 100,
    total: 435,
    message: "获取项目资料与下载地址（第 2 / 5 批）",
  });
  applyWorkflowEvent(
    workflow,
    event("file_plan", "pack_files", [file("first")]),
  );
  applyWorkflowEvent(
    workflow,
    event("file", "pack_files", [file("first", "done", 100)]),
  );
  assert.deepEqual(workflow.steps[0].resolved, { completed: 100, total: 435 });
  assert.equal(workflow.steps[0].filesDone, 1);
  assert.equal(workflow.steps[0].status, "running");
});

test("需手动补齐的整合包进入等待状态，无错误弹窗，也不会启动游戏", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const instance = fixture.state.database.instances[0];
  instance.versions.find((v) => v.id === instance.selected_version).modpack =
    true;
  const missing = {
    name: "Pack",
    completed: false,
    missing: [
      {
        key: "manual",
        path: "mods/manual.jar",
        display_name: "Manual mod",
        manual_url:
          "https://www.curseforge.com/minecraft/mc-mods/manual/files/1",
      },
    ],
  };
  fixture.handlers.install = () => {
    fixture.event({
      payload: { ...event("pack_manual", "pack_files"), pack_missing: missing },
    });
    fixture.event({ payload: { ...event("workflow", "java") } });
    return false;
  };
  fixture.handlers.modpack_status = () => missing;
  try {
    await ui.sync();
    assert.equal(await ui.install(instance), false);
    const task = ui.tasks.value[0];
    assert.equal(task.status, "waiting");
    assert.equal(task.workflow.steps[0].status, "done");
    assert.equal(task.workflow.steps.at(-1).id, "pack_manual");
    assert.equal(task.workflow.steps.at(-1).status, "waiting");
    assert.equal(
      task.workflow.steps.some((step) => step.status === "running"),
      false,
    );
    assert.equal(task.modpack_status.missing[0].key, "manual");
    assert.equal(ui.error.value, "");
    assert.equal(ui.busy.value, false);
    assert.equal(ui.utility.value, "downloads");
    assert.equal(
      fixture.calls.some((call) => call.op === "launch"),
      false,
    );
    const workflow = task.workflow;
    ui.utility.value = null;
    const installs = fixture.calls.filter(
      (call) => call.op === "install",
    ).length;
    await ui.startGame();
    assert.equal(ui.utility.value, "downloads");
    assert.equal(
      fixture.calls.filter((call) => call.op === "install").length,
      installs,
    );
    fixture.handlers.install = () => true;
    fixture.handlers.select_version = () => true;
    await ui.resumePack();
    assert.equal(ui.tasks.value[0].workflow, workflow);
    assert.equal(ui.tasks.value[0].status, "done");
    assert.equal(ui.error.value, "");
  } finally {
    dispose();
  }
});

test("拖入手动资源框，匹配失败留在区域内，补齐后继续", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const instance = fixture.state.database.instances[0];
  instance.versions.find((v) => v.id === instance.selected_version).modpack =
    true;
  const status = {
    name: "Pack",
    completed: false,
    missing: [
      { key: "manual", path: "mods/manual.jar", display_name: "Manual" },
    ],
  };
  fixture.handlers.install = () => false;
  fixture.handlers.modpack_status = () => status;
  try {
    await ui.sync();
    await ui.install(instance);
    ui.utility.value = "downloads";
    fixture.hovered = {
      dataset: {},
      closest: (selector) =>
        selector.includes("data-manual-pack-drop") ? fixture.hovered : null,
      getBoundingClientRect: () => ({
        left: 40,
        top: 50,
        width: 300,
        height: 100,
      }),
    };
    fixture.fileDrop({
      payload: {
        type: "enter",
        position: { x: 100, y: 100 },
        paths: ["/Downloads/manual.jar"],
      },
    });
    assert.equal(ui.utility.value, "downloads");
    assert.equal(ui.manualPackDragging.value, true);
    fixture.fileDrop({ payload: { type: "leave" } });
    assert.equal(ui.manualPackDragging.value, false);
    fixture.handlers.supply_modpack_files = () => {
      throw new Error("所选文件与整合包声明的文件校验值不匹配");
    };
    fixture.fileDrop({
      payload: { type: "drop", paths: ["/Downloads/wrong.jar"] },
    });
    await flush();
    assert.match(ui.manualPackError.value, /校验值不匹配/);
    assert.equal(ui.error.value, "");
    assert.equal(
      fixture.calls.filter((call) => call.op === "inspect_resources").length,
      1,
    );
    fixture.handlers.supply_modpack_files = (request) => {
      assert.equal(request.id, instance.id);
      assert.equal(request.version, instance.selected_version);
      assert.deepEqual(request.paths, ["/Downloads/manual.jar"]);
      status.missing = [];
      return 1;
    };
    fixture.handlers.select_version = () => true;
    fixture.handlers.install = () => true;
    fixture.fileDrop({
      payload: { type: "drop", paths: ["/Downloads/manual.jar"] },
    });
    await flush();
    await flush();
    assert.equal(ui.manualPackError.value, "");
    assert.equal(ui.tasks.value[0].status, "done");
  } finally {
    dispose();
  }
});

test("自动安装期间仍可拖入手动文件，补齐不会另开安装任务", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const instance = fixture.state.database.instances[0];
  instance.versions.find((v) => v.id === instance.selected_version).modpack =
    true;
  const download = deferred();
  const status = {
    name: "Pack",
    completed: false,
    missing: [{ key: "manual", path: "mods/manual.jar" }],
  };
  fixture.handlers.install = () => download.promise;
  fixture.handlers.modpack_status = () => status;
  fixture.handlers.supply_modpack_files = () => {
    status.missing = [];
    return 1;
  };
  await ui.sync();
  const installing = ui.install(instance);
  try {
    fixture.event({
      payload: { ...event("pack_manual", "pack_files"), pack_missing: status },
    });
    assert.equal(ui.busy.value, true);
    ui.utility.value = "downloads";
    fixture.hovered = {
      dataset: {},
      closest: (selector) =>
        selector.includes("data-manual-pack-drop") ? fixture.hovered : null,
      getBoundingClientRect: () => ({
        left: 40,
        top: 50,
        width: 300,
        height: 100,
      }),
    };
    fixture.fileDrop({
      payload: {
        type: "enter",
        position: { x: 100, y: 100 },
        paths: ["/Downloads/manual.jar"],
      },
    });
    assert.equal(ui.manualPackDragging.value, true);
    fixture.fileDrop({
      payload: { type: "drop", paths: ["/Downloads/manual.jar"] },
    });
    await flush();
    await flush();
    assert.equal(
      fixture.calls.filter((call) => call.op === "supply_modpack_files").length,
      1,
    );
    assert.equal(
      fixture.calls.filter((call) => call.op === "install").length,
      1,
    );
    assert.equal(ui.busy.value, true);
    assert.equal(ui.manualPackError.value, "");
    assert.equal(ui.current.value.modpack_status.missing.length, 0);
  } finally {
    download.resolve(true);
    await installing;
    dispose();
  }
});
