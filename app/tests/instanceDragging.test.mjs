import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, ref } from "vue";
import { useInstanceSorting } from "../src/useInstanceSorting.ts";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

function draggingFixture(ids = ["a", "b"], grid = false) {
  const previous = Object.fromEntries(
    [
      "window",
      "document",
      "requestAnimationFrame",
      "cancelAnimationFrame",
      "getComputedStyle",
    ].map((k) => [k, globalThis[k]]),
  );
  const listeners = new Map(),
    frames = new Map();
  let frameId = 0,
    ui;
  const animations = [];
  const surface = () => ({
    addEventListener: (type, callback) => listeners.set(type, callback),
    removeEventListener: (type) => listeners.delete(type),
  });
  globalThis.window = { ...surface(), matchMedia: () => ({ matches: true }) };
  globalThis.document = { ...surface(), hidden: false };
  globalThis.requestAnimationFrame = (fn) => {
    frames.set(++frameId, fn);
    return frameId;
  };
  globalThis.cancelAnimationFrame = (id) => frames.delete(id);
  globalThis.getComputedStyle = () => ({ opacity: ".75", filter: "none" });
  const instances = ref(ids.map((id) => ({ id }))),
    blocked = ref(false),
    allowed = ref(true);
  const moves = [],
    removals = [];
  let remove = async () => true;
  const captures = () => {
    const pointers = new Set();
    return {
      setPointerCapture: (id) => pointers.add(id),
      hasPointerCapture: (id) => pointers.has(id),
      releasePointerCapture: (id) => pointers.delete(id),
    };
  };
  const target = { ...captures(), querySelector: () => ({}) };
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
    createElement: () => ({}),
    createText: () => ({}),
    setText() {},
    setElementText() {},
    patchProp() {},
  });
  const app = renderer.createApp({
    setup() {
      ui = useInstanceSorting(
        instances,
        blocked,
        (id, before) => moves.push([id, before]),
        () => {},
        {
          allowed: () => allowed.value,
          remove: async (id) => {
            removals.push(id);
            return remove(id);
          },
        },
      );
      return () => null;
    },
  });
  app.mount({});
  ui.strip.value = {
    ...captures(),
    scrollLeft: 0,
    scrollTop: 0,
    clientWidth: 200,
    getBoundingClientRect: () => ({
      left: 100,
      right: grid ? 640 : 300,
      top: 500,
      bottom: 564,
      width: grid ? 540 : 200,
    }),
    querySelectorAll: () =>
      (grid ? ui.displayInstances.value.slice(0, 8) : instances.value).map(
        (i, index) => ({
          dataset: { instanceId: i.id },
          offsetLeft: index * 60,
          offsetTop: 0,
          offsetWidth: 56,
          offsetHeight: 56,
        }),
      ),
  };
  if (grid) {
    ui.overflow.value = {
      scrollTop: 0,
      scrollLeft: 0,
      clientHeight: 160,
      getBoundingClientRect: () => ({
        left: 100,
        right: 640,
        top: 300,
        bottom: 460,
        width: 540,
        height: 160,
      }),
      querySelectorAll: () =>
        ui.displayInstances.value.slice(8).map((item, index) => ({
          dataset: { instanceId: item.id },
          offsetLeft: (index % 9) * 60,
          offsetTop: Math.floor(index / 9) * 80,
          offsetWidth: 56,
          offsetHeight: 56,
        })),
    };
    ui.toggle.value = {
      getBoundingClientRect: () => ({
        left: 45,
        right: 89,
        top: 500,
        bottom: 544,
        width: 44,
        height: 44,
      }),
    };
  }
  ui.trash.value = {
    getBoundingClientRect: () => ({
      left: 800,
      right: 872,
      top: 20,
      bottom: 92,
      width: 72,
      height: 72,
    }),
  };
  ui.ghost.value = {
    style: {},
    getBoundingClientRect: () => ({
      left: 808,
      top: 28,
      width: 56,
      height: 56,
    }),
    animate(keyframes, options) {
      const done = deferred();
      const animation = {
        keyframes,
        options,
        finished: done.promise,
        cancel() {
          done.reject(new Error("cancelled"));
        },
        finish: done.resolve,
      };
      animations.push(animation);
      return animation;
    },
  };
  const event = (x, y) => ({
    pointerId: 1,
    button: 0,
    isPrimary: true,
    currentTarget: target,
    clientX: x,
    clientY: y,
    preventDefault() {},
  });
  return {
    ui,
    instances,
    blocked,
    allowed,
    moves,
    removals,
    listeners,
    frames,
    animations,
    remove: (fn) => {
      remove = fn;
    },
    start: (id = ids[0]) => ui.start(event(128, 528), id),
    track: (x, y) => {
      ui.track(event(x, y));
      const pending = [...frames.values()];
      frames.clear();
      pending.forEach((fn) => fn());
    },
    release: (x, y) => ui.release(event(x, y)),
    dispose() {
      app.unmount();
      Object.assign(globalThis, previous);
    },
  };
}

