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
  assert.strictEqual(media.details("modrinth", "cat"), first);
  assert.notStrictEqual(media.details("curseforge", "cat"), first);
  assert.notStrictEqual(media.details("modrinth", "cat", true), first);
  await first;
  const unsafe = createProjectMedia(
    async () => "data:image/svg+xml;base64,YQ==",
  );
  await assert.rejects(unsafe.image(url), /Invalid project image/);
});
