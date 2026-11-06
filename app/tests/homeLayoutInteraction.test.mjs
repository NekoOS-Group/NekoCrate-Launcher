import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick, reactive } from "vue";
import { HOME_LAYOUT_KEY } from "../src/homeLayout.ts";
import {
  NEWS_SOURCES_KEY,
  presetFeeds,
  homeWidgets,
} from "../src/homeWidgets.ts";
import { settingsFixture, flush } from "./helpers/appSettingsFixture.mjs";
const file = new URL("../src/components/HomeLayout.vue", import.meta.url);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "layout-interaction",
  }).content,
  {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  },
).outputText;
code = code.replace(
  /import NewsSources from ["']\.\/NewsSources\.vue["'];/,
  "const NewsSources = {};",
);
code = code.replace(
  /import LayoutSlider from ["\']\.\/LayoutSlider\.vue["\'];/,
  "const LayoutSlider = {};",
);
code = code.replace(
  /from ["']([^"']+)["']/g,
  (_, name) =>
    `from ${JSON.stringify(name.startsWith(".") ? new URL(`${name}.ts`, file).href : import.meta.resolve(name))}`,
);
const Component = (
  await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  )
).default;
function fixture(value, portable = false) {
  const keys = [
    "document",
    "window",
    "localStorage",
    "requestAnimationFrame",
    "cancelAnimationFrame",
  ];
  const old = Object.fromEntries(
    keys.map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  const events = [],
    listeners = new Map(),
    classes = new Set(),
    frames = new Map(),
    captures = new Set();
  const storage = new Map(value ? [[HOME_LAYOUT_KEY, value]] : []);
  const addEventListener = (name, fn) => listeners.set(name, fn),
    removeEventListener = (name) => listeners.delete(name);
  let nextFrame = 1;
  const window = {
    innerWidth: 900,
    innerHeight: 600,
    addEventListener,
    removeEventListener,
    dispatchEvent() {},
  };
  const document = {
    documentElement: {
      classList: {
        add: (name) => classes.add(name),
        remove: (name) => classes.delete(name),
      },
    },
    addEventListener,
    removeEventListener,
  };
  const target = {
    focus() {},
    setPointerCapture: (id) => captures.add(id),
    releasePointerCapture: (id) => captures.delete(id),
    hasPointerCapture: (id) => captures.has(id),
  };
  Object.assign(globalThis, {
    window,
    document,
    localStorage: {
      getItem: (key) => storage.get(key),
      setItem: (key, v) => storage.set(key, v),
      removeItem: (key) => storage.delete(key),
    },
    requestAnimationFrame: (fn) => {
      const id = nextFrame++;
      frames.set(id, fn);
      return id;
    },
    cancelAnimationFrame: (id) => frames.delete(id),
  });
  let ui;
  const props = reactive({ instanceSlots: 4, portable });
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      ui = Component.setup(props, {
        expose() {},
        emit: (_, value) => events.push(value),
      });
      return () => null;
    },
  });
  app.mount({});
  const event = (x = 60, y = 50) => ({
    pointerId: 1,
    button: 0,
    isPrimary: true,
    currentTarget: target,
    clientX: x,
    clientY: y,
    preventDefault() {},
    stopPropagation() {},
  });
  return {
    ui,
    props,
    window,
    events,
    listeners,
    storage,
    classes,
    captures,
    frames,
    event,
    tick() {
      const pending = [...frames.values()];
      frames.clear();
      pending.forEach((fn) => fn());
    },
    dispose() {
      app.unmount();
      for (const key of keys)
        if (old[key]) Object.defineProperty(globalThis, key, old[key]);
        else delete globalThis[key];
    },
  };
}
test("layout stays locked normally; drag previews yield to neighbours and commit only on Done", async () => {
  const f = fixture();
  try {
    f.ui.start(f.event(), "account");
    assert.equal(f.captures.size, 0);
    f.ui.begin();
    const before = { ...f.ui.rects.value.account };
    const toolsBefore = { ...f.ui.rects.value.tools };
    f.ui.start(f.event(), "account");
    assert.equal(f.classes.has("home-layout-dragging"), true);
    f.ui.move(f.event(380, 300));
    f.ui.move(f.event(400, 310));
    assert.equal(f.frames.size, 1);
    f.tick();
    assert.notEqual(f.ui.rects.value.account.x, before.x);
    f.ui.release(f.event(400, 310));
    assert.equal(f.classes.size, 0);
    assert.equal(f.captures.size, 0);
    assert.equal(f.storage.size, 0);
    const destination = { ...f.ui.rects.value.account };
    assert.deepEqual(f.ui.rects.value.tools, toolsBefore);
    f.ui.end(true);
    assert.equal(f.ui.editing.value, false);
    assert.ok(f.storage.has(HOME_LAYOUT_KEY));
    assert.deepEqual(f.events, [true, false]);
    assert.ok(Math.abs(f.ui.rects.value.account.x - destination.x) < 0.01);
    f.ui.begin();
    f.ui.start(f.event(), "tools");
    f.ui.move(f.event(140, 170));
    f.tick();
    f.ui.release(f.event(140, 170));
    f.ui.end(true);
    assert.deepEqual(f.ui.rects.value.account, destination);
    assert.notEqual(f.ui.rects.value.tools.x, toolsBefore.x);
    const positions = JSON.parse(f.storage.get(HOME_LAYOUT_KEY)).positions;
    assert.notDeepEqual(positions.account, positions.tools);
    await nextTick();
  } finally {
    f.dispose();
  }
  assert.equal(f.listeners.size, 0);
});
test("cancelled gestures, blur and Escape restore the last layout; reset removes only its own preference", async () => {
  const f = fixture();
  try {
    f.storage.set("unrelated", "keep");
    f.ui.begin();
    f.ui.nudge(
      { key: "ArrowRight", shiftKey: true, preventDefault() {} },
      "account",
    );
    const base = { ...f.ui.rects.value.account };
    f.ui.start(f.event(), "account");
    f.ui.move(f.event(500, 330));
    f.tick();
    f.listeners.get("blur")();
    assert.deepEqual(f.ui.rects.value.account, base);
    f.ui.end(true);
    const persisted = f.storage.get(HOME_LAYOUT_KEY);
    f.ui.begin();
    f.ui.reset();
    f.ui.keyboard({ key: "Escape", preventDefault() {} });
    assert.equal(f.storage.get(HOME_LAYOUT_KEY), persisted);
    f.ui.begin();
    f.ui.reset();
    f.ui.end(true);
    assert.equal(f.storage.has(HOME_LAYOUT_KEY), false);
    assert.equal(f.storage.get("unrelated"), "keep");
    f.ui.begin();
    f.ui.start(f.event(), "themes");
    f.ui.move(f.event(120, 400));
    await nextTick();
  } finally {
    f.dispose();
  }
  assert.equal(f.frames.size, 0);
  assert.equal(f.captures.size, 0);
  assert.equal(f.classes.size, 0);
});
test("saved positions load before paint and viewport changes keep all controls reachable", () => {
  const value = JSON.stringify({
    version: 1,
    positions: {
      account: { x: 1, y: 1 },
      instances: { x: 0, y: 0 },
      themes: { x: 0, y: 1 },
      launch: { x: 1, y: 0 },
    },
  });
  const f = fixture(value);
  try {
    assert.equal(f.ui.rects.value.tools.x + f.ui.rects.value.tools.width, 888);
    assert.equal(f.ui.rects.value.account.x, 508);
    f.window.innerWidth = 760;
    f.window.innerHeight = 500;
    f.listeners.get("resize")();
    for (const box of Object.values(f.ui.rects.value)) {
      assert.ok(
        box.x >= 12 &&
          box.y >= 12 &&
          box.x + box.width <= 748 &&
          box.y + box.height <= 488,
      );
    }
    assert.equal(f.storage.get(HOME_LAYOUT_KEY), value);
  } finally {
    f.dispose();
  }
});
test("orientation changes preview independently, save with positions, and cancel or reset together", () => {
  const f = fixture();
  let value;
  try {
    f.ui.setOrientation("tools", "vertical");
    assert.equal(f.ui.orientations.value.tools, "horizontal");
    f.ui.begin();
    const account = { ...f.ui.rects.value.account };
    f.ui.setOrientation("tools", "vertical");
    assert.deepEqual(f.ui.orientations.value, {
      tools: "vertical",
      instances: "horizontal",
    });
    assert.deepEqual(f.ui.rects.value.account, account);
    assert.equal(f.ui.rects.value.tools.width, 64);
    assert.equal(f.ui.rects.value.tools.height, 144);
    f.ui.setOrientation("instances", "vertical");
    const destination = { ...f.ui.rects.value.instances };
    assert.equal(destination.width, 64);
    assert.equal(f.storage.size, 0);
    f.ui.end(true);
    value = f.storage.get(HOME_LAYOUT_KEY);
    assert.deepEqual(JSON.parse(value).orientations, {
      tools: "vertical",
      instances: "vertical",
    });
    f.ui.begin();
    f.ui.setOrientation("instances", "horizontal");
    f.ui.end(false);
    assert.deepEqual(f.ui.rects.value.instances, destination);
    assert.equal(f.storage.get(HOME_LAYOUT_KEY), value);
    f.ui.begin();
    f.ui.reset();
    assert.deepEqual(f.ui.orientations.value, {
      tools: "horizontal",
      instances: "horizontal",
    });
    f.ui.end(false);
    assert.equal(f.ui.orientations.value.tools, "vertical");
  } finally {
    f.dispose();
  }
  const reloaded = fixture(value);
  try {
    assert.deepEqual(reloaded.ui.orientations.value, {
      tools: "vertical",
      instances: "vertical",
    });
    assert.equal(reloaded.ui.rects.value.instances.width, 64);
    reloaded.ui.begin();
    reloaded.ui.reset();
    reloaded.ui.end(true);
    assert.equal(reloaded.storage.size, 0);
    assert.equal(reloaded.ui.orientations.value.instances, "horizontal");
  } finally {
    reloaded.dispose();
  }
});
test("layout editing suppresses native file imports and browser resource drops", async () => {
  const { fixture: f, context: ui, dispose } = await settingsFixture();
  try {
    await flush();
    ui.layoutEditing.value = true;
    const before = f.calls.length;
    f.fileDrop({
      payload: {
        type: "drop",
        paths: ["/Mods/example.jar"],
        position: { x: 200, y: 300 },
      },
    });
    let prevented = false;
    const event = {
      preventDefault() {
        prevented = true;
      },
      dataTransfer: { dropEffect: "copy" },
    };
    ui.previewProjectLink(event);
    await ui.receiveProjectLink(event);
    assert.equal(prevented, true);
    assert.equal(event.dataTransfer.dropEffect, "none");
    assert.equal(f.calls.length, before);
  } finally {
    dispose();
  }
});
test("widgets preview, cancel, and persist placement/removal with the layout", () => {
  const f = fixture();
  let stored;
  try {
    assert.deepEqual(f.ui.widgets.value, []);
    assert.deepEqual(f.ui.availableWidgets.value, homeWidgets);
    f.ui.toggleWidget("quote");
    assert.deepEqual(f.ui.widgets.value, []);
    f.ui.begin();
    f.ui.toggleWidget("quote");
    f.ui.toggleWidget("news");
    f.ui.setQuoteStyle("splash");
    f.ui.setQuoteAngle(12);
    f.ui.setQuoteScale(140);
    assert.deepEqual(f.ui.widgets.value, ["quote", "news"]);
    assert.deepEqual(
      f.ui.availableWidgets.value,
      homeWidgets.filter((id) => !["quote", "news"].includes(id)),
    );
    assert.ok(f.ui.rects.value.quote);
    assert.ok(f.ui.rects.value.news);
    f.ui.end(false);
    assert.equal(f.ui.quoteStyle.value, "simple");
    assert.equal(f.ui.quoteAngle.value, 0);
    assert.equal(f.ui.quoteScale.value, 100);
    assert.deepEqual(f.ui.widgets.value, []);
    assert.equal(f.storage.size, 0);
    f.ui.begin();
    f.ui.toggleWidget("quote");
    f.ui.toggleWidget("news");
    f.ui.setQuoteAngle(15);
    f.ui.setQuoteScale(120);
    f.ui.setQuoteStyle("splash");
    assert.equal(f.ui.quoteAngle.value, -20);
    assert.equal(f.ui.quoteScale.value, 100);
    f.ui.setQuoteScale(150);
    assert.equal(f.ui.rects.value.quote.width, 450);
    f.ui.setQuoteAngle(-30);
    const before = f.ui.rects.value.quote.x;
    f.ui.nudge(
      { key: "ArrowRight", shiftKey: false, preventDefault() {} },
      "quote",
    );
    assert.equal(f.ui.rects.value.quote.x, before + 8);
    f.ui.end(true);
    stored = f.storage.get(HOME_LAYOUT_KEY);
    assert.deepEqual(JSON.parse(stored).widgets, ["quote", "news"]);
  } finally {
    f.dispose();
  }
  const restored = fixture(stored);
  try {
    assert.equal(restored.ui.quoteStyle.value, "splash");
    assert.equal(restored.ui.quoteAngle.value, -30);
    assert.equal(restored.ui.quoteScale.value, 150);
    assert.deepEqual(restored.ui.widgets.value, ["quote", "news"]);
    restored.ui.begin();
    restored.ui.setQuoteStyle("simple");
    assert.equal(restored.ui.quoteAngle.value, 15);
    assert.equal(restored.ui.quoteScale.value, 120);
    restored.ui.toggleWidget("news");
    assert.deepEqual(
      restored.ui.availableWidgets.value,
      homeWidgets.filter((id) => id !== "quote"),
    );
    restored.ui.end(true);
    assert.equal(restored.ui.rects.value.news, undefined);
    assert.deepEqual(
      JSON.parse(restored.storage.get(HOME_LAYOUT_KEY)).widgets,
      ["quote"],
    );
    restored.ui.begin();
    restored.ui.reset();
    restored.ui.end(true);
    assert.equal(restored.storage.size, 0);
    assert.deepEqual(restored.ui.widgets.value, []);
  } finally {
    restored.dispose();
  }
});

