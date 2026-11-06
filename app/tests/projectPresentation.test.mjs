import assert from "node:assert/strict";
import test from "node:test";
import {
  createProjectMedia,
  projectImageUrl,
  projectLink,
} from "../src/projectPresentation.ts";

const artwork = "data:image/png;base64,YQ==";
const tick = () => new Promise((resolve) => setImmediate(resolve));

test("project artwork accepts platform CDNs but rejects local, authenticated and redirected-looking hosts", () => {
  for (const value of [
    "https://cdn.modrinth.com/data/cat/icon.png",
    "https://media.forgecdn.net/avatars/1/2.png",
    "https://mediafilez.forgecdn.net/attachments/1/2.png",
  ])
    assert.equal(projectImageUrl(value), value);
  for (const value of [
    "http://cdn.modrinth.com/icon.png",
    "https://cdn.modrinth.com.evil.test/icon.png",
    "https://cdn.modrinth.com:8443/icon.png",
    "https://user:secret@cdn.modrinth.com/icon.png",
    "http://127.0.0.1/file",
    "file:///etc/passwd",
    "data:image/svg+xml,<svg/>",
  ])
    assert.equal(projectImageUrl(value), undefined);
  assert.equal(
    projectLink("/mod/cat", "https://modrinth.com"),
    "https://modrinth.com/mod/cat",
  );
  for (const value of [
    "javascript:alert(1)",
    "file:///tmp/a",
    "https://a:b@example.com",
    "#anchor",
  ])
    assert.equal(projectLink(value), undefined);
});

test("list and detail artwork share in-flight requests, stay at four concurrent fetches and reuse cache", async () => {
  const calls = [],
    pending = [];
  const media = createProjectMedia((op, data) => {
    calls.push({ op, ...data });
    return new Promise((resolve) => pending.push(resolve));
  });
  const urls = Array.from(
    { length: 6 },
    (_, i) => `https://cdn.modrinth.com/${i}.png`,
  );
  const requests = urls.map(media.image);
  const duplicate = media.image(urls[0]);
  assert.equal(calls.length, 4);
  pending.shift()(artwork);
  await tick();
  assert.equal(calls.length, 5);
  pending.shift()(artwork);
  await tick();
  assert.equal(calls.length, 6);
  for (const resolve of pending) resolve(artwork);
  assert.deepEqual(
    await Promise.all([...requests, duplicate]),
    Array(7).fill(artwork),
  );
  assert.equal(await media.image(urls[0]), artwork);
  assert.equal(calls.length, 6);
});

test("failed media can retry and description caches stay scoped to platform/project", async () => {
  let attempts = 0;
  const media = createProjectMedia(async (op, data) => {
    attempts++;
    if (attempts === 1) throw new Error("offline");
    return op === "resource_image" ? artwork : data;
  });
  const url = "https://cdn.modrinth.com/cat.png";
  await assert.rejects(media.image(url), /offline/);
  assert.equal(await media.image(url), artwork);
  const first = media.details("modrinth", "cat");
  const count = attempts;
  assert.deepEqual(await media.details("modrinth", "cat"), await first);
  assert.equal(attempts, count);
  assert.equal(
    (await media.details("curseforge", "cat")).platform,
    "curseforge",
  );
  assert.equal((await media.details("modrinth", "cat", true)).refresh, true);
  const unsafe = createProjectMedia(
    async () => "data:image/svg+xml;base64,YQ==",
  );
  await assert.rejects(unsafe.image(url), /Invalid project image/);
});

const description = (version = "1", updated = "2026-11-01T00:00:00Z") => ({
  project: { id: "cat", latest_version: version, updated },
  body: `Description ${version}`,
  updated,
  refresh_required: false,
});

test("description reuse detects newer project revisions without rolling back to an older search result", async () => {
  let calls = 0;
  const media = createProjectMedia(async () => {
    calls++;
    return description();
  });
  const known = { updated: "2026-11-02T00:00:00Z", latest_version: "2" };
  assert.equal(
    (await media.details("curseforge", "cat")).refresh_required,
    false,
  );
  assert.equal(
    (await media.details("curseforge", "cat", false, known)).refresh_required,
    true,
  );
  assert.equal(
    (await media.details("curseforge", "cat", false, { latest_version: "2" }))
      .refresh_required,
    true,
  );
  assert.equal(
    (
      await media.details("curseforge", "cat", false, {
        updated: "2026-10-01T00:00:00Z",
        latest_version: "0",
      })
    ).refresh_required,
    false,
  );
  assert.equal(calls, 1);
});

test("expired memory only rereads the disk-backed description; it does not force a network refresh", async () => {
  let time = 0;
  const calls = [];
  const media = createProjectMedia(
    async (_op, data) => {
      calls.push(data);
      return description();
    },
    () => time,
  );
  await media.details("modrinth", "cat");
  time += 11 * 60_000;
  assert.equal(
    (await media.details("modrinth", "cat")).refresh_required,
    false,
  );
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.refresh === false));
});

test("a stale description stays immediately readable during a shared background refresh", async () => {
  let complete,
    calls = 0;
  const old = { ...description(), refresh_required: true };
  const media = createProjectMedia((_op, data) => {
    calls++;
    return data.refresh
      ? new Promise((resolve) => {
          complete = resolve;
        })
      : Promise.resolve(old);
  });
  assert.equal((await media.details("modrinth", "cat")).refresh_required, true);
  const refresh = media.details("modrinth", "cat", true);
  const duplicate = media.details("modrinth", "cat", true);
  const cached = await media.details("modrinth", "cat");
  assert.equal(cached.body, old.body);
  assert.equal(cached.refresh_required, false);
  assert.equal(calls, 2);
  complete(description("2"));
  assert.deepEqual(await duplicate, await refresh);
  assert.equal((await media.details("modrinth", "cat")).body, "Description 2");
});

test("failed background refresh retains the old description and retries only after cooldown", async () => {
  let time = 0;
  const old = { ...description(), refresh_required: true };
  const media = createProjectMedia(
    async (_op, data) => {
      if (data.refresh) throw new Error("offline");
      return old;
    },
    () => time,
  );
  await media.details("modrinth", "cat");
  await assert.rejects(media.details("modrinth", "cat", true), /offline/);
  const cached = await media.details("modrinth", "cat");
  assert.equal(cached.body, old.body);
  assert.equal(cached.refresh_required, false);
  time += 60_001;
  assert.equal((await media.details("modrinth", "cat")).refresh_required, true);
});
