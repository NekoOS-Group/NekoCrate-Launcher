import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const archive = {
  path: "/cache/cats.mrpack",
  preview: {
    pack: {
      name: "Cats",
      format: "modrinth",
      minecraft: "1.21.1",
      version: "1",
      loader: null,
      files: [],
    },
  },
};

test("opening the list directory uses its captured instance and version without revealing a file", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.open_content_folder = (request) => {
      assert.equal(request.id, "instance-a");
      assert.equal(request.version, "1.21.1");
      assert.equal(request.kind, "mods");
      assert.equal(request.file, undefined);
      return true;
    };
    await f.context.openContentFolder("instance-a", "1.21.1", "mods");
    assert.equal(
      f.calls.filter((c) => c.op === "open_content_folder").length,
      1,
    );
  } finally {
    f.dispose();
  }
});

test("add content keeps the selected resource version local to the dialog and clears it on target changes", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.instances[0].versions.push({
      id: "other",
      name: "Other",
      installed: true,
    });
    await f.context.sync();
    f.context.newVersion(f.context.instances.value[0]);
    await flush();
    f.context.additionSection.value = "resources";
    assert.equal(f.context.additionResourceSession.value.version, "1.21.1");
    f.context.additionVersion.value = "other";
    assert.equal(f.context.additionResourceSession.value.version, "other");
    assert.equal(f.context.instances.value[0].selected_version, "1.21.1");
    f.context.switchDownloadTarget(null);
    await flush();
    assert.equal(f.context.additionResourceSession.value, undefined);
    f.context.switchDownloadTarget("instance-a");
    await flush();
    assert.equal(f.context.additionResourceSession.value.version, "1.21.1");
  } finally {
    f.dispose();
  }
});

test("online archive download is pausable and opens a review without creating or installing a version", async () => {
  const f = await settingsFixture();
  const waiting = deferred();
  try {
    f.context.newVersion(f.context.instances.value[0]);
    await flush();
    f.context.additionSection.value = "modpacks";
    f.handlers.download_modpack = (request) => {
      assert.equal(request.platform, "modrinth");
      return waiting.promise;
    };
    f.handlers.pause = () => true;
    f.handlers.resume = () => true;
    const result = f.context.downloadOnlineModpack("modrinth", "cats", "one");
    await flush();
    assert.equal(f.context.current.value.version_id, null);
    assert.equal(f.context.current.value.cancellable, true);
    assert.equal(
      f.context.modpackDownloadTask.value.workflow.steps[0].id,
      "pack_archive_resolve",
    );
    await f.context.setDownloadPaused(true);
    assert.equal(f.context.current.value.status, "paused");
    await f.context.setDownloadPaused(false);
    waiting.resolve(archive);
    assert.equal(await result, true);
    assert.equal(f.context.packPreview.value.pack.name, "Cats");
    assert.equal(f.context.packPath.value, archive.path);
    assert.equal(f.context.onlinePack.value, true);
    assert.equal(f.context.editing.value.id, "instance-a");
    assert.equal(
      f.calls.some((c) =>
        ["create", "prepare_modpack", "install"].includes(c.op),
      ),
      false,
    );
    assert.equal(f.context.instances.value[0].versions[0].installed, false);
  } finally {
    waiting.resolve(archive);
    f.dispose();
  }
});

test("a completed archive request does not reopen a dismissed add window", async () => {
  const f = await settingsFixture();
  const waiting = deferred();
  try {
    f.context.newVersion(f.context.instances.value[0]);
    await flush();
    f.handlers.download_modpack = () => waiting.promise;
    const result = f.context.downloadOnlineModpack("modrinth", "cats", "one");
    await flush();
    f.context.modal.value = null;
    waiting.resolve(archive);
    assert.equal(await result, false);
    assert.equal(f.context.modal.value, null);
    assert.equal(f.context.packPreview.value, null);
  } finally {
    waiting.resolve(archive);
    f.dispose();
  }
});

test("online pack review preserves the inline external-directory draft", async () => {
  const f = await settingsFixture();
  try {
    f.context.newVersion(f.context.instances.value[0]);
    await flush();
    f.context.switchDownloadTarget(null);
    f.context.form.name = "My packs";
    f.context.form.location = "external";
    f.context.form.external_directory = "/Games/Cats";
    f.handlers.download_modpack = () => archive;
    assert.equal(
      await f.context.downloadOnlineModpack("modrinth", "cats", "one"),
      true,
    );
    assert.equal(f.context.editing.value, null);
    assert.equal(f.context.form.name, "My packs");
    assert.equal(f.context.form.external_directory, "/Games/Cats");
    f.handlers.preview_modpack = () => archive.preview;
    await f.context.openModpack("/local.mrpack");
    assert.equal(f.context.onlinePack.value, false);
    assert.equal(f.context.form.external_directory, "/Games/Cats");
  } finally {
    f.dispose();
  }
});
