import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

async function fixtureWithTwoInstances() {
  const fixture = await settingsFixture();
  const { context: ui, fixture: ipc } = fixture;
  const first = ipc.state.database.instances[0];
  const second = {
    ...structuredClone(first),
    id: "instance-b",
    name: "Other",
    java_path: "/old/bin/java",
  };
  ipc.state.database.instances.push(second);
  ipc.handlers.java = () => [];
  ipc.handlers.instance_directory = ({ id }) => `/${id}/.minecraft`;
  ipc.handlers.update = ({ op, id, ...settings }) => {
    Object.assign(
      ipc.state.database.instances.find((i) => i.id === id),
      settings,
    );
    return true;
  };
  await ui.sync();
  return { ...fixture, first, second };
}

test("下载中实例打开任务列表并持续更新进度，其他实例仍能修改名称、图标、内存和 Java 并自动保存", async () => {
  const {
    fixture,
    context: ui,
    first,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const download = deferred();
  fixture.handlers.install = () => download.promise;
  const installing = ui.install(first);
  try {
    const active = ui.current.value;
    await ui.openInstance(first);
    assert.equal(ui.showDownloads.value, true);
    assert.equal(ui.modal.value, null);
    assert.equal(ui.tasks.value[0], active);
    await ui.openInstance(first);
    assert.equal(ui.showDownloads.value, true, "再次打开不应切换为关闭");
    fixture.event({
      payload: {
        kind: "progress",
        instance_id: first.id,
        message: "下载资源",
        completed: 42,
        total: 100,
        bytes: 42,
      },
    });
    assert.equal(ui.tasks.value[0].progress, 42);
    await ui.openInstance(second);
    assert.equal(ui.modal.value, "edit");
    assert.equal(ui.utility.value, null);
    assert.equal(ui.instanceSaving.value, false);
    ui.form.name = "Renamed";
    ui.form.icon = 3;
    ui.form.memory_mb = 4096;
    fixture.pickFile = () => "/chosen/bin/java";
    await ui.chooseJava();
    await flush();
    assert.equal(ui.modal.value, "edit");
    assert.equal(second.name, "Renamed");
    assert.equal(second.icon, 3);
    assert.equal(second.memory_mb, 4096);
    assert.equal(second.java_path, "/chosen/bin/java");
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value, active);
    assert.equal(active.progress, 42);
    assert.equal(active.status, "running");
    assert.equal(ui.tasks.value.length, 1);
    assert.ok(!fixture.calls.some((call) => call.op === "cancel"));
    await ui.edit(second);
    ui.form.name = "Live edit";
    download.resolve(true);
    await installing;
    assert.equal(ui.modal.value, "edit", "下载完成不应关闭其他实例设置");
    assert.equal(ui.form.name, "Live edit");
    await flush();
    assert.equal(second.name, "Live edit");
    assert.equal(ui.current.value, null);
    assert.equal(ui.busy.value, false);
    await ui.openInstance(first);
    assert.equal(ui.modal.value, "edit", "下载结束后双击恢复设置入口");
    assert.equal(ui.editing.value.id, first.id);
  } finally {
    download.resolve(true);
    await installing;
    dispose();
  }
});

test("保存失败不会结束下载或关闭设置，可重试且不会重复提交同一实例", async () => {
  const {
    fixture,
    context: ui,
    first,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const download = deferred();
  const saving = deferred();
  fixture.handlers.install = () => download.promise;
  fixture.handlers.update = () => saving.promise;
  const installing = ui.install(first);
  try {
    await ui.edit(second);
    ui.form.memory_mb = 8192;
    const submission = ui.submit();
    assert.equal(ui.instanceSaving.value, true);
    await flush();
    assert.equal(
      fixture.calls.filter((call) => call.op === "update").length,
      1,
    );
    saving.reject(new Error("write failed"));
    await submission;
    assert.match(ui.visibleError.value, /write failed/);
    assert.equal(ui.modal.value, "edit");
    assert.equal(ui.instanceSaving.value, false);
    assert.equal(ui.form.memory_mb, 8192);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value.status, "running");
    fixture.handlers.update = () => true;
    await ui.retrySettingsSave();
    assert.equal(
      fixture.calls.filter((call) => call.op === "update").length,
      2,
    );
    assert.equal(ui.modal.value, "edit");
    assert.equal(ui.visibleError.value, "");
    assert.equal(ui.busy.value, true);
  } finally {
    download.resolve(true);
    await installing;
    dispose();
  }
});

