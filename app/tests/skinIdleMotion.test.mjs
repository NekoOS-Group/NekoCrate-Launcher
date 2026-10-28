import assert from "node:assert/strict";
import test from "node:test";
import { createSkinIdleMotion } from "../src/skinIdleMotion.ts";

test("idle motion shares redraws, cancels wakeups while hidden and resumes without a time jump", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const skin = Object.fromEntries(
    ["leftArm", "rightArm", "leftLeg", "rightLeg", "head"].map((name) => [
      name,
      { rotation: { x: 0, y: 0, z: 0 } },
    ]),
  );
  let requests = 0;
  const motion = createSkinIdleMotion(skin, () => requests++);
  motion.update(0);
  assert.equal(requests, 0);
  motion.setPaused(false);
  assert.equal(requests, 1);
  motion.update(100);
  // Camera input and its damping can request extra frames without extra timers.
  for (let now = 110; now <= 130; now += 10) motion.update(now);
  t.mock.timers.tick(32);
  assert.equal(requests, 1);
  t.mock.timers.tick(2);
  assert.equal(requests, 2);
  motion.update(140);
  const pose = structuredClone(skin);
  motion.setPaused(true);
  t.mock.timers.tick(60_000);
  motion.update(60_000);
  assert.equal(requests, 2);
  assert.deepEqual(skin, pose);
  motion.setPaused(false);
  motion.update(60_100);
  assert.deepEqual(skin, pose);
  motion.update(60_134);
  assert.notDeepEqual(skin, pose);
  motion.dispose();
  const last = requests;
  t.mock.timers.tick(60_000);
  motion.setPaused(false);
  motion.update(120_200);
  assert.equal(requests, last);
});
