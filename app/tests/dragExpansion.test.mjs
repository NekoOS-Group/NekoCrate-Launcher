import assert from "node:assert/strict";
import test from "node:test";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";
import { projectDropUrl } from "../src/projectDrop.ts";
import { configDifference } from "../src/configDrop.ts";
import { resourceCopyDestination } from "../src/useResourceTransfer.ts";
test("project drop only accepts canonical Minecraft project URLs, including deep links", () => {
  for (const link of [
    "https://modrinth.com/mod/sodium",
    "https://www.curseforge.com/minecraft/modpacks/atm9/files/123",
    "https://modrinth.com/shader/test/version/a",
  ])
    assert.ok(projectDropUrl(link));
  for (const link of [
    "https://modrinth.com.evil.test/mod/sodium",
    "file:///tmp/key",
    "https://x:y@modrinth.com/mod/test",
    "https://curseforge.com/wow/addons/test",
    "https://modrinth.com/mod/%2e%2e",
    "https://modrinth.com:9999/mod/test",
  ])
    assert.equal(projectDropUrl(link), undefined);
});
test("configuration preview isolates changes and bounds huge previews without editing input", () => {
  const before = "first\nsecond\nthird";
  const after = "first\nchanged\nthird";
  assert.deepEqual(configDifference(before, after), {
    same: false,
    before,
    after,
    truncated: false,
  });
  assert.equal(configDifference(before, before).same, true);
  const large = "new\n".repeat(1000);
  const diff = configDifference("", large);
  assert.equal(diff.truncated, true);
  assert.equal(diff.after.split("\n").length, 100);
  assert.equal(large.length, 4000);
});
test("datapacks may move between worlds in one version; ordinary version targets require a world", () => {
  const source = {
    kind: "datapacks",
    instance: "a",
    version: "one",
    file: "First/a.zip",
    name: "a",
  };
  const instance = { id: "a", versions: [{ id: "one", installed: true }] };
  const element = (world) => ({
    closest: (selector) =>
      selector === "[data-drop-world]"
        ? world
          ? { dataset: { dropWorld: world } }
          : null
        : {
            dataset: {
              dropKind: "datapacks",
              resourceInstance: "a",
              resourceVersion: "one",
            },
          },
  });
  assert.equal(
    resourceCopyDestination(element(), source, [instance], []),
    undefined,
  );
  assert.deepEqual(
    resourceCopyDestination(element("Second"), source, [instance], []),
    { instance: "a", version: "one", world: "Second" },
  );
  assert.equal(
    resourceCopyDestination(element("Second"), source, [instance], ["a"]),
    undefined,
  );
});
test("export uses explicit portable platform and does not mutate the instance; active games are blocked", async () => {
  const f = await settingsFixture();
  try {
    const ui = f.context;
    const instance = ui.instances.value[0];
    f.handlers.export_game = () => ({
      id: "export-token",
      name: "Games.zip",
      size: 1024,
    });
    ui.openGameExport(instance.id);
    assert.equal(ui.exportSession.value.instance.id, instance.id);
    const result = await ui.buildGameExport("windows");
    assert.equal(result.name, "Games.zip");
    const request = f.calls.find((c) => c.op === "export_game");
    assert.equal(request.platform, "windows");
    assert.equal(request.id, instance.id);
    assert.equal(ui.tasks.value[0].status, "done");
    assert.equal(
      f.calls.some((c) =>
        ["delete", "create", "delete_version"].includes(c.op),
      ),
      false,
    );
    ui.exportSession.value = undefined;
    ui.snapshot.value.running = [instance.id];
    ui.openGameExport(instance.id);
    assert.equal(ui.exportSession.value, undefined);
  } finally {
    f.dispose();
  }
});
test("mixed resources remain one import session and a world drop carries the exact destination", async () => {
  const f = await settingsFixture();
  try {
    const ui = f.context,
      instance = ui.instances.value[0];
    const files = [
      {
        path: "/a.jar",
        name: "a.jar",
        format: "resource",
        kinds: ["mods"],
        issue: null,
      },
      {
        path: "/r.zip",
        name: "r.zip",
        format: "resource",
        kinds: ["resourcepacks"],
        issue: null,
      },
    ];
    await ui.handleResourceDrop(
      files.map((f) => f.path),
      { instance, version: "1.21.1" },
      files,
      "version",
    );
    assert.equal(ui.resourceSession.value.files.length, 2);
    ui.resourceSession.value = undefined;
    const pack = {
      path: "/data.zip",
      name: "data.zip",
      format: "resource",
      kinds: ["datapacks"],
      issue: null,
    };
    const surface = { closest: () => ({ dataset: { dropWorld: "Second" } }) };
    await ui.handleResourceDrop(
      [pack.path],
      { instance, version: "1.21.1" },
      [pack],
      "datapacks",
      surface,
    );
    assert.equal(ui.resourceSession.value.world, "Second");
  } finally {
    f.dispose();
  }
});
test("a project link opens compatible resources directly in the selected target", async () => {
  const f = await settingsFixture();
  try {
    const ui = f.context;
    ui.instances.value[0].versions[0].installed = true;
    f.handlers.resolve_project_link = () => ({
      platform: "modrinth",
      kind: "mods",
      project: {
        id: "sodium",
        name: "Sodium",
        description: "",
        url: "https://modrinth.com/mod/sodium",
      },
    });
    await ui.receiveProjectLink({
      target: null,
      dataTransfer: {
        getData: (type) =>
          type === "text/uri-list" ? "https://modrinth.com/mod/sodium" : "",
      },
      preventDefault() {},
      stopPropagation() {},
    });
    await flush();
    assert.equal(ui.resourceSession.value.project.id, "sodium");
    assert.equal(ui.resourceSession.value.version, "1.21.1");
    assert.equal(
      f.calls.some((c) => c.op === "install_resources"),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("arrival identities distinguish identical datapack names in different worlds", async () => {
  const { arrivalFileKey, announceResources, resourceArrival } =
    await import("../src/resourceArrival.ts");
  announceResources("a", "one", "datapacks", [
    "saves/Second/datapacks/Pack.zip",
  ]);
  assert.equal(
    resourceArrival.value.files.includes(
      arrivalFileKey("datapacks", "saves/First/datapacks/Pack.zip"),
    ),
    false,
  );
  assert.equal(
    resourceArrival.value.files.includes(
      arrivalFileKey("datapacks", "saves/Second/datapacks/Pack.zip"),
    ),
    true,
  );
  assert.equal(arrivalFileKey("mods", "cat.jar"), "mods/cat.jar");
  resourceArrival.value = undefined;
});
