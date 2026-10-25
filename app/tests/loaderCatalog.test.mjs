import assert from "node:assert/strict";
import test from "node:test";
import { createLoaderCatalog } from "../src/loaderCatalog.ts";

test("loader catalogs coalesce requests, preserve choices and separate game/loader keys", async () => {
  let clock = 0,
    calls = 0,
    resolve;
  const catalog = createLoaderCatalog(
    async () => {
      calls++;
      return new Promise((done) => {
        resolve = done;
      });
    },
    () => clock,
  );
  const first = catalog.load("1.20.1", "forge");
  const same = catalog.load("1.20.1", "forge");
  assert.equal(first, same);
  resolve([
    { version: "47.4.28", stable: false },
    { version: "47.4.4", recommended: true },
  ]);
  const versions = await first;
  assert.equal(catalog.selected("1.20.1", "forge", versions), "47.4.4");
  catalog.remember("1.20.1", "forge", "47.4.28");
  assert.equal(catalog.selected("1.20.1", "forge", versions), "47.4.28");
  assert.equal(await catalog.load("1.20.1", "forge"), versions);
  assert.equal(calls, 1);
  assert.equal(catalog.peek("1.21.1", "forge"), undefined);
  assert.equal(catalog.peek("1.20.1", "fabric"), undefined);
  clock = 3_600_001;
  const refreshing = catalog.load("1.20.1", "forge");
  assert.equal(
    catalog.peek("1.20.1", "forge").versions,
    versions,
    "old options remain available during refresh",
  );
  resolve([{ version: "47.4.29", recommended: true }]);
  assert.equal(
    catalog.selected("1.20.1", "forge", await refreshing),
    "47.4.29",
  );
  assert.equal(calls, 2);
});

test("empty availability is cached; network failure never overwrites an existing catalog", async () => {
  let fail = false,
    calls = 0;
  const catalog = createLoaderCatalog(async () => {
    calls++;
    if (fail) throw new Error("offline");
    return [];
  });
  await catalog.load("b1.8.1", "fabric");
  await catalog.load("b1.8.1", "fabric");
  assert.equal(calls, 1);
  fail = true;
  await assert.rejects(catalog.load("b1.8.1", "fabric", true), /offline/);
  assert.deepEqual(catalog.peek("b1.8.1", "fabric").versions, []);
  await assert.rejects(catalog.load("1.21.1", "fabric"), /offline/);
  assert.equal(catalog.peek("1.21.1", "fabric"), undefined);
});
