import assert from "node:assert/strict";
import test from "node:test";
import {
  createInstanceWheelStepper,
  instanceWheelFrame,
  readInstanceWheelBounds,
  resizeInstanceWheelBounds,
  defaultInstanceWheelBounds,
} from "../src/instanceWheel.ts";

test("wheel notches select once, trackpad pixels accumulate, momentum is bounded and stale input expires", () => {
  const wheel = createInstanceWheelStepper();
  assert.equal(wheel.step(3, 1, 0), 1);
  assert.equal(wheel.step(120, 0, 30), 0);
  assert.equal(wheel.step(120, 0, 100), 1);
  wheel.reset();
  for (let i = 0; i < 3; i++) assert.equal(wheel.step(10, 0, i * 10), 0);
  assert.equal(wheel.step(10, 0, 30), 1);
  assert.equal(wheel.step(-40, 0, 140), -1);
  wheel.reset();
  assert.equal(wheel.step(20, 0, 0), 0);
  assert.equal(wheel.step(20, 0, 300), 0);
  assert.equal(wheel.step(20, 0, 320), 1);
  assert.equal(wheel.step(1, 2, 430), 1);
  assert.equal(wheel.step(NaN, 0, 600), 0);
});

test("expanded picker stays inside every window corner while preserving the collapsed anchor", () => {
  for (const viewport of [
    { width: 760, height: 500 },
    { width: 1100, height: 760 },
  ]) {
    for (const left of [12, viewport.width / 2, viewport.width - 76])
      for (const top of [12, viewport.height / 2, viewport.height - 76]) {
        const anchor = { left, top, width: 64, height: 64 };
        const p = instanceWheelFrame(anchor, viewport);
        assert.ok(
          p.left + left >= 12 && p.left + left + p.width <= viewport.width - 12,
        );
        assert.ok(
          p.top + top >= 12 && p.top + top + p.height <= viewport.height - 12,
        );
        assert.ok(p.iconX >= 32 && p.iconX <= p.width - 32);
        assert.equal(p.left + p.iconX, anchor.width / 2);
        assert.equal(p.top + p.iconY, anchor.height / 2);
        assert.equal(p.leftLabels, left + 32 > viewport.width / 2);
      }
  }
});

test("independent visible edges resize from the clipped boundary without moving the selected anchor", () => {
  const anchor = { left: 600, top: 300, width: 64, height: 64 },
    viewport = { width: 760, height: 500 };
  const bounds = { above: 180, below: 300 };
  const resized = resizeInstanceWheelBounds(
    bounds,
    "bottom",
    -40,
    anchor,
    viewport,
  );
  assert.deepEqual(resized, { above: 180, below: 116 });
  const before = instanceWheelFrame(anchor, viewport, bounds),
    after = instanceWheelFrame(anchor, viewport, resized);
  assert.equal(before.height - after.height, 40);
  assert.equal(before.top, after.top);
  assert.equal(after.top + after.iconY, 32);
  assert.deepEqual(
    resizeInstanceWheelBounds(resized, "top", -60, anchor, viewport),
    { above: 240, below: 116 },
  );
  assert.equal(
    resizeInstanceWheelBounds(bounds, "top", 1000, anchor, viewport).above,
    72,
  );
  assert.equal(
    resizeInstanceWheelBounds(bounds, "bottom", 1000, anchor, viewport).below,
    156,
  );
});

test("wheel boundaries accept older layouts and sanitize invalid persisted values", () => {
  assert.deepEqual(readInstanceWheelBounds(null), defaultInstanceWheelBounds);
  assert.deepEqual(readInstanceWheelBounds("{"), defaultInstanceWheelBounds);
  assert.deepEqual(
    readInstanceWheelBounds(
      JSON.stringify({
        version: 1,
        instanceWheel: { above: -20, below: 9999 },
      }),
    ),
    { above: 72, below: 600 },
  );
  assert.deepEqual(
    readInstanceWheelBounds(
      JSON.stringify({
        version: 1,
        instanceWheel: { above: "100", below: null },
      }),
    ),
    defaultInstanceWheelBounds,
  );
});