test("news edge docking and alignment preview, persist, cancel and undock without losing the layout", () => {
  const f = fixture();
  let value;
  try {
    f.ui.begin();
    f.ui.toggleWidget("news");
    f.ui.toggleNewsAlignment();
    const before = { ...f.ui.rects.value.account };
    f.ui.start(f.event(750, 200), "news");
    f.ui.move(f.event(1000, 200));
    f.tick();
    assert.equal(f.ui.newsDock.value, "right");
    f.ui.cancelDrag();
    assert.equal(f.ui.newsDock.value, null);
    f.ui.start(f.event(750, 200), "news");
    f.ui.move(f.event(1000, 200));
    f.tick();
    f.ui.release(f.event(1000, 200));
    assert.equal(f.ui.newsDock.value, "right");
    assert.deepEqual(f.ui.rects.value.account, before);
    f.ui.end(true);
    value = f.storage.get(HOME_LAYOUT_KEY);
    assert.deepEqual(JSON.parse(value).news, {
      align: "right",
      dock: "right",
      height: 244,
    });
    f.ui.begin();
    f.ui.undockNews();
    assert.equal(f.ui.newsDock.value, null);
    f.ui.end(false);
    assert.equal(f.ui.newsDock.value, "right");
  } finally {
    f.dispose();
  }
  const loaded = fixture(value);
  try {
    assert.equal(loaded.ui.newsDock.value, "right");
    assert.equal(loaded.ui.newsLayout.value.align, "right");
    assert.equal(loaded.ui.newsOpen.value, false);
    loaded.ui.begin();
    loaded.ui.undockNews();
    loaded.ui.end(true);
    assert.equal(
      JSON.parse(loaded.storage.get(HOME_LAYOUT_KEY)).news.dock,
      null,
    );
  } finally {
    loaded.dispose();
  }
});

