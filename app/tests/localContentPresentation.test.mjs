import assert from "node:assert/strict";
import test from "node:test";
import {
  createLocalContentMedia,
  declaredProject,
} from "../src/localContentPresentation.ts";

const scope = { id: "game", version: "pack", kind: "mods" };
const file = (name = "cat.jar") => ({
  file_name: name,
  name,
  size: 100,
  enabled: true,
});
const icon = "data:image/png;base64,YQ==";
const tick = () => new Promise((resolve) => setImmediate(resolve));

test("online icon fallback uses summaries, shares projects and caps requests separately from local reads", async () => {
  const calls = [],
    pending = [];
  const media = createLocalContentMedia((op, data) => {
    calls.push({ op, data });
    return new Promise((resolve) => pending.push(resolve));
  });
  const projects = Array.from({ length: 5 }, (_, i) => ({
    platform: "modrinth",
    id: String(i),
  }));
  const requests = projects.map(media.onlineIcon);
  const duplicate = media.onlineIcon(projects[0]);
  assert.equal(calls.length, 4);
  assert.ok(calls.every((call) => call.op === "resource_project_summary"));
  pending.shift()({ icon_url: "https://cdn.modrinth.com/cat.png" });
  await tick();
  assert.equal(calls.length, 5);
  for (const resolve of pending) resolve({ icon_url: null });
  await Promise.all([...requests, duplicate]);
  assert.equal(
    await media.onlineIcon(projects[0]),
    "https://cdn.modrinth.com/cat.png",
  );
  assert.equal(calls.length, 5);
});

test("local artwork shares pending reads, bounds concurrency and reuses results", async () => {
  const calls = [],
    pending = [];
  const media = createLocalContentMedia((op, data) => {
    calls.push({ op, data });
    return new Promise((resolve) => pending.push(resolve));
  });
  const requests = Array.from({ length: 6 }, (_, i) =>
    media.load(scope, file(`${i}.jar`)),
  );
  const duplicate = media.load(scope, file("0.jar"));
  assert.equal(calls.length, 4);
  pending.shift()({ icon });
  await tick();
  assert.equal(calls.length, 5);
  pending.shift()({ icon });
  await tick();
  assert.equal(calls.length, 6);
  for (const resolve of pending) resolve({ icon });
  assert.equal((await Promise.all([...requests, duplicate])).length, 7);
  assert.equal((await media.load(scope, file("0.jar"))).icon, icon);
  assert.equal(calls.length, 6);
});

test("pending declarations use their exact project without reading a missing file", async () => {
  const media = createLocalContentMedia(() => {
    throw new Error("must not read disk");
  });
  const entry = {
    ...file(),
    declaration: { source: "CurseForge", project: 123, status: "pending" },
  };
  assert.deepEqual(await media.load(scope, entry), {
    project: { platform: "curseforge", id: "123" },
  });
  assert.equal(
    declaredProject({
      ...entry,
      declaration: { source: "Modrinth", project: 123 },
    }),
    undefined,
  );
});

test("icons reject executable/oversized data, failed reads retry, and enabled/disabled twins stay distinct", async () => {
  let fail = true,
    calls = 0;
  const media = createLocalContentMedia(async (_op, data) => {
    calls++;
    if (fail) {
      fail = false;
      throw new Error("deleted");
    }
    return {
      icon: data.file.endsWith("disabled") ? icon : "data:image/svg+xml,<svg/>",
    };
  });
  await assert.rejects(media.load(scope, file()), /deleted/);
  assert.equal((await media.load(scope, file())).icon, null);
  assert.equal((await media.load(scope, file("cat.jar.disabled"))).icon, icon);
  assert.equal(calls, 3);
  const oversized = createLocalContentMedia(async () => ({
    icon: "data:image/png;base64," + "A".repeat(720_000),
  }));
  assert.equal((await oversized.load(scope, file())).icon, null);
});
