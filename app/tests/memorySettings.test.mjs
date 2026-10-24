import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  flush,
  deferred,
} from "./helpers/appSettingsFixture.mjs";

test("memory defaults to auto, previews each preset, and remembers custom values", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.java = () => [];
    fixture.handlers.instance_directory = () => "/game/.minecraft";
    const instance = fixture.state.database.instances[0];
    instance.memory_mb = 5120;
    await ui.edit(instance);
    await flush();
    assert.equal(ui.form.memory_mode, "auto");
    assert.equal(
      ui.memoryChoices.value.find((c) => c.value === "auto").label,
      "自动",
    );
    ui.form.memory_mode = "recommended";
    assert.equal(
      ui.memoryChoices.value.find((c) => c.value === "recommended").label,
      "推荐 (4096)",
    );
    ui.form.memory_mode = "minimum";
    assert.equal(
      ui.memoryChoices.value.find((c) => c.value === "minimum").label,
      "最小 (2048)",
    );
    ui.form.memory_mode = "custom";
    assert.equal(ui.form.memory_mb, 5120);
    ui.form.memory_mb = 7168;
    ui.form.memory_mode = "auto";
    fixture.handlers.update = (request) => {
      instance.memory_mb = request.memory_mb;
      instance.memory_mode = request.memory_mode;
      return true;
    };
    await ui.submit();
    assert.equal(instance.memory_mode, "auto");
    assert.equal(instance.memory_mb, 7168);
    await ui.edit(instance);
    ui.form.memory_mode = "custom";
    assert.equal(ui.form.memory_mb, 7168);
    ui.form.memory_mb = 0;
    assert.equal(ui.invalidCustomMemory.value, true);
    const writes = fixture.calls.filter((c) => c.op === "update").length;
    await ui.submit();
    assert.equal(fixture.calls.filter((c) => c.op === "update").length, writes);
    ui.form.memory_mb = 6400;
    await ui.submit();
    assert.equal(instance.memory_mode, "custom");
    assert.equal(instance.memory_mb, 6400);
    await ui.edit(instance);
    ui.form.memory_mb = "";
    ui.form.memory_mode = "auto";
    await ui.submit();
    assert.equal(instance.memory_mode, "auto");
    assert.equal(
      instance.memory_mb,
      6400,
      "unfinished custom input does not prevent saving Auto",
    );
  } finally {
    dispose();
  }
});

test("late memory previews cannot replace another instance or reopened settings", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.java = () => [];
    fixture.handlers.instance_directory = () => "/game/.minecraft";
    const slow = deferred();
    fixture.handlers.memory_options = () => slow.promise;
    const first = fixture.state.database.instances[0];
    await ui.edit(first);
    fixture.handlers.memory_options = () => ({
      auto_mb: 8000,
      recommended_mb: 6144,
      minimum_mb: 3072,
    });
    await ui.edit({ ...first, id: "second", memory_mode: "recommended" });
    await flush();
    slow.resolve({ auto_mb: 1000, recommended_mb: 1024, minimum_mb: 512 });
    await flush();
    assert.equal(
      ui.memoryChoices.value.find((c) => c.value === "recommended").label,
      "推荐 (6144)",
    );
    assert.equal(ui.form.memory_mode, "recommended");
    fixture.handlers.memory_options = () => {
      throw new Error("no memory preview");
    };
    await ui.edit(first);
    await flush();
    assert.equal(ui.memoryOptions.value, null);
    ui.form.memory_mode = "custom";
    assert.equal(ui.form.memory_mb, first.memory_mb);
  } finally {
    dispose();
  }
});
