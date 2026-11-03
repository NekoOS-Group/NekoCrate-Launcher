import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer } from "vue";
import { useCatalogFeed } from "../src/useCatalogFeed.ts";
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
};
function fixture(fetch) {
  let feed;
  const errors = [];
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      feed = useCatalogFeed(fetch, (error) => errors.push(error));
      return () => null;
    },
  });
  app.mount({});
  return { feed, errors, dispose: () => app.unmount() };
}
const page = (start, count = 20, total = 60) => ({
  hits: Array.from({ length: count }, (_, i) => ({ id: String(start + i) })),
  total,
});

test("catalog pages append once, deduplicate overlapping results and retain row identity/scroll", async () => {
  const pending = deferred(),
    calls = [];
  const f = fixture(async (offset) => {
    calls.push(offset);
    return offset ? pending.promise : page(0);
  });
  try {
    await f.feed.search();
    const first = f.feed.hits.value[0],
      revision = f.feed.revision.value;
    f.feed.scrollTop.value = 640;
    const next = f.feed.search(true);
    await f.feed.search(true);
    assert.deepEqual(calls, [0, 20]);
    pending.resolve(page(19));
    await next;
    assert.equal(f.feed.hits.value.length, 39);
    assert.strictEqual(f.feed.hits.value[0], first);
    assert.equal(f.feed.revision.value, revision);
    assert.equal(f.feed.scrollTop.value, 640);
    assert.equal(f.feed.more.value, true);
  } finally {
    f.dispose();
  }
});

test("a failed next page retries the same offset without discarding completed pages", async () => {
  const calls = [];
  const f = fixture(async (offset) => {
    calls.push(offset);
    if (calls.length === 2) throw new Error("offline");
    return page(offset, 20, 40);
  });
  try {
    await f.feed.search();
    await f.feed.search(true);
    assert.equal(f.feed.hits.value.length, 20);
    assert.match(f.feed.failure.value, /offline/);
    await f.feed.retry();
    assert.deepEqual(calls, [0, 20, 20]);
    assert.equal(f.feed.hits.value.length, 40);
    assert.equal(f.feed.more.value, false);
    assert.equal(f.feed.failure.value, "");
    await f.feed.search(true);
    assert.equal(calls.length, 3);
  } finally {
    f.dispose();
  }
});

test("changing filters invalidates late pages, resets the cursor and ignores results after closing", async () => {
  const late = deferred(),
    closed = deferred(),
    calls = [];
  const f = fixture((offset) => {
    calls.push(offset);
    return calls.length === 2
      ? late.promise
      : calls.length === 4
        ? closed.promise
        : Promise.resolve(page(calls.length === 3 ? 100 : 0));
  });
  await f.feed.search();
  const old = f.feed.search(true);
  f.feed.invalidate(true);
  await f.feed.search();
  late.resolve(page(20));
  await old;
  assert.deepEqual(calls, [0, 20, 0]);
  assert.equal(f.feed.hits.value[0].id, "100");
  const pending = f.feed.search(true);
  f.dispose();
  closed.resolve(page(120));
  await pending;
  assert.equal(f.feed.hits.value.length, 20);
});

test("empty and repeated final pages terminate scrolling even if the server total is stale", async () => {
  for (const last of [page(20, 0, 100), page(0, 20, 100)]) {
    const f = fixture(async (offset) => (offset ? last : page(0, 20, 100)));
    try {
      await f.feed.search();
      await f.feed.search(true);
      assert.equal(f.feed.more.value, false);
      assert.equal(f.feed.hits.value.length, 20);
    } finally {
      f.dispose();
    }
  }
});
