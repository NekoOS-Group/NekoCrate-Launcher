import assert from "node:assert/strict";
import test from "node:test";
import { hexPiePoints, versionDownloadState } from "../src/versionProgress.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const points = (value) =>
  value.split(" ").map((point) => point.split(",").map(Number));
const area = (polygon) =>
  Math.abs(
    polygon.reduce((sum, [x, y], index) => {
      const [nextX, nextY] = polygon[(index + 1) % polygon.length];
      return sum + x * nextY - y * nextX;
    }, 0),
  ) / 2;

test("六边形饼图按面积表达进度，边界值安全且动画顶点数量稳定", () => {
  const full = area(points(hexPiePoints(100)));
  for (const percent of [0, 1, 16.667, 25, 50, 66.667, 99, 100]) {
    const polygon = points(hexPiePoints(percent));
    assert.equal(polygon.length, 8);
    assert.ok(Math.abs(area(polygon) / full - percent / 100) < 0.00001);
  }
  assert.equal(hexPiePoints(-20), hexPiePoints(0));
  assert.equal(hexPiePoints(NaN), hexPiePoints(0));
  assert.equal(hexPiePoints(200), hexPiePoints(100));
});

test("进度只对应同一实例下的版本，未确认安装完成前不会显示满格", () => {
  const version = { id: "1.21.1", installed: false };
  const tasks = [
    {
      instance_id: "other",
      version_id: version.id,
      download_progress: 90,
      status: "running",
    },
    {
      instance_id: "world",
      version_id: "1.20.1",
      download_progress: 70,
      status: "done",
    },
    {
      instance_id: "world",
      version_id: version.id,
      download_progress: 35,
      status: "cancelled",
    },
  ];
  assert.equal(versionDownloadState("world", version, tasks).progress, 35);
  assert.equal(versionDownloadState("world", version, tasks).active, false);
  assert.equal(versionDownloadState("empty", version, tasks).progress, 0);
  assert.equal(
    versionDownloadState("world", { ...version, installed: true }, tasks)
      .progress,
    100,
  );
  tasks.unshift({
    instance_id: "world",
    version_id: version.id,
    download_progress: 100,
    status: "running",
    indeterminate: true,
  });
  assert.deepEqual(versionDownloadState("world", version, tasks), {
    progress: 99,
    active: true,
    indeterminate: true,
  });
});

test("阶段切换和取消保留子实例下载进度，浏览其他实例不会改写任务归属", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const download = deferred();
    fixture.handlers.install = () => download.promise;
    const instance = fixture.state.database.instances[0];
    const installing = ui.install(instance);
    const emit = (kind, completed = 0, total = 0, instance_id = instance.id) =>
      fixture.event({
        payload: {
          kind,
          message: "stage",
          completed,
          total,
          instance_id,
          bytes: 0,
        },
      });
    assert.equal(ui.current.value.version_id, "1.21.1");
    emit("progress", 35, 100);
    ui.selectInstance("another-instance");
    emit("progress", 90, 100, "another-instance");
    assert.equal(ui.current.value.download_progress, 35);
    emit("stage");
    assert.equal(ui.current.value.indeterminate, false);
    assert.equal(ui.current.value.progress, 35);
    assert.equal(ui.current.value.download_progress, 35);
    emit("progress", 20, 100);
    assert.equal(ui.current.value.download_progress, 35);
    download.reject(new Error("用户取消"));
    await installing;
    await flush();
    assert.equal(ui.tasks.value[0].status, "cancelled");
    assert.equal(
      versionDownloadState(instance.id, instance.versions[0], ui.tasks.value)
        .progress,
      35,
    );
  } finally {
    dispose();
  }
});

test("没有内存任务历史时也显示持久化进度，新任务继承进度且按字节更新", async () => {
  assert.equal(
    versionDownloadState(
      "world",
      { id: "1.21.1", installed: false, download_progress: 42 },
      [],
    ).progress,
    42,
  );
  assert.equal(
    versionDownloadState(
      "world",
      { id: "1.21.1", installed: false, download_progress: 42 },
      [
        {
          instance_id: "world",
          version_id: "1.21.1",
          download_progress: 0,
          status: "error",
        },
      ],
    ).progress,
    42,
  );
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.state.database.instances[0].versions[0].download_progress = 42;
    await ui.sync();
    const download = deferred();
    fixture.handlers.install = () => download.promise;
    const instance = ui.snapshot.value.database.instances[0];
    const installing = ui.install(instance);
    assert.equal(ui.current.value.download_progress, 42);
    assert.equal(ui.current.value.progress, 42);
    fixture.event({
      payload: {
        kind: "progress",
        message: "",
        instance_id: instance.id,
        step: "files",
        completed: 0,
        total: 2,
        bytes: 70,
        total_bytes: 100,
      },
    });
    assert.equal(ui.current.value.progress, 70);
    assert.equal(ui.current.value.download_progress, 70);
    assert.equal(
      versionDownloadState(instance.id, instance.versions[0], ui.tasks.value)
        .progress,
      70,
    );
    download.reject(new Error("操作已取消"));
    await installing;
    const retry = deferred();
    fixture.handlers.install = () => retry.promise;
    const retrying = ui.install(instance);
    assert.equal(ui.current.value.download_progress, 70);
    assert.equal(ui.current.value.progress, 70);
    retry.reject(new Error("操作已取消"));
    await retrying;
  } finally {
    dispose();
  }
});

test("磁盘检查发现丢失或损坏时纠正旧进度，后续阶段不再清零", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  const download = deferred();
  try {
    fixture.state.database.instances[0].versions[0].download_progress = 93;
    await ui.sync();
    fixture.handlers.install = () => download.promise;
    const instance = ui.snapshot.value.database.instances[0];
    const installing = ui.install(instance);
    assert.equal(ui.current.value.progress, 93);
    fixture.event({
      payload: {
        kind: "progress",
        message: "",
        instance_id: instance.id,
        step: "files",
        completed: 0,
        total: 10,
        bytes: 60,
        total_bytes: 100,
        reset_progress: true,
      },
    });
    assert.equal(ui.current.value.progress, 60);
    assert.equal(instance.versions[0].download_progress, 60);
    assert.equal(
      versionDownloadState(instance.id, instance.versions[0], ui.tasks.value)
        .progress,
      60,
    );
    fixture.event({
      payload: {
        kind: "stage",
        message: "校验本地文件",
        instance_id: instance.id,
        step: "files",
        completed: 0,
        total: 0,
        bytes: 0,
      },
    });
    assert.equal(ui.current.value.progress, 60);
    fixture.state.database.instances[0].versions[0].download_progress = 60;
    download.reject(new Error("操作已取消"));
    await installing;
    assert.equal(
      versionDownloadState(
        instance.id,
        ui.snapshot.value.database.instances[0].versions[0],
        ui.tasks.value,
      ).progress,
      60,
    );
  } finally {
    dispose();
  }
});