test("关闭后重开的编辑器不被迟到的保存响应关闭或覆盖", async () => {
  const {
    fixture,
    context: ui,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const saving = deferred();
  try {
    fixture.handlers.update = () => saving.promise;
    await ui.edit(second);
    ui.form.name = "Submitted";
    const submission = ui.submit();
    ui.modal.value = null;
    await flush();
    await ui.edit(second);
    ui.form.name = "New draft";
    saving.resolve(true);
    await submission;
    assert.equal(ui.modal.value, "edit");
    assert.equal(ui.form.name, "New draft");
    assert.equal(ui.instanceSaving.value, false);
  } finally {
    dispose();
  }
});

test("同一实例的自动保存串行合并，关闭气泡也保留最后一次改动", async () => {
  const {
    fixture,
    context: ui,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const firstWrite = deferred();
  const requests = [];
  let active = 0,
    peak = 0;
  fixture.handlers.update = async ({ op, id, ...preferences }) => {
    requests.push(preferences);
    peak = Math.max(peak, ++active);
    if (requests.length === 1) await firstWrite.promise;
    Object.assign(second, preferences);
    active--;
    return true;
  };
  try {
    await ui.edit(second);
    assert.equal(requests.length, 0, "打开设置不应保存");
    ui.form.name = "First";
    await flush();
    ui.form.name = "Latest";
    ui.form.icon = 5;
    ui.form.java_path = "/new/bin/java";
    ui.form.launch.log_behavior = "manual";
    ui.modal.value = null;
    assert.equal(requests.length, 1);
    firstWrite.resolve(true);
    await ui.persistInstanceSettings(second.id);
    assert.equal(peak, 1);
    assert.equal(requests.length, 2);
    assert.equal(second.name, "Latest");
    assert.equal(second.icon, 5);
    assert.equal(second.java_path, "/new/bin/java");
    assert.equal(second.launch.log_behavior, "manual");
    assert.equal(ui.modal.value, null);
    await ui.edit(second);
    assert.equal(ui.form.name, "Latest");
    assert.equal(requests.length, 2, "重开气泡不应重复保存");
  } finally {
    firstWrite.resolve(true);
    await ui.persistInstanceSettings(second.id);
    dispose();
  }
});

test("切换实例不串写；迟到的快照保留偏好但接受游戏状态", async () => {
  const {
    fixture,
    context: ui,
    first,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const delayed = deferred();
  try {
    await ui.edit(first);
    const old = fixture.snapshot();
    old.state.database.instances[0].versions[0].installed = true;
    fixture.handlers.snapshot = () => delayed.promise;
    const syncing = ui.sync();
    ui.form.name = "First updated";
    await ui.edit(second);
    ui.form.name = "Second updated";
    await flush();
    delayed.resolve(old);
    await syncing;
    assert.equal(first.name, "First updated");
    assert.equal(second.name, "Second updated");
    assert.equal(ui.form.name, "Second updated");
    const displayed = ui.snapshot.value.database.instances;
    assert.equal(displayed[0].name, "First updated");
    assert.equal(displayed[1].name, "Second updated");
    assert.equal(displayed[0].versions[0].installed, true);
    assert.equal(ui.modal.value, "edit");
  } finally {
    delayed.resolve(fixture.snapshot());
    dispose();
  }
});

test("未完成的数字输入不覆盖有效值，也不阻止其他设置保存", async () => {
  const {
    fixture,
    context: ui,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  try {
    await ui.edit(second);
    ui.form.memory_mode = "custom";
    await flush();
    ui.form.memory_mb = 0;
    ui.form.icon = 4;
    await flush();
    assert.equal(second.memory_mb, 2048);
    assert.equal(second.icon, 4);
    ui.form.memory_mb = 8192;
    ui.form.launch.window_mode = "windowed";
    await flush();
    ui.form.launch.width = 12;
    ui.form.name = "Valid name";
    await flush();
    assert.equal(second.memory_mb, 8192);
    assert.equal(second.launch.width, 1280);
    assert.equal(second.name, "Valid name");
    ui.form.launch.width = 1440;
    await flush();
    assert.equal(second.launch.width, 1440);
    assert.equal(ui.visibleError.value, "");
  } finally {
    dispose();
  }
});

test("修改后立即启动会等待实例设置保存", async () => {
  const {
    fixture,
    context: ui,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const saving = deferred();
  let started = false;
  fixture.handlers.update = async ({ op, id, ...preferences }) => {
    await saving.promise;
    Object.assign(second, preferences);
    return true;
  };
  fixture.handlers.launch = () => {
    assert.equal(second.java_path, "/selected/bin/java");
    started = true;
    return true;
  };
  try {
    await ui.edit(second);
    ui.form.java_path = "/selected/bin/java";
    ui.modal.value = null;
    const launching = ui.launch(second);
    await flush();
    assert.equal(started, false);
    saving.resolve(true);
    await launching;
    assert.equal(started, true);
    assert.equal(ui.visibleError.value, "");
  } finally {
    saving.resolve(true);
    await ui.persistInstanceSettings(second.id);
    dispose();
  }
});

test("旧写入失败时继续保存已修正的新值，不显示过时错误", async () => {
  const {
    fixture,
    context: ui,
    second,
    dispose,
  } = await fixtureWithTwoInstances();
  const old = deferred();
  let count = 0;
  fixture.handlers.update = async ({ op, id, ...preferences }) => {
    if (++count === 1) await old.promise;
    Object.assign(second, preferences);
    return true;
  };
  try {
    await ui.edit(second);
    ui.form.launch.jvm_args = ["invalid"];
    await flush();
    ui.form.launch.jvm_args = ["-Dpack=valid"];
    old.reject(new Error("invalid JVM argument"));
    await ui.persistInstanceSettings(second.id);
    assert.equal(count, 2);
    assert.deepEqual(second.launch.jvm_args, ["-Dpack=valid"]);
    assert.equal(ui.visibleError.value, "");
  } finally {
    dispose();
  }
});
