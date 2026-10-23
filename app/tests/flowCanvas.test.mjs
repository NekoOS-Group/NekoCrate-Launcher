import assert from "node:assert/strict";
import test from "node:test";
import { createBackdropGraph } from "../src/flowGraph.ts";
import { createHexFlow, advanceHexFlow } from "../src/hexFlow.ts";
import {
  createFlowPaintState,
  flowCellColor,
  parseFlowColor,
  roundFlowValue,
} from "../src/flowPainter.ts";
import { createFlowCanvas } from "../src/flowCanvas.ts";
import { createFlowPainter } from "./helpers/svgFlowPainter.ts";

function random() {
  let seed = 12345;
  return () =>
    (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
}
function targets(count) {
  return Array.from({ length: count }, () => [
    {
      attrs: {},
      style: { fill: "" },
      setAttribute(key, value) {
        this.attrs[key] = value;
      },
    },
  ]);
}
const palette = {
  cyan: parseFlowColor("#007ea7"),
  blue: parseFlowColor("#80ced7"),
  aqua: parseFlowColor("#9ad1d4"),
};
test("数值舍入与 toFixed 一致，包括每个半边界两侧相邻的浮点数", () => {
  const bytes = new DataView(new ArrayBuffer(8));
  function adjacent(value, direction) {
    bytes.setFloat64(0, value);
    bytes.setBigUint64(0, bytes.getBigUint64(0) + BigInt(direction));
    return bytes.getFloat64(0);
  }
  for (const digits of [2, 3]) {
    const scale = 10 ** digits;
    for (let i = 0; i < 4000; i++) {
      const half = (i + 0.5) / scale;
      for (const value of [
        half,
        adjacent(half, -1),
        adjacent(half, 1),
        Math.fround(half),
      ])
        assert.equal(
          roundFlowValue(value, digits),
          Number(value.toFixed(digits)),
        );
    }
    const next = random();
    for (let i = 0; i < 100000; i++) {
      const value = next() * 4;
      assert.equal(
        roundFlowValue(value, digits),
        Number(value.toFixed(digits)),
      );
    }
  }
});
for (const kind of ["honeycomb", "chunks", "diamonds", "facets", "circuit"]) {
  test(`${kind}: Canvas 每帧线宽、透明度、混色比例与原 SVG 完全一致`, () => {
    const graph = createBackdropGraph(kind);
    const flow = createHexFlow(graph, random());
    const edges = targets(graph.edges.length),
      cells = targets(graph.cells.length);
    const svg = createFlowPainter(edges, cells);
    const state = createFlowPaintState(edges.length, cells.length);
    for (let frame = 0; frame < 360; frame++) {
      advanceHexFlow(flow, [1 / 60, 1 / 120, 0.04][frame % 3]);
      const still = frame >= 119 && frame <= 121;
      svg(flow, still);
      state.update(flow, still);
      edges.forEach(([edge], i) => {
        assert.equal(state.intensity[i], Number(edge.attrs["stroke-opacity"]));
        assert.equal(state.width[i], Number(edge.attrs["stroke-width"]));
      });
      cells.forEach(([cell], i) => {
        assert.equal(state.opacity[i], Number(cell.attrs["fill-opacity"]));
        if (!cell.style.fill) {
          assert.equal(state.cyanMix[i], -1);
          return;
        }
        const ratios = [...cell.style.fill.matchAll(/([\d.]+)%/g)].map((m) =>
          Number(m[1]),
        );
        assert.equal(state.cyanMix[i] / 2, ratios[0]);
        assert.equal(
          state.blueMix[i] < 0 ? 50 : state.blueMix[i] / 2,
          ratios[1],
        );
      });
    }
  });
}

test("sRGB 混色保留三个色源及未取整的中间色", () => {
  assert.equal(flowCellColor(palette, 200, 0), "rgb(0 126 167)");
  assert.equal(flowCellColor(palette, 0, 200), "rgb(128 206 215)");
  assert.equal(flowCellColor(palette, 0, 0), "rgb(154 209 212)");
  assert.equal(flowCellColor(palette, 100, 100), "rgb(70.5 166.75 190.25)");
});

test("仅绘制可见路径；DPR、跨周期和移入窗口的状态与完整图一致", (t) => {
  const previousPath = globalThis.Path2D;
  globalThis.Path2D = class {
    constructor(path) {
      this.path = path;
    }
  };
  t.after(() => {
    if (previousPath) globalThis.Path2D = previousPath;
    else delete globalThis.Path2D;
  });
  const calls = [];
  const context = {
    setTransform(...args) {
      calls.push(["transform", ...args]);
    },
    clearRect() {},
    save() {},
    restore() {},
    beginPath() {},
    clip() {},
    rect(...args) {
      calls.push(["clip", ...args]);
    },
    fill(path) {
      calls.push(["fill", path]);
    },
    stroke(path) {
      calls.push(["stroke", path, this.globalAlpha, this.lineWidth]);
    },
  };
  const output = { width: 0, height: 0, getContext: () => context };
  const graph = createBackdropGraph("honeycomb");
  const flow = createHexFlow(graph, random());
  const state = createFlowPaintState(graph.edges.length, graph.cells.length);
  const indexes = new Map(graph.edges.map((edge, i) => [edge.path, i]));
  const renderer = createFlowCanvas(output, graph, palette);
  const sizes = [
    [900, 600, 1, 0],
    [2560, 1440, 2, 70],
    [900, 600, 1.25, graph.width / 10],
    [100, 100, 2, graph.width / 20],
  ];
  for (const [width, height, ratio, seconds] of sizes) {
    calls.length = 0;
    renderer.resize(width, height, ratio);
    for (let i = 0; i < 120; i++) advanceHexFlow(flow, 1 / 60);
    state.update(flow);
    renderer.paint(flow, seconds);
    assert.equal(output.width, Math.ceil(width * ratio));
    assert.equal(output.height, Math.ceil(height * ratio));
    const strokes = calls.filter((c) => c[0] === "stroke");
    assert.ok(strokes.length > 0);
    if (width <= 900) assert.ok(strokes.length < graph.edges.length);
    for (const [, path, alpha, lineWidth] of strokes) {
      const index = indexes.get(path.path);
      assert.equal(alpha, state.intensity[index]);
      assert.equal(lineWidth, state.width[index]);
    }
    const phase = ((seconds * 10) / graph.width) % 1;
    const transforms = calls.filter((c) => c[0] === "transform").slice(1);
    const scaleX = output.width / width,
      scaleY = output.height / height;
    assert.deepEqual(transforms[0], [
      "transform",
      scaleX,
      0,
      0,
      scaleY,
      -phase * graph.width * scaleX,
      -phase * graph.height * scaleY,
    ]);
    let covered = 0;
    for (const transform of transforms) {
      const x = transform[5] / scaleX,
        y = transform[6] / scaleY;
      const w = Math.min(width, x + graph.width) - Math.max(0, x);
      const h = Math.min(height, y + graph.height) - Math.max(0, y);
      assert.ok(w > 0 && h > 0);
      covered += w * h;
    }
    assert.ok(Math.abs(covered - width * height) < 1e-5);
    assert.equal(
      calls.filter((c) => c[0] === "clip").length,
      transforms.length,
    );
  }
  calls.length = 0;
  renderer.paint(flow, 55, true);
  assert.ok(
    calls
      .filter((c) => c[0] === "stroke")
      .every(
        (c) => Math.abs(c[2] - 0.13) < 0.004 && Math.abs(c[3] - 0.65) < 0.015,
      ),
  );
  assert.deepEqual(
    calls.filter((c) => c[0] === "transform")[1].slice(5),
    [-0, -0],
  );
  renderer.dispose();
  assert.equal(output.width + output.height, 0);
});
