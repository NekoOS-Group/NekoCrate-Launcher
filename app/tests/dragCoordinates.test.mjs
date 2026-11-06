import assert from "node:assert/strict";
import test from "node:test";
import { nativeDropPoint } from "../src/dragCoordinates.ts";
import { dropHint } from "../src/dropSurface.ts";
import {
  settingsFixture,
  flush,
  deferred,
} from "./helpers/appSettingsFixture.mjs";

test("native drag coordinates match CSS pixels on Retina, scaled GTK, and Win32", () => {
  const point = { x: 600, y: 300 };
  for (const platform of ["MacIntel", "Linux x86_64", "Linux aarch64"]) {
    for (const ratio of [1, 1.5, 2, 3])
      assert.deepEqual(nativeDropPoint(point, platform, ratio), point);
  }
  for (const ratio of [1, 1.25, 1.5, 2, 3])
    assert.deepEqual(
      nativeDropPoint({ x: 600 * ratio, y: 300 * ratio }, "Win32", ratio),
      point,
    );
  for (const ratio of [0, NaN, undefined])
    assert.deepEqual(nativeDropPoint(point, "Win32", ratio ?? NaN), point);
});

function surface(kind, left) {
  return {
    dataset: { dropKind: kind },
    closest(selector) {
      return selector.includes("data-drop-kind") ? this : null;
    },
    getBoundingClientRect: () => ({ left, top: 240, width: 150, height: 140 }),
  };
}
test("native enter, over, and drop all hit the same visible skin target without double scaling", async () => {
  const navigatorBefore = Object.getOwnPropertyDescriptor(
    globalThis,
    "navigator",
  );
  try {
    for (const platform of ["MacIntel", "Linux x86_64", "Win32"]) {
      Object.defineProperty(globalThis, "navigator", {
        configurable: true,
        value: { platform, language: "zh-CN", languages: ["zh-CN"] },
      });
      const f = await settingsFixture();
      try {
        window.devicePixelRatio = 2;
        const skins = surface("skins", 540),
          wrong = surface("java", 200),
          hits = [],
          drops = [];
        document.elementFromPoint = (x, y) => {
          hits.push([x, y]);
          return x >= 540 && y >= 240 ? skins : wrong;
        };
        f.context.resourceManager.value = {
          drop: async (...args) => drops.push(args),
        };
        const position =
          platform === "Win32" ? { x: 1200, y: 600 } : { x: 600, y: 300 };
        f.fileDrop({
          payload: { type: "enter", paths: ["/skin.png"], position },
        });
        f.fileDrop({ payload: { type: "over", position } });
        assert.equal(dropHint.value.left, 540);
        assert.equal(dropHint.value.message, "导入皮肤");
        f.fileDrop({
          payload: { type: "drop", paths: ["/skin.png"], position },
        });
        await flush();
        assert.ok(hits.every(([x, y]) => x === 600 && y === 300));
        assert.deepEqual(drops, [[["/skin.png"], "skins"]]);
      } finally {
        f.dispose();
      }
    }
  } finally {
    if (navigatorBefore)
      Object.defineProperty(globalThis, "navigator", navigatorBefore);
    else delete globalThis.navigator;
  }
});

test("late inspection errors cannot move the drop indicator back to an old target", async () => {
  const f = await settingsFixture();
  try {
    const inspected = deferred();
    f.handlers.inspect_resources = () => inspected.promise;
    f.fixture.hovered = surface("mods", 20);
    f.fileDrop({
      payload: {
        type: "enter",
        paths: ["/skin.png"],
        position: { x: 60, y: 300 },
      },
    });
    f.fixture.hovered = surface("skins", 540);
    f.fileDrop({ payload: { type: "over", position: { x: 600, y: 300 } } });
    inspected.reject(Error("old inspection failed"));
    await flush();
    assert.equal(dropHint.value.left, 540);
    assert.equal(dropHint.value.message, "导入皮肤");
    assert.equal(dropHint.value.invalid, false);
    f.fileDrop({ payload: { type: "leave" } });
  } finally {
    f.dispose();
  }
});

test("native type mismatches stay invisible before and after inspection and never fall back on drop", async () => {
  const f = await settingsFixture();
  try {
    const inspected = deferred(),
      drops = [];
    f.handlers.inspect_resources = () => inspected.promise;
    f.context.resourceManager.value = {
      drop: async (...args) => drops.push(args),
    };
    f.fixture.hovered = surface("resourcepacks", 20);
    const payload = { paths: ["/cat.jar"], position: { x: 60, y: 300 } };
    f.fileDrop({ payload: { ...payload, type: "enter" } });
    assert.equal(
      dropHint.value,
      undefined,
      "no identifying outline before knowing the type",
    );
    inspected.resolve([
      {
        path: "/cat.jar",
        name: "Cat",
        format: "resource",
        kinds: ["mods"],
        issue: null,
      },
    ]);
    await flush();
    assert.equal(dropHint.value, undefined);
    f.context.busy.value = true;
    for (const kind of [
      "resourcepacks",
      "shaderpacks",
      "skins",
      "java",
      "config",
      "metadata",
      "blocked",
      "selection",
    ]) {
      f.fixture.hovered = surface(kind, 20);
      f.fileDrop({ payload: { type: "over", position: payload.position } });
      await flush();
      assert.equal(dropHint.value, undefined, kind);
    }
    f.fixture.hovered = surface("resourcepacks", 20);
    f.fileDrop({ payload: { ...payload, type: "drop" } });
    await flush();
    assert.equal(dropHint.value, undefined);
    assert.deepEqual(drops, []);
    assert.equal(f.context.modal.value, null);
    assert.equal(
      f.calls.some((call) =>
        ["preview_resource_drop", "preview_modpack", "scan_folders"].includes(
          call.op,
        ),
      ),
      false,
    );
  } finally {
    f.dispose();
  }
});

test("matching native resources retain busy errors and switch to a quiet wrong-type target", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.inspect_resources = () => [
      {
        path: "/cat.jar",
        name: "Cat",
        format: "resource",
        kinds: ["mods"],
        issue: null,
      },
    ];
    f.context.busy.value = true;
    f.fixture.hovered = surface("mods", 20);
    f.fileDrop({
      payload: {
        type: "enter",
        paths: ["/cat.jar"],
        position: { x: 60, y: 300 },
      },
    });
    await flush();
    assert.equal(dropHint.value.invalid, true);
    assert.match(dropHint.value.message, /当前任务/);
    f.fixture.hovered = surface("skins", 540);
    f.fileDrop({ payload: { type: "over", position: { x: 600, y: 300 } } });
    assert.equal(dropHint.value, undefined);
    f.fileDrop({ payload: { type: "leave" } });
  } finally {
    f.dispose();
  }
});