test("toolbar contents preview, cancel, keep settings when clearing, and restore defaults", () => {
  const f = fixture();
  let value;
  try {
    assert.deepEqual(f.ui.tools.value, ["resources", "settings", "logs"]);
    f.ui.setTools(["store"]);
    assert.equal(f.ui.tools.value.includes("store"), false);
    f.ui.begin();
    f.ui.setTools(["store", "settings"]);
    assert.equal(f.ui.rects.value.tools.width, 94);
    f.ui.end(false);
    assert.deepEqual(f.ui.tools.value, ["resources", "settings", "logs"]);
    f.ui.begin();
    f.ui.setTools([]);
    f.ui.end(true);
    value = f.storage.get(HOME_LAYOUT_KEY);
    assert.deepEqual(JSON.parse(value).tools, ["settings"]);
  } finally {
    f.dispose();
  }
  const restored = fixture(value);
  try {
    assert.deepEqual(restored.ui.tools.value, ["settings"]);
    assert.equal(restored.ui.rects.value.tools.width, 44);
    restored.ui.begin();
    restored.ui.setTools(["logs", "store", "resources", "settings"]);
    restored.ui.setOrientation("tools", "vertical");
    assert.equal(restored.ui.rects.value.tools.height, 194);
    restored.ui.end(true);
    assert.deepEqual(JSON.parse(restored.storage.get(HOME_LAYOUT_KEY)).tools, [
      "logs",
      "store",
      "resources",
      "settings",
    ]);
    restored.ui.begin();
    restored.ui.reset();
    restored.ui.end(true);
    assert.deepEqual(restored.ui.tools.value, [
      "resources",
      "settings",
      "logs",
    ]);
    assert.equal(restored.storage.size, 0);
  } finally {
    restored.dispose();
  }
});