test("even a single instance can be dragged to trash; a click does not remove it", async () => {
  const f = draggingFixture(["a"]);
  try {
    f.start();
    f.track(130, 528);
    f.release(130, 528);
    assert.equal(f.ui.trashVisible.value, false);
    assert.deepEqual(f.removals, []);
    f.start();
    f.track(836, 56);
    assert.equal(f.ui.trashVisible.value, true);
    assert.equal(f.ui.trashHovered.value, true);
    f.release(836, 56);
    await flush();
    assert.deepEqual(f.removals, ["a"]);
    assert.deepEqual(f.moves, []);
    assert.equal(f.ui.draggedId.value, null);
    assert.equal(f.ui.trashVisible.value, false);
  } finally {
    f.dispose();
  }
});

test("outside drops, Escape, lost capture and unavailable removal return without deleting", async () => {
  for (const scenario of ["outside", "escape", "capture", "disabled"]) {
    const f = draggingFixture();
    try {
      f.start();
      f.track(836, 56);
      if (scenario === "outside") f.release(750, 56);
      else if (scenario === "escape")
        f.listeners.get("keydown")({ key: "Escape", preventDefault() {} });
      else if (scenario === "capture") {
        f.ui.strip.value.releasePointerCapture(1);
        f.ui.lostCapture({ target: f.ui.strip.value });
      } else {
        f.allowed.value = false;
        f.release(836, 56);
      }
      await flush();
      assert.deepEqual(f.removals, [], scenario);
      assert.equal(f.ui.draggedId.value, null, scenario);
      assert.equal(f.frames.size, 0);
    } finally {
      f.dispose();
    }
  }
});

test("a failed removal lands back, while successful removal retains the ghost through its animation", async () => {
  const f = draggingFixture();
  try {
    f.remove(async () => false);
    f.start();
    f.track(836, 56);
    f.release(836, 56);
    await flush();
    assert.equal(f.ui.draggedId.value, null);
    assert.deepEqual(
      f.instances.value.map((i) => i.id),
      ["a", "b"],
    );
    globalThis.window.matchMedia = () => ({ matches: false });
    f.remove(async (id) => {
      f.instances.value = f.instances.value.filter((i) => i.id !== id);
      return true;
    });
    f.start();
    f.track(836, 56);
    f.release(836, 56);
    await flush();
    assert.equal(f.ui.draggedInstance.value.id, "a");
    assert.equal(f.ui.removing.value, true);
    assert.equal(f.animations.at(-1).options.duration, 220);
    f.animations.at(-1).finish();
    await flush();
    assert.equal(f.ui.draggedId.value, null);
    assert.equal(f.ui.removing.value, false);
    assert.equal(
      f.ui.ghost.value.style.opacity,
      "0",
      "finished removal cannot flash back during Vue leave",
    );
  } finally {
    f.dispose();
  }
});

