import assert from "node:assert/strict";
import test from "node:test";
import { createSkinCarousel } from "../src/skinCarousel.ts";

const flush = () => new Promise(setImmediate);
function fixture(t) {
  const original = globalThis.getComputedStyle;
  globalThis.getComputedStyle = () => ({ transform: "none", opacity: "1" });
  t.after(() => {
    globalThis.getComputedStyle = original;
  });
  const animations = [];
  const canvas = {
    style: { removeProperty() {} },
    animate(frames, options) {
      let finish, cancel;
      const animation = {
        frames,
        options,
        paused: false,
        finished: new Promise((resolve, reject) => {
          finish = resolve;
          cancel = reject;
        }),
        finish: () => finish(),
        cancel: () => cancel(new Error("cancelled")),
        pause() {
          this.paused = true;
        },
        play() {
          this.paused = false;
        },
      };
      animations.push(animation);
      return animation;
    },
  };
  return { motion: createSkinCarousel(canvas), animations };
}

test("texture changes only between exit and entrance; rapid choices cannot apply cancelled skins", async (t) => {
  const { motion, animations } = fixture(t),
    applied = [];
  const first = motion.show(() => applied.push("first"), 1, true);
  assert.deepEqual(applied, []);
  assert.match(animations[0].frames[1].transform, /-96px/);
  const second = motion.show(() => applied.push("second"), -1, true);
  await first;
  assert.deepEqual(applied, []);
  assert.match(animations[1].frames[1].transform, /translate3d\(96px/);
  animations[1].finish();
  await flush();
  assert.deepEqual(applied, ["second"]);
  assert.match(animations[2].frames[0].transform, /-96px/);
  animations[2].finish();
  await second;
  assert.equal(motion.active, false);
});

test("hidden carousel pauses both phases; reduced motion applies immediately and dispose cancels pending changes", async (t) => {
  const { motion, animations } = fixture(t),
    applied = [];
  motion.setPaused(true);
  const changing = motion.show(() => applied.push("animated"), 1, true);
  assert.equal(animations[0].paused, true);
  motion.setPaused(false);
  assert.equal(animations[0].paused, false);
  await motion.show(() => applied.push("static"), 1, false);
  await changing;
  assert.deepEqual(applied, ["static"]);
  const discarded = motion.show(() => applied.push("discarded"), 1, true);
  motion.dispose();
  await discarded;
  await motion.show(() => applied.push("after disposal"), 1, false);
  assert.deepEqual(applied, ["static"]);
});
