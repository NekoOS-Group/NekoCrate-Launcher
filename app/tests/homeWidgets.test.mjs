import assert from "node:assert/strict";
import test from "node:test";
import {
  readHomeWidgets,
  readQuoteStyle,
  readQuoteAngles,
  readQuoteScales,
  readNewsLayout,
  HITOKOTO_URL,
  readFeedSources,
  presetFeeds,
  safeFeedUrl,
  parseQuote,
  mergeNews,
  freshWidgetCache,
  WIDGET_CACHE_KEY,
} from "../src/homeWidgets.ts";
import { cachedWidget, fetchWidget } from "../src/widgetClient.ts";

test("optional widgets and subscriptions tolerate old/corrupt preferences", () => {
  assert.deepEqual(readNewsLayout(null), {
    align: "left",
    dock: null,
    height: 244,
  });
  assert.deepEqual(
    readNewsLayout('{"version":1,"news":{"align":"right","dock":"left"}}'),
    { align: "right", dock: "left", height: 244 },
  );
  assert.deepEqual(
    readNewsLayout('{"version":1,"news":{"align":"invalid","dock":"center"}}'),
    { align: "left", dock: null, height: 244 },
  );
  for (const [value, height] of [
    [440, 440],
    [0, 120],
    [10000, 4096],
    ["300", 244],
  ])
    assert.equal(
      readNewsLayout(JSON.stringify({ version: 1, news: { height: value } }))
        .height,
      height,
    );
  for (const value of [
    null,
    "broken",
    '{"version":1}',
    '{"version":2,"widgets":["quote"]}',
  ])
    assert.deepEqual(readHomeWidgets(value), []);
  assert.deepEqual(
    readHomeWidgets(
      '{"version":1,"widgets":["quote","invalid","news","quote"]}',
    ),
    ["quote", "news"],
  );
  assert.equal(
    readQuoteStyle('{"version":1,"quoteStyle":"minecraft"}'),
    "splash",
  );
  assert.equal(readQuoteStyle('{"version":1,"quoteStyle":"splash"}'), "splash");
  assert.equal(
    readQuoteStyle('{"version":1,"quoteStyle":"invalid"}'),
    "simple",
  );
  assert.deepEqual(readQuoteAngles(null), { simple: 0, splash: -20 });
  assert.deepEqual(readQuoteScales(null), { simple: 100, splash: 100 });
  assert.deepEqual(
    readQuoteScales('{"version":1,"quoteScales":{"simple":120,"splash":500}}'),
    { simple: 120, splash: 160 },
  );
  assert.deepEqual(
    readQuoteAngles('{"version":1,"quoteAngles":{"simple":25.8,"splash":-99}}'),
    { simple: 26, splash: -45 },
  );
  assert.deepEqual(readFeedSources(null), [presetFeeds[0]]);
  assert.deepEqual(readFeedSources("[]"), []);
  assert.deepEqual(
    readFeedSources(
      JSON.stringify([
        { url: "javascript:alert(1)" },
        { url: "https://example.org/feed", name: "Feed" },
        { url: "https://example.org/feed" },
      ]),
    ),
    [{ url: "https://example.org/feed", name: "Feed" }],
  );
  assert.equal(
    safeFeedUrl("../article", "https://example.org/feed/rss.xml"),
    "https://example.org/article",
  );
  for (const url of [
    "file:///etc/passwd",
    "data:text/xml,test",
    "https://user:password@example.org/feed",
  ])
    assert.equal(safeFeedUrl(url), null);
});

test("quotes and mixed-source headlines deduplicate links and sort newest first", () => {
  assert.deepEqual(
    parseQuote(
      '{"hitokoto":"hello  world","type":"a","from":"《A》","from_who":"B"}',
    ),
    { text: "hello world", source: "——B《A》" },
  );
  assert.throws(() => parseQuote('{"hitokoto":null}'));
  const item = (url, date) => ({ url, date, title: url, source: "Feed" });
  assert.deepEqual(
    mergeNews([
      [item("older", "2026-01-01"), item("new", "2026-04-01")],
      [item("new", "2026-04-01"), item("undated", "")],
    ]).map((item) => item.url),
    ["new", "older", "undated"],
  );
  assert.equal(freshWidgetCache({ time: 200 }, 100, 100), false);
  assert.equal(freshWidgetCache({ time: 100 }, 100, 150), true);
});

test("quotes filter inclusive length and topics both in the request and cached response", () => {
  const query = new URL(HITOKOTO_URL).searchParams;
  assert.equal(query.get("min_length"), "8");
  assert.equal(query.get("max_length"), "40");
  assert.deepEqual(query.getAll("c"), ["a", "c", "d", "i"]);
  for (const length of [8, 40])
    assert.equal(
      parseQuote(JSON.stringify({ hitokoto: "字".repeat(length), type: "i" }))
        .text.length,
      length,
    );
  for (const value of [
    { hitokoto: "字".repeat(7), type: "i" },
    { hitokoto: "字".repeat(41), type: "i" },
    { hitokoto: "字".repeat(10), type: "g" },
  ])
    assert.throws(() => parseQuote(JSON.stringify(value)));
});

test("requests coalesce, reuse fresh cache, retain content on failure, and replace malformed cache", async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  const storage = new Map();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (key) => storage.get(key),
      setItem: (key, value) => storage.set(key, value),
    },
  });
  const url = "https://example.org/feed";
  let calls = 0,
    resolve;
  const rpc = async () => {
    calls++;
    return new Promise((done) => {
      resolve = done;
    });
  };
  const validate = (data) => {
    assert.equal(data.text, "valid");
  };
  try {
    const first = fetchWidget(rpc, url, 10000, validate),
      second = fetchWidget(rpc, url, 10000, validate);
    assert.equal(calls, 1);
    resolve({ text: "valid", url });
    assert.deepEqual(await first, await second);
    await fetchWidget(rpc, url, 10000, validate);
    assert.equal(calls, 1);
    const startup = fetchWidget(rpc, url, 10000, validate, true);
    let returnedCached = false;
    const concurrent = fetchWidget(rpc, url, 10000, validate).then((value) => {
      returnedCached = true;
      return value;
    });
    await Promise.resolve();
    assert.equal(
      returnedCached,
      false,
      "wait for the refresh instead of returning the previous quote",
    );
    assert.equal(calls, 2);
    resolve({ text: "valid", url });
    assert.deepEqual(await startup, await concurrent);
    await assert.rejects(
      fetchWidget(
        async () => {
          throw new Error("offline");
        },
        url,
        10000,
        validate,
        true,
      ),
    );
    assert.equal(cachedWidget(url).text, "valid");
    storage.set(
      WIDGET_CACHE_KEY,
      JSON.stringify({
        [url]: { time: Date.now(), data: { text: "broken", url } },
      }),
    );
    await fetchWidget(
      async () => ({ text: "valid", url }),
      url,
      10000,
      validate,
    );
    assert.equal(cachedWidget(url).text, "valid");
  } finally {
    if (previous) Object.defineProperty(globalThis, "localStorage", previous);
    else delete globalThis.localStorage;
  }
});