test("ordinary sorting still commits its target and disposal stops the drag loop", async () => {
  const f = draggingFixture();
  try {
    f.start();
    f.track(280, 528);
    f.release(280, 528);
    await flush();
    assert.deepEqual(f.moves, [["a", null]]);
    assert.deepEqual(f.removals, []);
    f.start();
    f.track(836, 56);
  } finally {
    f.dispose();
  }
  assert.equal(f.frames.size, 0);
});

test("App removes by ID, selects a neighbor, and keeps the instance on IPC failure", async () => {
  const { fixture: f, context: ui, dispose } = await settingsFixture();
  try {
    const first = f.state.database.instances[0];
    f.state.database.instances.push({ ...first, id: "b" });
    await ui.sync();
    ui.selectInstance(first.id);
    f.handlers.delete = () => {
      throw new Error("locked");
    };
    assert.equal(await ui.removeDraggedInstance(first.id), false);
    assert.equal(ui.instances.value.length, 2);
    assert.equal(ui.error.value, "locked");
    f.handlers.delete = ({ id }) => {
      f.state.database.instances = f.state.database.instances.filter(
        (i) => i.id !== id,
      );
      return true;
    };
    assert.equal(await ui.removeDraggedInstance(first.id), true);
    assert.equal(ui.selectedInstanceId.value, "b");
    f.state.running = ["b"];
    await ui.sync();
    assert.equal(await ui.removeDraggedInstance("b"), false);
    ui.busy.value = true;
    assert.equal(ui.canRemoveDraggedInstance("b"), false);
    ui.busy.value = false;
    f.state.running = [];
    await ui.sync();
    assert.equal(await ui.removeDraggedInstance("b"), true);
    assert.equal(ui.instances.value.length, 0);
    assert.equal(ui.selectedInstanceId.value, "");
  } finally {
    dispose();
  }
});

test("cross-region drags keep eight visible tiles and commit or cancel without losing instances", async () => {
  for (const source of ["b", "j"]) {
    for (const commit of [true, false]) {
      const f = draggingFixture("abcdefghijk".split(""), true);
      try {
        f.ui.expanded.value = true;
        f.start(source);
        const [x, y] = source === "b" ? [120, 330] : [620, 528];
        f.track(x, y);
        await flush();
        const preview = f.ui.displayInstances.value.map((i) => i.id);
        assert.equal(preview.indexOf(source), source === "b" ? 8 : 7);
        f.track(x, y);
        await flush();
        assert.deepEqual(
          f.ui.displayInstances.value.map((i) => i.id),
          preview,
        );
        if (commit) f.release(x, y);
        else f.ui.cancel();
        await flush();
        assert.deepEqual(
          f.moves,
          commit ? [[source, source === "b" ? "j" : "h"]] : [],
        );
        assert.equal(f.ui.draggedId.value, null);
        assert.equal(f.instances.value.length, 11);
        assert.equal(f.frames.size, 0);
      } finally {
        f.dispose();
      }
    }
  }
});

test("hovering the arrow expands overflow during dragging; it collapses when only nine remain", async () => {
  const f = draggingFixture("abcdefghij".split(""), true);
  const previous = Date.now;
  let now = 1000;
  Date.now = () => now;
  try {
    f.start("a");
    f.track(65, 522);
    assert.equal(f.ui.expanded.value, false);
    now += 279;
    f.track(65, 522);
    assert.equal(f.ui.expanded.value, false);
    now++;
    f.track(65, 522);
    assert.equal(f.ui.expanded.value, true);
    f.ui.cancel();
    await flush();
    f.instances.value = f.instances.value.slice(0, 9);
    await flush();
    assert.equal(f.ui.expanded.value, false);
  } finally {
    Date.now = previous;
    f.dispose();
  }
});
