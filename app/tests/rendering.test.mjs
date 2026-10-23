import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createBackdropGraph } from "../src/flowGraph.ts";
import { createHexFlow, advanceHexFlow } from "../src/hexFlow.ts";
import { createFlowPainter } from "./helpers/svgFlowPainter.ts";
import { createDemandRenderer } from "../src/demandRenderer.ts";

const fixture = JSON.parse(
  readFileSync(
    new URL("./fixtures/rendering-before-optimization.json", import.meta.url),
    "utf8",
  ),
);
function targets(count) {
  return Array.from({ length: count }, () => [
    {
      attrs: {},
      style: { fill: "" },
      setAttribute(name, value) {
        this.attrs[name] = value;
      },
    },
  ]);
}
for (const [kind, expected] of Object.entries(fixture.themes)) {
  test(`${kind}：优化后逐帧模拟、SVG 颜色和线宽与原版记录完全一致`, () => {
    let seed = fixture.seed;
    const random = () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return seed / 4294967296;
    };
    const graph = createBackdropGraph(kind);
    const flow = createHexFlow(graph, random);
    const paths = targets(graph.edges.length),
      cells = targets(graph.cells.length);
    const paint = createFlowPainter(paths, cells);
    const hash = createHash("sha256");
    for (let frame = 0; frame < fixture.frames; frame++) {
      advanceHexFlow(flow, fixture.steps[frame % fixture.steps.length]);
      paint(flow, frame === 119 || frame === 120);
      hash.update(
        JSON.stringify({
          time: flow.time,
          occupancy: [...flow.occupancy],
          blocked: [...flow.blocked],
          intensity: [...flow.intensity],
          widths: [...flow.widths],
          walkers: flow.walkers,
          dye: [...flow.dye],
          sources: [...flow.sources],
          regions: flow.regions,
          paths: paths.map((c) => c[0].attrs),
          cells: cells.map((c) => ({
            attrs: c[0].attrs,
            fill: c[0].style.fill,
          })),
        }),
      );
    }
    assert.equal(hash.digest("hex"), expected);
  });
}

function frameClock() {
  let id = 0;
  const pending = new Map();
  return {
    request(callback) {
      pending.set(++id, callback);
      return id;
    },
    cancel(frame) {
      pending.delete(frame);
    },
    step() {
      const batch = [...pending.values()];
      pending.clear();
      for (const callback of batch) callback(0);
    },
    get size() {
      return pending.size;
    },
  };
}
test("静止皮肤仅绘制一次；多次纹理／尺寸请求合并到下一帧", () => {
  const clock = frameClock();
  let renders = 0;
  const drawing = createDemandRenderer(
    () => false,
    () => renders++,
    clock,
  );
  for (let i = 0; i < 20; i++) drawing.invalidate();
  assert.equal(clock.size, 1);
  for (let i = 0; i < 600; i++) clock.step();
  assert.equal(renders, 1);
  assert.equal(clock.size, 0);
  drawing.invalidate();
  clock.step();
  assert.equal(renders, 2);
});
test("拖拽惯性逐帧完成，controls change 与更新返回值不产生双重循环", () => {
  const clock = frameClock();
  let movement = 30,
    renders = 0;
  const drawing = createDemandRenderer(
    () => {
      if (!movement) return false;
      movement--;
      drawing.invalidate(); // OrbitControls change event during its update.
      return true;
    },
    () => renders++,
    clock,
  );
  drawing.invalidate();
  for (let i = 0; i < 30; i++) {
    clock.step();
    assert.equal(clock.size, 1);
  }
  clock.step();
  assert.equal(movement, 0);
  assert.equal(renders, 31);
  assert.equal(clock.size, 0);
});
test("隐藏／弹窗／丢失上下文时暂停，恢复补绘，销毁后不再绘制", () => {
  const clock = frameClock();
  let renders = 0;
  const drawing = createDemandRenderer(
    () => false,
    () => renders++,
    clock,
  );
  drawing.invalidate();
  drawing.setPaused(true);
  drawing.invalidate();
  clock.step();
  assert.equal(renders, 0);
  drawing.setPaused(false);
  clock.step();
  assert.equal(renders, 1);
  drawing.invalidate();
  drawing.dispose();
  drawing.setPaused(false);
  drawing.invalidate();
  clock.step();
  assert.equal(renders, 1);
  assert.equal(clock.size, 0);
});
