import assert from "node:assert/strict";
import test from "node:test";
import { settingsFixture, deferred } from "./helpers/appSettingsFixture.mjs";

const candidate = {
  name: "RLCraft",
  path: "/Games/RLCraft/.minecraft",
  source: "portable",
};
function linked(fixture) {
  const instance = {
    ...fixture.state.database.instances[0],
    id: "linked-pack",
    name: candidate.name,
    game_directory: candidate.path,
  };
  fixture.state.database.instances.push(instance);
  return { added: [instance], existing: [], failures: [] };
}

test("opening the picker only discovers directories; stale discovery cannot replace a later visit", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const first = deferred();
    fixture.handlers.suggested_instances = () => first.promise;
    const pending = ui.openInstancePicker();
    assert.equal(ui.modal.value, "add");
    assert.equal(ui.discoveringInstances.value, true);
    assert.equal(fixture.calls.at(-1).op, "suggested_instances");
    ui.newInstance();
    fixture.handlers.suggested_instances = () => [];
    await ui.openInstancePicker();
    first.resolve([candidate]);
    await pending;
    assert.deepEqual(ui.suggestedInstances.value, []);
    assert.equal(ui.discoveringInstances.value, false);
    assert.equal(
      fixture.calls.some(
        ({ op }) => op === "create" || op === "import_folders",
      ),
      false,
    );
    ui.newInstance();
    assert.equal(ui.modal.value, "create");
    assert.equal(ui.form.name, "新实例");
  } finally {
    dispose();
  }
});

test("clicking a suggestion links its original path once, selects it, and uses the two-second notice", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.suggested_instances = () => [candidate];
    const importing = deferred();
    fixture.handlers.import_folders = ({ paths }) => {
      assert.deepEqual(paths, [candidate.path]);
      return importing.promise;
    };
    await ui.openInstancePicker();
    const pending = ui.linkInstanceDirectory(candidate.path);
    await ui.linkInstanceDirectory(candidate.path);
    assert.equal(ui.linkingDirectory.value, candidate.path);
    assert.equal(
      fixture.calls.filter(({ op }) => op === "import_folders").length,
      1,
    );
    importing.resolve(linked(fixture));
    await pending;
    assert.equal(ui.modal.value, null);
    assert.equal(ui.selectedInstanceId.value, "linked-pack");
    assert.deepEqual(ui.folderDrop.state.notice, [
      { name: candidate.name, path: candidate.path },
    ]);
    assert.equal(ui.linkingDirectory.value, "");
    assert.equal(ui.tasks.value.length, 0);
  } finally {
    dispose();
  }
});

test("cancelled or late native folder choices never import; a current choice links directly", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.suggested_instances = () => [];
    fixture.handlers.pick_import_directory = () => null;
    await ui.openInstancePicker();
    await ui.chooseInstanceDirectory();
    assert.equal(ui.modal.value, "add");
    const choosing = deferred();
    fixture.handlers.pick_import_directory = () => choosing.promise;
    const pending = ui.chooseInstanceDirectory();
    ui.modal.value = null;
    await ui.openInstancePicker();
    choosing.resolve(candidate.path);
    await pending;
    assert.equal(
      fixture.calls.some(({ op }) => op === "import_folders"),
      false,
    );
    assert.equal(ui.directoryPicking.value, false);
    fixture.handlers.pick_import_directory = () => candidate.path;
    fixture.handlers.import_folders = () => linked(fixture);
    await ui.chooseInstanceDirectory();
    assert.equal(ui.modal.value, null);
    assert.equal(ui.selectedInstanceId.value, "linked-pack");
  } finally {
    dispose();
  }
});

test("a deleted or inaccessible suggestion keeps the picker open and reports the failure in the corner", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    fixture.handlers.suggested_instances = () => [candidate];
    fixture.handlers.import_folders = () => ({
      added: [],
      existing: [],
      failures: [{ path: candidate.path, message: "Directory disappeared" }],
    });
    await ui.openInstancePicker();
    await ui.linkInstanceDirectory(candidate.path);
    assert.equal(ui.modal.value, "add");
    assert.equal(ui.visibleError.value, "Directory disappeared");
    assert.equal(ui.busy.value, false);
    assert.equal(ui.linkingDirectory.value, "");
  } finally {
    dispose();
  }
});

test("an already linked folder is selected without adding another instance", async () => {
  const { fixture, context: ui, dispose } = await settingsFixture();
  try {
    const existing = fixture.state.database.instances[0];
    fixture.handlers.suggested_instances = () => [];
    fixture.handlers.import_folders = () => ({
      added: [],
      existing: [existing],
      failures: [],
    });
    await ui.openInstancePicker();
    await ui.linkInstanceDirectory(candidate.path);
    assert.equal(ui.selectedInstanceId.value, existing.id);
    assert.equal(fixture.state.database.instances.length, 1);
    assert.equal(ui.modal.value, null);
  } finally {
    dispose();
  }
});