test("news subscriptions migrate, preview and follow layout save, cancel and reset", () => {
  const f = fixture();
  const custom = { name: "example.org", url: "https://example.org/feed" };
  try {
    f.ui.setNewsSources([]);
    assert.deepEqual(f.ui.newsSources.value, [presetFeeds[0]]);
    f.ui.begin();
    f.ui.setNewsSources([presetFeeds[1], custom]);
    assert.deepEqual(f.ui.newsSources.value, [presetFeeds[1], custom]);
    assert.equal(f.storage.has(NEWS_SOURCES_KEY), false);
    f.ui.end(false);
    assert.deepEqual(f.ui.newsSources.value, [presetFeeds[0]]);
    f.ui.begin();
    f.ui.setNewsSources([presetFeeds[1], custom]);
    f.ui.end(true);
    assert.deepEqual(JSON.parse(f.storage.get(NEWS_SOURCES_KEY)), [
      presetFeeds[1],
      custom,
    ]);
    f.ui.begin();
    f.ui.setNewsSources([]);
    f.ui.end(true);
    assert.equal(f.storage.get(NEWS_SOURCES_KEY), "[]");
    f.ui.begin();
    f.ui.reset();
    f.ui.end(false);
    assert.deepEqual(f.ui.newsSources.value, []);
    f.ui.begin();
    f.ui.reset();
    f.ui.end(true);
    assert.deepEqual(f.ui.newsSources.value, [presetFeeds[0]]);
    assert.equal(f.storage.has(NEWS_SOURCES_KEY), false);
  } finally {
    f.dispose();
  }
});

