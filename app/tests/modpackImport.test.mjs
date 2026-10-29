import assert from "node:assert/strict";
import test from "node:test";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";
const preview = {
  pack: {
    name: "Pack",
    format: "modrinth",
    minecraft: "1.20.1",
    version: "1",
    loader: { kind: "fabric", version: "0.16.10" },
    files: [],
  },
};
test("archive drop opens pack preview instead of recursively importing directories", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.preview_modpack = () => preview;
    f.fileDrop({ payload: { type: "drop", paths: ["/packs/test.mrpack"] } });
    await flush();
    await flush();
    assert.equal(f.context.modal.value, "version");
    assert.equal(f.context.packPreview.value.pack.name, "Pack");
    assert.equal(f.calls.filter((r) => r.op === "scan_folders").length, 0);
  } finally {
    f.dispose();
  }
});
test("CurseForge import installs immediately without a user credential gate", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.preview_modpack = () => ({
      ...preview,
      pack: { ...preview.pack, format: "curseforge" },
    });
    f.handlers.prepare_modpack = (request) => {
      assert.equal(request.id, "instance-a");
      assert.deepEqual(request.optional, []);
      const instance = f.state.database.instances[0];
      instance.versions.push({
        id: "pack-test",
        modpack: true,
        installed: false,
      });
      instance.selected_version = "pack-test";
      return structuredClone(instance);
    };
    await f.context.openModpack("/packs/test.zip");
    await f.context.submitModpack("My pack", []);
    assert.equal(f.context.modal.value, null);
    assert.equal(f.calls.filter((r) => r.op === "install").length, 1);
    assert.equal(f.state.database.instances[0].selected_version, "pack-test");
  } finally {
    f.dispose();
  }
});
test("choosing a directory directly preserves the managed location on cancel", async () => {
  const f = await settingsFixture();
  try {
    f.context.newVersion(f.context.instances.value[0]);
    await flush();
    f.context.switchDownloadTarget(null);
    f.handlers.pick_instance_location = () => null;
    await f.context.chooseCreationDirectory(true);
    assert.equal(f.context.form.location, "managed");
    f.handlers.pick_instance_location = () => "/Games/New pack";
    await f.context.chooseCreationDirectory(true);
    assert.equal(f.context.form.location, "external");
    assert.equal(f.context.form.external_directory, "/Games/New pack");
  } finally {
    f.dispose();
  }
});
