import assert from "node:assert/strict";
import test from "node:test";
import { createSkinOutline } from "../src/skinOutline.ts";
import { createDemandRenderer } from "../src/demandRenderer.ts";

test("Tab 聚焦和离开静止皮肤都触发重绘，鼠标拖动不残留描边", () => {
  const document = new EventTarget();
  const canvas = new EventTarget();
  canvas.ownerDocument = document;
  let visible = false;
  canvas.matches = () => visible;
  let pass,
    frame,
    renders = [];
  const drawing = createDemandRenderer(
    () => false,
    () => renders.push(pass.enabled),
    {
      request(callback) {
        frame = callback;
        return 1;
      },
      cancel() {
        frame = null;
      },
    },
  );
  const viewer = {
    composer: {
      addPass(value) {
        pass = value;
      },
      removePass(value) {
        assert.equal(value, pass);
      },
    },
    renderer: {
      getDrawingBufferSize(size) {
        return size.set(600, 800);
      },
      getPixelRatio() {
        return 2;
      },
    },
  };
  const outline = createSkinOutline(viewer, canvas, drawing.invalidate);
  assert.equal(pass.enabled, false);
  assert.equal(frame, undefined);
  document.activeElement = canvas;
  visible = true;
  canvas.dispatchEvent(new Event("focus"));
  frame();
  frame = null;
  assert.deepEqual(renders, [true]);
  canvas.dispatchEvent(new Event("pointerdown"));
  frame();
  frame = null;
  assert.deepEqual(renders, [true, false]);
  document.dispatchEvent(new Event("keydown"));
  frame();
  frame = null;
  assert.deepEqual(renders, [true, false, true]);
  document.activeElement = null;
  canvas.dispatchEvent(new Event("blur"));
  frame();
  frame = null;
  assert.deepEqual(renders, [true, false, true, false]);
  let disposed = false;
  pass.material.addEventListener("dispose", () => (disposed = true));
  outline.dispose();
  canvas.dispatchEvent(new Event("focus"));
  assert.equal(frame, null);
  assert.equal(disposed, true);
  drawing.dispose();
});

test("异步加载前已聚焦也会显示轮廓；窗口与 DPR 变化保持 CSS 描边宽度", () => {
  const canvas = new EventTarget();
  canvas.ownerDocument = new EventTarget();
  canvas.matches = () => true;
  let pass,
    ratio = 1,
    width = 300,
    height = 400,
    invalidations = 0;
  const outline = createSkinOutline(
    {
      composer: {
        addPass(value) {
          pass = value;
        },
        removePass() {},
      },
      renderer: {
        getDrawingBufferSize(size) {
          return size.set(width * ratio, height * ratio);
        },
        getPixelRatio() {
          return ratio;
        },
      },
    },
    canvas,
    () => invalidations++,
  );
  assert.equal(pass.enabled, true);
  assert.equal(invalidations, 1);
  const first = pass.uniforms.stepSize.value.clone();
  ratio = 2;
  outline.resize();
  assert.deepEqual(pass.uniforms.stepSize.value, first);
  width *= 2;
  outline.resize();
  assert.equal(pass.uniforms.stepSize.value.x, first.x / 2);
  outline.dispose();
});
