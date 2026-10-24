import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("creation defaults to managed storage and never reuses an external path after switching back", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.state.database.instances[0].name = "新实例";
    fixture.state.database.instances.push({
      ...fixture.state.database.instances[0],
      id: "second",
      name: "新实例 2",
    });
    await ui.sync();
    ui.newInstance();
    assert.equal(ui.form.name, "新实例 3");
    assert.equal(ui.form.location, "managed");
    assert.equal(ui.form.external_directory, "");
    ui.form.name = "World";
    ui.form.location = "external";
    assert.equal(ui.invalidCreationLocation.value, true);
    await ui.submit();
    assert.equal(
      fixture.calls.some(({ op }) => op === "create"),
      false,
    );
    fixture.handlers.pick_instance_location = () => "/Games/World";
    await ui.chooseCreationDirectory();
    assert.equal(ui.form.external_directory, "/Games/World");
    ui.form.location = "managed";
    fixture.handlers.create = ({ name, directory }) => {
      assert.equal(directory, null);
      return { ...fixture.state.database.instances[0], id: "managed", name };
    };
    await ui.submit();
    assert.equal(ui.selectedInstanceId.value, "managed");
    assert.equal(ui.modal.value, null);
    ui.newInstance();
    assert.equal(ui.form.external_directory, "");
    assert.equal(ui.form.location, "managed");
  } finally {
    dispose();
  }
});

test("external creation passes the selected location, while cancellation and old picker results do not change the form", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    ui.newInstance();
    ui.form.location = "external";
    fixture.handlers.pick_instance_location = () => null;
    await ui.chooseCreationDirectory();
    assert.equal(ui.form.external_directory, "");
    const picker = deferred();
    fixture.handlers.pick_instance_location = () => picker.promise;
    const selecting = ui.chooseCreationDirectory();
    ui.modal.value = null;
    ui.newInstance();
    ui.form.location = "external";
    picker.resolve("/old/location");
    await selecting;
    assert.equal(ui.form.external_directory, "");
    fixture.handlers.pick_instance_location = () => "/Games/My World";
    await ui.chooseCreationDirectory();
    ui.form.name = "My World";
    fixture.handlers.create = ({ name, directory }) => {
      assert.equal(name, "My World");
      assert.equal(directory, "/Games/My World");
      return {
        ...fixture.state.database.instances[0],
        id: "external",
        name,
        game_directory: `${directory}/.minecraft`,
      };
    };
    await ui.submit();
    assert.equal(ui.modal.value, null);
    assert.equal(ui.selectedInstanceId.value, "external");
  } finally {
    dispose();
  }
});

test("dropping onto the add-directory picker hands off to import without leaving an obsolete dialog open", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.suggested_instances = () => [];
    fixture.handlers.scan_folders = () => ({
      candidates: [],
      issues: [],
      skipped: 0,
    });
    await ui.openInstancePicker();
    assert.equal(ui.modalTitles.add, "添加游戏目录");
    fixture.fileDrop({ payload: { type: "drop", paths: ["/Games"] } });
    await flush();
    assert.equal(ui.modal.value, null);
    assert.equal(ui.folderDrop.state.phase, null);
    assert.deepEqual(ui.folderDrop.state.notice, []);
    assert.equal(
      fixture.calls.some(({ op }) => op === "create"),
      false,
    );
  } finally {
    dispose();
  }
});
