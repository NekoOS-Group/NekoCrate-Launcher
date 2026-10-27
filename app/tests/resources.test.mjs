import assert from "node:assert/strict";
import test from "node:test";
import { resourceDropTarget, routeResourceDrop } from "../src/resources.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

test("drop routing uses inspected content, including ZIP worlds and ambiguous resource packs", () => {
  assert.equal(
    routeResourceDrop([{ format: "resource", kinds: ["saves"] }]),
    "resources",
  );
  assert.equal(
    routeResourceDrop([
      { format: "resource", kinds: ["datapacks", "resourcepacks"] },
    ]),
    "resources",
  );
  assert.equal(routeResourceDrop([{ format: "modpack" }]), "modpack");
  assert.equal(
    routeResourceDrop([{ format: "directory" }, { format: "directory" }]),
    "folders",
  );
  assert.equal(
    routeResourceDrop([{ format: "resource" }, { format: "directory" }]),
    "mixed",
  );
});

test("a version row overrides the selected-version fallback without accepting stale IDs", () => {
  const instance = { id: "games", versions: [{ id: "a" }, { id: "b" }] };
  const element = {
    closest: () => ({
      dataset: { resourceInstance: "games", resourceVersion: "b" },
    }),
  };
  assert.equal(
    resourceDropTarget(element, [instance], { instance: "games", version: "a" })
      .version,
    "b",
  );
  assert.equal(
    resourceDropTarget(null, [instance], { instance: "games", version: "a" })
      .version,
    "a",
  );
  assert.equal(resourceDropTarget(element, []), undefined);
  assert.equal(
    resourceDropTarget(null, [instance], {
      instance: "games",
      version: "deleted",
    }),
    undefined,
  );
});

test("resource ZIP drop opens the selected profile's resource preview and does not import a modpack", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.inspect_resources = ({ paths }) =>
      paths.map((path) => ({
        path,
        name: "world.zip",
        kinds: ["saves"],
        format: "resource",
      }));
    f.fileDrop({ payload: { type: "drop", paths: ["/world.zip"] } });
    await flush();
    assert.equal(f.context.resourceSession.value.instance.id, "instance-a");
    assert.equal(f.context.resourceSession.value.version, "1.21.1");
    assert.equal(f.context.resourceSession.value.kind, "saves");
    assert.equal(
      f.calls.some((c) =>
        ["scan_folders", "preview_modpack", "import_folders"].includes(c.op),
      ),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("resource installation has independent progress and supports pause without marking Minecraft installed", async () => {
  const f = await settingsFixture();
  const completed = deferred();
  try {
    f.handlers.install_resources = () => completed.promise;
    f.handlers.pause = () => true;
    f.handlers.resume = () => true;
    const plan = {
      id: "resource-task",
      target: { instance: "instance-a", version: "1.21.1" },
      entries: [{ origin: { platform: "modrinth" } }],
    };
    const result = f.context.installResources(plan);
    await flush();
    assert.equal(f.context.current.value.resource_task, "resource-task");
    assert.equal(f.context.current.value.version_id, null);
    assert.equal(
      f.context.current.value.workflow.steps[0].id,
      "resource_check",
    );
    await f.context.setDownloadPaused(true);
    assert.equal(f.context.current.value.status, "paused");
    await f.context.setDownloadPaused(false);
    assert.equal(f.context.current.value.status, "running");
    completed.resolve(true);
    assert.equal(await result, true);
    assert.equal(f.context.instances.value[0].versions[0].installed, false);
    assert.equal(
      f.context.instances.value[0].versions[0].download_progress,
      undefined,
    );
  } finally {
    completed.resolve(true);
    f.dispose();
  }
});

test("invalid or mixed drops report a corner error and never silently associate directories", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.inspect_resources = () => [
      { format: "unknown", issue: "invalid archive" },
    ];
    f.fileDrop({ payload: { type: "drop", paths: ["/broken.zip"] } });
    await flush();
    assert.equal(f.context.error.value, "invalid archive");
    assert.equal(f.context.resourceSession.value, undefined);
    assert.equal(
      f.calls.some((c) => c.op === "scan_folders"),
      false,
    );
  } finally {
    f.dispose();
  }
});
