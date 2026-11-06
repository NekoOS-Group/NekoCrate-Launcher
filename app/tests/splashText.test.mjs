import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  unicodeGlyph,
  splashPulse,
  splashScale,
  splashRotation,
  paintSplash,
} from "../src/splashText.ts";
test("splash rotation, sine pulse and width normalization match the vanilla renderer", () => {
  assert.equal(splashRotation, -20);
  assert.equal(splashPulse(0), 1.8);
  assert.equal(splashPulse(250), 1.7);
  assert.equal(splashPulse(500), 1.8);
  assert.equal(splashScale(68), 1);
});
test("Unifont retains half-pixel coverage and fixed CJK advances; bitmap and shadow are only painted on content changes", () => {
  const sample = JSON.parse(
    readFileSync(
      new URL("../src/assets/fonts/unifont-sample.json", import.meta.url),
    ),
  );
  const glyph = unicodeGlyph("祸", sample["祸"]);
  assert.equal(glyph[0], 9);
  assert.equal(glyph[1], 0.5);
  assert.equal(glyph[3], 0.5);
  assert.equal(glyph[4], 16);
  assert.equal(unicodeGlyph("A", "invalid"), undefined);
  const wide = unicodeGlyph("😀", "f".repeat(128));
  assert.equal(wide[4], 32);
  assert.ok(!wide[6].includes("-"));
  const pixels = [];
  const ctx = { fillRect: (...args) => pixels.push(args) };
  const canvas = { getContext: () => ctx };
  const size = paintSplash(canvas, "祸", { 祸: glyph });
  assert.equal(size.width, 9);
  assert.ok(pixels.length > 0);
  assert.equal(ctx.fillStyle, "#ffff00");
  const bitmap = JSON.parse(
    readFileSync(
      new URL("../src/assets/fonts/minecraft-bitmap.json", import.meta.url),
    ),
  );
  assert.equal(bitmap[" "][0], 4);
  assert.equal(bitmap.A[0], 6);
  for (const glyph of Object.values(bitmap))
    assert.equal(glyph[6].length, (glyph[4] * glyph[5]) / 4);
});
