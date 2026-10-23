import assert from "node:assert/strict";
import test from "node:test";
import { createBackdropGraph } from "../src/flowGraph.ts";
import { createHexFlow, advanceHexFlow } from "../src/hexFlow.ts";
import { createFlowPaintState } from "../src/flowPainter.ts";
import {
  createFlowGeometry,
  createFlowBatchGeometry,
  flowPolygons,
} from "../src/flowGeometry.ts";
import { createFlowWebGL } from "../src/flowWebGL.ts";

const palette = {
  cyan: [0, 126, 167],
  blue: [128, 206, 215],
  aqua: [154, 209, 212],
};
function fixture(instanced = true) {
  const calls = [],
    buffers = [],
    programs = [],
    arrays = [];
  let bound,
    program,
    tileOffset = [0, 0],
    lost = false;
  const instances = {
    vertexAttribDivisorANGLE() {},
    drawArraysInstancedANGLE(mode, start, count, instanceCount) {
      calls.push({
        program,
        mode,
        start,
        count,
        instanceCount,
        drift: program.uniforms.drift.slice(),
      });
    },
  };
  const gl = {
    ARRAY_BUFFER: 1,
    DYNAMIC_DRAW: 2,
    STATIC_DRAW: 3,
    FLOAT: 4,
    TRIANGLES: 5,
    FRAGMENT_SHADER: 6,
    VERTEX_SHADER: 7,
    HIGH_FLOAT: 8,
    LINK_STATUS: 9,
    DEPTH_TEST: 10,
    CULL_FACE: 11,
    DITHER: 12,
    BLEND: 13,
    ONE: 14,
    ONE_MINUS_SRC_ALPHA: 15,
    COLOR_BUFFER_BIT: 16,
    getExtension(name) {
      return name === "ANGLE_instanced_arrays"
        ? instanced
          ? instances
          : null
        : name === "WEBGL_lose_context"
          ? {
              loseContext() {
                lost = true;
              },
            }
          : {};
    },
    getShaderPrecisionFormat() {
      return { precision: 23 };
    },
    createProgram() {
      const p = { attributes: new Map(), uniforms: {} };
      programs.push(p);
      return p;
    },
    createShader() {
      return {};
    },
    shaderSource() {},
    compileShader() {},
    attachShader() {},
    linkProgram() {},
    getProgramParameter() {
      return true;
    },
    deleteShader() {},
    deleteProgram(p) {
      p.deleted = true;
    },
    createBuffer() {
      const b = {};
      buffers.push(b);
      return b;
    },
    bindBuffer(_, b) {
      bound = b;
    },
    bufferData(_, data) {
      bound.data = data.slice();
      calls.push("allocate");
    },
    bufferSubData(_, offset, data) {
      assert.equal(offset, 0);
      bound.data.set(data);
      calls.push("upload");
    },
    deleteBuffer(b) {
      b.deleted = true;
    },
    useProgram(p) {
      program = p;
    },
    bindAttribLocation(p, location, name) {
      p.attributes.set(name, location);
    },
    getAttribLocation(p, name) {
      if (!p.attributes.has(name)) p.attributes.set(name, p.attributes.size);
      return p.attributes.get(name);
    },
    getUniformLocation(p, name) {
      return { p, name };
    },
    uniform2f(location, ...values) {
      location.p.uniforms[location.name] = values;
    },
    uniform3f() {},
    disableVertexAttribArray() {},
    enableVertexAttribArray() {},
    vertexAttribPointer() {},
    vertexAttrib2f(_, x, y) {
      tileOffset = [x, y];
    },
    disable() {},
    enable() {},
    blendFunc() {},
    clearColor() {},
    viewport(...values) {
      calls.push(["viewport", ...values]);
    },
    clear() {},
    drawArrays(mode, start, count) {
      calls.push({
        program,
        mode,
        start,
        count,
        offset: tileOffset.slice(),
        drift: program.uniforms.drift.slice(),
      });
    },
    isContextLost() {
      return lost;
    },
  };
  const canvas = {
    width: 0,
    height: 0,
    getContext(kind, options) {
      assert.equal(kind, "webgl");
      assert.equal(options.antialias, false);
      return gl;
    },
  };
  return {
    canvas,
    gl,
    calls,
    buffers,
    programs,
    arrays,
    setLost(value) {
      lost = value;
    },
  };
}