test("Trends resizes with pointer and keyboard, cancels gestures, preserves docking and saves height", () => {
  const f = fixture();
  let stored;
  try {
    f.ui.begin();
    f.ui.toggleWidget("news");
    const before = { ...f.ui.rects.value.news };
    f.ui.start(f.event(700, 320), "news", "bottom");
    f.ui.move(f.event(700, 420));
    f.tick();
    assert.equal(f.ui.rects.value.news.height, before.height + 100);
    assert.equal(f.ui.rects.value.news.y, before.y);
    assert.ok(f.classes.has("home-layout-resizing"));
    f.ui.cancelDrag();
    assert.equal(f.ui.rects.value.news.height, before.height);
    assert.equal(f.classes.size, 0);
    f.ui.start(f.event(700, 320), "news", "bottom");
    f.ui.move(f.event(700, 420));
    f.tick();
    f.ui.release(f.event(700, 420));
    assert.equal(f.ui.newsLayout.value.height, 344);
    const box = f.ui.rects.value.news;
    // Only drag to the visible 12px inset, never outside the window.
    f.ui.start(f.event(700, 220), "news");
    const x = 700 + 900 - 12 - box.width - box.x;
    f.ui.move(f.event(x, 220));
    f.tick();
    f.ui.release(f.event(x, 220));
    assert.equal(f.ui.newsDock.value, "right");
    f.ui.nudgeNewsSize({ key: "ArrowDown", preventDefault() {} }, "bottom");
    assert.equal(f.ui.newsLayout.value.height, 352);
    assert.equal(f.ui.newsDock.value, "right");
    f.ui.end(true);
    stored = f.storage.get(HOME_LAYOUT_KEY);
    f.ui.begin();
    f.ui.nudgeNewsSize({ key: "ArrowDown", preventDefault() {} }, "top");
    f.ui.end(false);
    assert.equal(f.ui.newsLayout.value.height, 352);
  } finally {
    f.dispose();
  }
  const restored = fixture(stored);
  try {
    assert.equal(restored.ui.rects.value.news.height, 352);
    assert.equal(restored.ui.newsDock.value, "right");
    restored.ui.begin();
    restored.ui.nudge({ key: "ArrowLeft", preventDefault() {} }, "news");
    assert.equal(restored.ui.newsDock.value, null);
    restored.ui.end(true);
  } finally {
    restored.dispose();
  }
});

