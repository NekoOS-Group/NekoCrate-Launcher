import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("Java 列表合并探测结果与已有路径，迟到的探测不覆盖用户选择", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const discovery = deferred();
    fixture.handlers.java = () => discovery.promise;
    fixture.handlers.instance_directory = () => "/world/.minecraft";
    const instance = fixture.state.database.instances[0];
    instance.java_path = "/custom/jdk/bin/java";
    await ui.sync();
    await ui.edit(instance);
    assert.equal(ui.javaDiscovering.value, true);
    assert.equal(ui.javaChoices.value[0].value, "");
    assert.ok(
      ui.javaChoices.value.some(
        (choice) => choice.value === instance.java_path,
      ),
    );
    fixture.pickFile = () => "/chosen/jdk/bin/java";
    await ui.chooseJava();
    discovery.resolve([
      { path: "/jdk8/bin/java", major: 8, version: 'java version "1.8.0_422"' },
      {
        path: "/jdk21/bin/java",
        major: 21,
        version: 'openjdk version "21.0.5"',
      },
      {
        path: "/jdk21/bin/java",
        major: 21,
        version: 'openjdk version "21.0.5"',
      },
    ]);
    await flush();
    assert.equal(ui.form.java_path, "/chosen/jdk/bin/java");
    assert.deepEqual(
      ui.javaChoices.value.map((choice) => choice.value),
      [
        "",
        "/jdk21/bin/java",
        "/jdk8/bin/java",
        "/chosen/jdk/bin/java",
        "/custom/jdk/bin/java",
      ],
    );
    fixture.handlers.update = (request) => {
      instance.java_path = request.java_path;
      return true;
    };
    await ui.submit();
    assert.equal(instance.java_path, "/chosen/jdk/bin/java");
    await ui.edit(instance);
    ui.form.java_path = "";
    await ui.submit();
    assert.equal(instance.java_path, null);
  } finally {
    dispose();
  }
});

test("取消文件选择保持原值，旧弹窗的文件选择结果不修改新实例", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.java = () => [];
    fixture.handlers.instance_directory = () => "/world/.minecraft";
    const instance = fixture.state.database.instances[0];
    instance.java_path = "/existing/bin/java";
    await ui.edit(instance);
    fixture.pickFile = () => null;
    await ui.chooseJava();
    assert.equal(ui.form.java_path, instance.java_path);
    const picker = deferred();
    fixture.pickFile = (options) => {
      assert.equal(options.directory, false);
      assert.equal(options.multiple, false);
      return picker.promise;
    };
    const choosing = ui.chooseJava();
    ui.modal.value = null;
    await ui.edit({
      ...instance,
      id: "instance-b",
      java_path: "/other/bin/java",
    });
    picker.resolve("/too-late/bin/java");
    await choosing;
    assert.equal(ui.form.java_path, "/other/bin/java");
    assert.equal(ui.javaPicking.value, false);
  } finally {
    dispose();
  }
});

test("Java 探测失败时保留已配置路径，仍可自动匹配或选择文件", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.java = () => {
      throw new Error("discovery failed");
    };
    fixture.handlers.instance_directory = () => "/world/.minecraft";
    const instance = fixture.state.database.instances[0];
    instance.java_path = "/existing/bin/java";
    await ui.edit(instance);
    await flush();
    assert.equal(ui.javaDiscovering.value, false);
    assert.equal(ui.javaDiscoveryError.value, "discovery failed");
    assert.equal(ui.form.java_path, instance.java_path);
    assert.ok(ui.javaChoices.value.some((choice) => choice.value === ""));
    fixture.pickFile = () => "/chosen/bin/java";
    await ui.chooseJava();
    assert.equal(ui.form.java_path, "/chosen/bin/java");
  } finally {
    dispose();
  }
});