for (const kind of ["honeycomb", "chunks", "diamonds", "facets", "circuit"]) {
  test(`${kind}: GPU 常驻几何保留全部路径和顺序，动态数据与旧绘制状态一致`, () => {
    const graph = createBackdropGraph(kind),
      geometry = createFlowGeometry(graph);
    const batch = createFlowBatchGeometry(graph);
    assert.equal(batch.vertices.length, batch.ids.length * 11);
    assert.deepEqual(
      [...batch.ids],
      [...geometry.fillIds, ...geometry.lineIds],
    );
    const expectedFills = [
      ...graph.washes.flatMap((p) => flowPolygons(p).map(() => -1)),
      ...graph.cells.flatMap((c, i) => flowPolygons(c.path).map(() => i)),
    ];
    const expectedLines = graph.edges.flatMap((e, i) =>
      flowPolygons(e.path).map(() => i),
    );
    assert.deepEqual(
      [...geometry.fillIds],
      expectedFills.flatMap((i) => Array(6).fill(i)),
    );
    assert.deepEqual(
      [...geometry.lineIds],
      expectedLines.flatMap((i) => Array(6).fill(i)),
    );
    assert.equal(geometry.fills.length, geometry.fillIds.length * 10);
    assert.equal(geometry.lines.length, geometry.lineIds.length * 6);
    const f = fixture();
    const renderer = createFlowWebGL(f.canvas, graph, palette);
    const flow = createHexFlow(graph, () => 0.37),
      state = createFlowPaintState(graph.edges.length, graph.cells.length);
    renderer.resize(900, 600, 2);
    assert.equal(f.canvas.width, 1800);
    assert.equal(f.canvas.height, 1200);
    const allocations = f.calls.filter((c) => c === "allocate").length;
    for (let frame = 0; frame < 90; frame++) {
      advanceHexFlow(flow, frame % 2 ? 1 / 60 : 1 / 120);
      const still = frame === 44;
      state.update(flow, still);
      renderer.paint(flow, frame / 60, still);
      geometry.fillIds.forEach((id, i) => {
        assert.equal(
          f.buffers[1].data[i * 4],
          Math.fround(id < 0 ? 0 : state.cyanMix[id] / 200),
        );
        assert.equal(
          f.buffers[1].data[i * 4 + 1],
          Math.fround(
            id < 0 ? 0 : state.blueMix[id] < 0 ? 0.5 : state.blueMix[id] / 200,
          ),
        );
        assert.equal(
          f.buffers[1].data[i * 4 + 2],
          Math.fround(id < 0 ? 0.025 : state.opacity[id]),
        );
      });
      geometry.lineIds.forEach((id, i) => {
        assert.equal(
          f.buffers[1].data[(batch.fillCount + i) * 4],
          Math.fround(state.width[id]),
        );
        assert.equal(
          f.buffers[1].data[(batch.fillCount + i) * 4 + 2],
          Math.fround(state.intensity[id]),
        );
      });
    }
    assert.equal(f.calls.filter((c) => c === "allocate").length, allocations);
    assert.equal(f.calls.filter((c) => c === "upload").length, 90);
    renderer.dispose();
    assert.ok(f.buffers.every((b) => b.deleted));
    assert.ok(f.programs.every((p) => p.deleted));
    assert.ok(f.arrays.every((a) => a.deleted));
    assert.equal(f.canvas.width + f.canvas.height, 0);
  });
}

test("整窗每帧一次上传和绘制；精确周期、DPR、无实例化回退及上下文恢复", () => {
  const graph = createBackdropGraph("honeycomb"),
    flow = createHexFlow(graph, () => 0.37);
  for (const instanced of [true, false]) {
    const f = fixture(instanced),
      renderer = createFlowWebGL(f.canvas, graph, palette);
    for (const [w, h, ratio, seconds] of [
      [900, 600, 1.25, 20],
      [2560, 1440, 2, graph.width / 10],
      [900, 600, 2, graph.width / 10 - 0.001],
    ]) {
      renderer.resize(w, h, ratio);
      f.calls.length = 0;
      renderer.paint(flow, seconds);
      const draws = f.calls.filter(
        (c) => typeof c === "object" && !Array.isArray(c),
      );
      const phase = ((seconds * 10) / graph.width) % 1;
      const count =
        (Math.ceil(w / graph.width) + 1) * (Math.ceil(h / graph.height) + 1);
      assert.equal(draws.length, instanced ? 1 : count);
      if (instanced) assert.equal(draws[0].instanceCount, count);
      for (const draw of draws)
        assert.deepEqual(draw.drift, [
          -phase * graph.width,
          -phase * graph.height,
        ]);
      assert.equal(f.calls.filter((c) => c === "upload").length, 1);
      assert.deepEqual(f.programs[0].uniforms.pixelScale, [
        Math.ceil(w * ratio) / w,
        Math.ceil(h * ratio) / h,
      ]);
      const copies = instanced
        ? f.buffers[2].data
        : draws.flatMap((d) => d.offset);
      let covered = 0;
      for (let i = 0; i < copies.length; i += 2) {
        const x = copies[i] - phase * graph.width,
          y = copies[i + 1] - phase * graph.height;
        covered +=
          Math.max(0, Math.min(w, x + graph.width) - Math.max(0, x)) *
          Math.max(0, Math.min(h, y + graph.height) - Math.max(0, y));
      }
      // GPU coordinates are float32; tolerate only their subpixel roundoff.
      assert.ok(Math.abs(covered - w * h) < 0.5);
    }
    f.setLost(true);
    f.calls.length = 0;
    renderer.paint(flow, 10);
    assert.equal(f.calls.length, 0);
    f.setLost(false);
    renderer.restore();
    renderer.paint(flow, 11);
    assert.equal(f.programs.length, 2);
    assert.equal(f.buffers.length, 6);
    assert.ok(
      f.calls.some((c) => typeof c === "object" && c.program === f.programs[1]),
    );
    renderer.dispose();
  }
});