test("editing then resizing preserves adjacent controls while detached controls remain independent", () => {
  const f = fixture();
  let stored;
  try {
    f.ui.begin();
    f.ui.toggleWidget("news");
    f.ui.end(true);
    stored = f.storage.get(HOME_LAYOUT_KEY);
    assert.equal(JSON.parse(stored).attachments.tools.side, "right");
    for (const [width, height] of [
      [1440, 900],
      [760, 500],
      [1200, 800],
    ]) {
      Object.assign(f.window, { innerWidth: width, innerHeight: height });
      f.ui.resize();
      const r = f.ui.rects.value;
      assert.ok(
        Math.abs(r.tools.x - r.account.x - r.account.width - 12) < 0.01,
      );
      assert.ok(
        Math.abs(r.launch.x - r.instances.x - r.instances.width - 12) < 0.01,
      );
    }
    f.ui.begin();
    f.ui.start(f.event(300, 50), "tools");
    f.ui.move(f.event(600, 320));
    f.tick();
    f.ui.release(f.event(600, 320));
    f.ui.end(true);
    assert.equal(
      JSON.parse(f.storage.get(HOME_LAYOUT_KEY)).attachments.tools,
      undefined,
    );
  } finally {
    f.dispose();
  }
  const loaded = fixture(stored);
  try {
    Object.assign(loaded.window, { innerWidth: 1600, innerHeight: 1000 });
    loaded.ui.resize();
    const r = loaded.ui.rects.value;
    assert.ok(Math.abs(r.tools.x - r.account.x - r.account.width - 12) < 0.01);
    assert.ok(
      Math.abs(r.launch.x - r.instances.x - r.instances.width - 12) < 0.01,
    );
  } finally {
    loaded.dispose();
  }
});

test("Trends dismisses on pointer leave even when WebKit retains focus-visible on a link", async () => {
  const f = fixture(
    JSON.stringify({ version: 1, widgets: ["news"], news: { dock: "left" } }),
  );
  try {
    globalThis.document.activeElement = { matches: () => true };
    f.ui.newsHost.value = { contains: () => true };
    f.ui.showNews();
    assert.equal(f.ui.newsOpen.value, true);
    f.ui.leaveNews();
    await new Promise((resolve) => setTimeout(resolve, 220));
    assert.equal(f.ui.newsOpen.value, false);
    f.ui.showNews();
    f.ui.leaveNews();
    f.ui.showNews();
    await new Promise((resolve) => setTimeout(resolve, 220));
    assert.equal(f.ui.newsOpen.value, true, "re-entry cancels a pending close");
  } finally {
    f.dispose();
  }
});

test("all dashboard widgets and clock preferences follow layout save/cancel, removal preserves note data", () => {
  const f = fixture();
  try {
    f.storage.set("nekocrate.home-dashboard.v1", "keep-note-and-favorite-data");
    f.ui.begin();
    for (const id of homeWidgets) f.ui.toggleWidget(id);
    assert.deepEqual(f.ui.availableWidgets.value, []);
    f.ui.draftClock.value = { scale: 130, angle: 12, date: false };
    f.ui.end(false);
    assert.deepEqual(f.ui.widgets.value, []);
    assert.equal(f.ui.clockLayout.value.scale, 100);
    f.ui.begin();
    for (const id of homeWidgets) f.ui.toggleWidget(id);
    f.ui.draftClock.value = { scale: 130, angle: 12, date: false };
    f.ui.end(true);
    const saved = JSON.parse(f.storage.get(HOME_LAYOUT_KEY));
    assert.deepEqual(saved.widgets, homeWidgets);
    assert.deepEqual(saved.clock, { scale: 130, angle: 12, date: false });
    f.ui.begin();
    f.ui.toggleWidget("todos");
    f.ui.end(true);
    assert.equal(
      f.storage.get("nekocrate.home-dashboard.v1"),
      "keep-note-and-favorite-data",
    );
  } finally {
    f.dispose();
  }
});

