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

test("下载中实例打开任务列表并持续更新进度，其他实例仍能修改名称、图标、内存和 Java 后保存", async () => {
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
    assert.equal(ui.utility.value, "downloads");
    assert.equal(ui.modal.value, null);
    assert.equal(ui.tasks.value[0], active);
    await ui.openInstance(first);
    assert.equal(ui.utility.value, "downloads", "再次打开不应切换为关闭");
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
    await ui.submit();
    assert.equal(ui.modal.value, null);
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
    ui.form.name = "Unsaved draft";
    download.resolve(true);
    await installing;
    assert.equal(ui.modal.value, "edit", "下载完成不应关闭其他实例设置");
    assert.equal(ui.form.name, "Unsaved draft");
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
    await ui.submit();
    assert.equal(
      fixture.calls.filter((call) => call.op === "update").length,
      1,
    );
    saving.reject(new Error("write failed"));
    await submission;
    assert.equal(ui.error.value, "write failed");
    assert.equal(ui.modal.value, "edit");
    assert.equal(ui.instanceSaving.value, false);
    assert.equal(ui.form.memory_mb, 8192);
    assert.equal(ui.busy.value, true);
    assert.equal(ui.current.value.status, "running");
    fixture.handlers.update = () => true;
    await ui.submit();
    assert.equal(
      fixture.calls.filter((call) => call.op === "update").length,
      2,
    );
    assert.equal(ui.modal.value, null);
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
