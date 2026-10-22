import assert from "node:assert/strict";
import test from "node:test";
import {
  createInstanceLanding,
  instanceTranslation,
} from "../src/instanceMotion.ts";

function fixture() {
  let center = { x: 300, y: 400 };
  const animations = [];
  const element = {
    style: {},
    getBoundingClientRect: () => ({
      left: center.x - 28,
      top: center.y - 28,
      width: 56,
      height: 56,
    }),
    animate(frames, options) {
      let resolve, reject;
      const animation = {
        frames,
        options,
        cancelled: false,
        finished: new Promise((yes, no) => {
          resolve = yes;
          reject = no;
        }),
        finish() {
          resolve();
        },
        cancel() {
          this.cancelled = true;
          reject(new Error("cancelled"));
        },
      };
      animations.push(animation);
      return animation;
    },
  };
  return {
    element,
    animations,
    position: (value) => {
      center = value;
    },
  };
}

test("左右拖放均从当前视觉坐标归位，完成前不隐藏预览", async () => {
  for (const target of [
    { x: 120, y: 380 },
    { x: 480, y: 380 },
  ]) {
    const { element, animations } = fixture();
    let completed = 0;
    const motion = createInstanceLanding(element, () => completed++);
    motion.moveTo(target);
    assert.equal(completed, 0);
    assert.equal(
      animations[0].frames[0].transform,
      instanceTranslation({ x: 300, y: 400 }),
    );
    assert.equal(
      animations[0].frames[1].transform,
      instanceTranslation(target),
    );
    assert.equal(element.style.transform, instanceTranslation(target));
    assert.equal(animations[0].options.easing, "cubic-bezier(0.16, 1, 0.3, 1)");
    animations[0].finish();
    await Promise.resolve();
    assert.equal(completed, 1);
  }
});

test("保存失败回滚时从动画途中改道，旧动画不能提前结束归位", async () => {
  const { element, animations, position } = fixture();
  let completed = 0;
  const motion = createInstanceLanding(element, () => completed++);
  motion.moveTo({ x: 120, y: 380 });
  position({ x: 220, y: 390 });
  motion.moveTo({ x: 360, y: 380 });
  assert.equal(animations[0].cancelled, true);
  assert.equal(
    animations[1].frames[0].transform,
    instanceTranslation({ x: 220, y: 390 }),
  );
  animations[0].finish();
  await Promise.resolve();
  assert.equal(completed, 0);
  animations[1].finish();
  await Promise.resolve();
  assert.equal(completed, 1);
});

test("减少动态效果直接归位，销毁后不调用完成或创建后续动画", async () => {
  const { element, animations } = fixture();
  let completed = 0;
  const reduced = createInstanceLanding(element, () => completed++, true);
  reduced.moveTo({ x: 200, y: 380 });
  assert.equal(completed, 1);
  assert.equal(animations.length, 0);
  const motion = createInstanceLanding(element, () => completed++);
  motion.moveTo({ x: 120, y: 380 });
  motion.dispose();
  animations[0].finish();
  await Promise.resolve();
  motion.moveTo({ x: 480, y: 380 });
  assert.equal(completed, 1);
  assert.equal(animations.length, 1);
});