test("themes are removable and restorable, installed-mode instance dock remains required", () => {
  const f = fixture();
  let stored;
  try {
    assert.ok(f.ui.groups.value.includes("themes"));
    assert.ok(f.ui.groups.value.includes("instances"));
    f.ui.toggleWidget("themes");
    assert.ok(f.ui.groups.value.includes("themes"));
    f.ui.begin();
    f.ui.toggleWidget("instances");
    f.ui.toggleWidget("account");
    f.ui.toggleWidget("launch");
    f.ui.toggleWidget("tools");
    assert.deepEqual(f.ui.hiddenGroups.value, []);
    const position = { ...f.ui.rects.value.themes };
    f.ui.toggleWidget("themes");
    assert.equal(f.ui.groups.value.includes("themes"), false);
    assert.ok(f.ui.availableWidgets.value.includes("themes"));
    f.ui.end(false);
    assert.ok(f.ui.groups.value.includes("themes"));
    assert.equal(f.storage.has(HOME_LAYOUT_KEY), false);
    f.ui.begin();
    f.ui.toggleWidget("themes");
    f.ui.end(true);
    stored = f.storage.get(HOME_LAYOUT_KEY);
    assert.deepEqual(JSON.parse(stored).hiddenGroups, ["themes"]);
    f.ui.begin();
    f.ui.toggleWidget("themes");
    assert.deepEqual(f.ui.rects.value.themes, position);
    assert.equal(f.ui.availableWidgets.value.includes("themes"), false);
    f.ui.end(false);
  } finally {
    f.dispose();
  }
  const restored = fixture(stored);
  try {
    assert.equal(restored.ui.groups.value.includes("themes"), false);
    assert.ok(restored.ui.groups.value.includes("instances"));
    restored.ui.begin();
    restored.ui.toggleWidget("themes");
    restored.ui.end(true);
    assert.deepEqual(
      JSON.parse(restored.storage.get(HOME_LAYOUT_KEY)).hiddenGroups,
      [],
    );
  } finally {
    restored.dispose();
  }
});

test("portable dock removal persists, follows late mode detection, and reset restores both defaults", () => {
  const f = fixture(undefined, true);
  let stored;
  try {
    f.ui.begin();
    f.ui.toggleWidget("themes");
    f.ui.toggleWidget("instances");
    assert.deepEqual(f.ui.hiddenGroups.value, ["themes", "instances"]);
    assert.ok(f.ui.availableWidgets.value.includes("instances"));
    assert.equal(f.ui.groups.value.includes("instances"), false);
    f.ui.end(true);
    stored = f.storage.get(HOME_LAYOUT_KEY);
  } finally {
    f.dispose();
  }
  const loaded = fixture(stored);
  try {
    assert.ok(
      loaded.ui.groups.value.includes("instances"),
      "installed mode never inherits a hidden instance dock",
    );
    assert.equal(loaded.ui.availableWidgets.value.includes("instances"), false);
    loaded.props.portable = true;
    assert.equal(
      loaded.ui.groups.value.includes("instances"),
      false,
      "late snapshot applies portable visibility",
    );
    loaded.ui.begin();
    loaded.ui.toggleWidget("instances");
    assert.ok(loaded.ui.groups.value.includes("instances"));
    loaded.ui.end(false);
    assert.equal(loaded.ui.groups.value.includes("instances"), false);
    loaded.ui.begin();
    loaded.ui.reset();
    assert.ok(loaded.ui.groups.value.includes("instances"));
    assert.ok(loaded.ui.groups.value.includes("themes"));
    loaded.ui.end(true);
    assert.equal(loaded.storage.has(HOME_LAYOUT_KEY), false);
  } finally {
    loaded.dispose();
  }
});
