import assert from "node:assert/strict";
import test from "node:test";
import { createAccountRefresher } from "../src/accountRefresh.ts";

const EPOCH = 1_800_000_000_000;
const account = (id, remaining) => ({
  id,
  expires_at: (EPOCH + remaining) / 1000,
});

test("local checks skip valid logins and coalesce refreshes within 60 seconds of expiry", async () => {
  let now = 0,
    selected = account("a", 3_600_000),
    finish;
  const calls = [];
  const refresher = createAccountRefresher({
    now: () => now,
    wallNow: () => EPOCH + now,
    selected: () => selected,
    enabled: () => true,
    refresh: (id) => {
      calls.push(id);
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
    failed: () => assert.fail("unexpected error"),
  });
  for (now = 0; now < 3_540_000; now += 60_000) await refresher.check();
  now = 3_539_999;
  await refresher.check();
  assert.deepEqual(calls, []);
  now++;
  const first = refresher.check();
  assert.equal(refresher.check(), first);
  await Promise.resolve();
  assert.deepEqual(calls, ["a"]);
  selected = account("a", now + 3_600_000);
  refresher.fresh("a");
  finish(true);
  await first;
  for (let i = 0; i < 30; i++) {
    now += 60_000;
    await refresher.check();
  }
  assert.deepEqual(calls, ["a"]);
  refresher.dispose();
});

test("expired accounts refresh independently; guests and hidden/offline windows skip requests", async () => {
  let now = 0,
    selected = account("a", 0),
    enabled = false;
  const calls = [];
  const refresher = createAccountRefresher({
    now: () => now,
    wallNow: () => EPOCH + now,
    selected: () => selected,
    enabled: () => enabled,
    refresh: async (id) => {
      calls.push(id);
      return true;
    },
    failed: () => assert.fail("unexpected error"),
  });
  await refresher.check();
  enabled = true;
  selected = null;
  await refresher.check();
  assert.deepEqual(calls, []);
  selected = account("a", 0);
  await refresher.check();
  selected = account("b", 3_600_000);
  await refresher.check();
  selected = account("c", -60_000);
  await refresher.check();
  selected = account("a", 0);
  await refresher.check();
  assert.deepEqual(calls, ["a", "c"]);
  now = 60_000;
  await refresher.check();
  assert.deepEqual(calls, ["a", "c", "a"]);
  refresher.dispose();
});

test("failed refresh backs off; renewed expiry and disposal stop requests", async () => {
  let now = 0,
    selected = account("a", 0),
    count = 0;
  const errors = [];
  const refresher = createAccountRefresher({
    now: () => now,
    wallNow: () => EPOCH + now,
    selected: () => selected,
    enabled: () => true,
    refresh: async () => {
      count++;
      throw new Error("offline");
    },
    failed: (error) => errors.push(error.message),
  });
  await refresher.check();
  now = 59_999;
  await refresher.check();
  assert.equal(count, 1);
  now = 60_000;
  await refresher.check();
  assert.equal(count, 2);
  assert.deepEqual(errors, ["offline", "offline"]);
  selected = account("a", now + 3_600_000);
  refresher.fresh("a");
  now += 60_000;
  await refresher.check();
  assert.equal(count, 2);
  refresher.dispose();
  now += 3_600_000;
  await refresher.check();
  assert.equal(count, 2);
});

test("a busy account is retried later instead of treated as a successful refresh", async () => {
  let now = 0,
    calls = 0;
  const refresher = createAccountRefresher({
    now: () => now,
    wallNow: () => EPOCH + now,
    selected: () => account("a", 0),
    enabled: () => true,
    refresh: async () => {
      calls++;
      return false;
    },
    failed: () => assert.fail("unexpected error"),
  });
  await refresher.check();
  now = 14_999;
  await refresher.check();
  assert.equal(calls, 1);
  now++;
  await refresher.check();
  assert.equal(calls, 2);
  refresher.dispose();
});

test("clock rollback cannot extend an observed lifetime; sleep or clock advance triggers expiry", async () => {
  for (const rollback of [false, true]) {
    let now = 0,
      wall = EPOCH,
      calls = 0;
    const refresher = createAccountRefresher({
      now: () => now,
      wallNow: () => wall,
      selected: () => account("a", 120_000),
      enabled: () => true,
      refresh: async () => {
        calls++;
        return true;
      },
      failed: () => assert.fail("unexpected error"),
    });
    await refresher.check();
    assert.equal(calls, 0);
    now = rollback ? 60_000 : 1_000;
    wall += rollback ? -3_600_000 : 3_600_000;
    await refresher.check();
    assert.equal(calls, 1);
    refresher.dispose();
  }
});

test("a selection change before a queued check does not refresh the previous account", async () => {
  let selected = account("a", 0);
  const refresher = createAccountRefresher({
    wallNow: () => EPOCH,
    selected: () => selected,
    enabled: () => true,
    refresh: async () => assert.fail("previous identity must not be refreshed"),
    failed: () => assert.fail("unexpected error"),
  });
  const checking = refresher.check();
  selected = account("b", 3_600_000);
  await checking;
  refresher.dispose();
});
