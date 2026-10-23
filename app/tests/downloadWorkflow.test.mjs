import assert from "node:assert/strict";
import test from "node:test";
import { computed, reactive } from "vue";
import {
  createWorkflow,
  applyWorkflowEvent,
  finishWorkflow,
  fileWindow,
  filePercent,
  activeFilesFirst,
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

test("下载中的文件稳定置顶，字节变化不触发排序，其余文件保留原顺序", () => {
  const rows = reactive([
    file("a", "done", 100),
    file("b", "downloading", 40),
    file("c"),
    file("d", "downloading", 20),
  ]);
  let evaluations = 0;
  const ordered = computed(() => {
    evaluations++;
    return activeFilesFirst(rows);
  });
  assert.deepEqual(
    ordered.value.map((file) => file.id),
    ["b", "d", "a", "c"],
  );
  rows[1].downloaded = 70;
  assert.equal(ordered.value[0].downloaded, 70);
  assert.equal(evaluations, 1);
  rows[1].status = "done";
  assert.deepEqual(
    ordered.value.map((file) => file.id),
    ["d", "a", "b", "c"],
  );
  assert.equal(evaluations, 2);
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
      ui.logs.value.filter((l) => l.message === "").length,
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
