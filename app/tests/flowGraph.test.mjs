import test from "node:test";
import assert from "node:assert/strict";
import { createBackdropGraph } from "../src/flowGraph.ts";
import { backdropThemes } from "../src/backdropThemes.ts";
import {
  createHexFlow,
  advanceHexFlow,
  sampleHexTransition,
  findEnclosedHexRegions,
  diffuseHexDye,
  HEX_EDGE_CAPACITY,
} from "../src/hexFlow.ts";

function seededRandom(seed = 125) {
  return () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
}

for (const { kind, name } of backdropThemes) {
  test(`${name}背景：无重复边、无孤立面、跨接缝双向邻接，转移概率归一`, () => {
    const graph = createBackdropGraph(kind);
    const vertices = new Set(graph.edges.flatMap((e) => e.vertices));
    const keys = new Set(
      graph.edges.map((e) => [...e.vertices].sort().join("|")),
    );
    assert.equal(keys.size, graph.edges.length);
    // Euler characteristic of a valid torus tiling.
    assert.equal(vertices.size - graph.edges.length + graph.cells.length, 0);
    const visited = new Set([0]);
    for (const i of visited)
      for (const n of graph.cells[i].neighbors) visited.add(n.cell);
    assert.equal(visited.size, graph.cells.length);
    for (const [i, edge] of graph.edges.entries()) {
      assert.equal(edge.cells.length, 2);
      assert.ok(edge.neighbors.length >= 4);
      for (const n of edge.neighbors) {
        assert.ok(graph.edges[n].neighbors.includes(i));
        assert.ok(
          edge.vertices.some((v) => graph.edges[n].vertices.includes(v)),
        );
      }
      for (const cell of edge.cells)
        assert.ok(graph.cells[cell].edges.includes(i));
      assert.equal(
        sampleHexTransition(graph, i, () => 0.05),
        i,
      );
      edge.neighbors.forEach((n, slot) => {
        const p = 0.1 + ((slot + 0.5) * 0.9) / edge.neighbors.length;
        assert.equal(
          sampleHexTransition(graph, i, () => p),
          n,
        );
      });
    }
    for (const [i, cell] of graph.cells.entries()) {
      assert.equal(cell.neighbors.length, cell.edges.length);
      for (const n of cell.neighbors) {
        const reverse = graph.cells[n.cell].neighbors.find((v) => v.cell === i);
        assert.ok(reverse);
        assert.equal(n.dx + reverse.dx, 0);
        assert.equal(n.dy + reverse.dy, 0);
        assert.ok(Number.isInteger(n.dx) && Number.isInteger(n.dy));
      }
    }
  });

  test(`${name}背景：跨缝围区、开口泄漏与长时间运行仍守恒有界`, () => {
    const graph = createBackdropGraph(kind);
    const blocked = new Uint8Array(graph.edges.length);
    assert.deepEqual(findEnclosedHexRegions(graph, blocked), []);
    // Cell 0 touches the tile boundary in every design. Close it and one neighbor together.
    const pair = [0, graph.cells[0].neighbors[0].cell];
    for (const edge of graph.edges) {
      if (pair.includes(edge.cells[0]) !== pair.includes(edge.cells[1])) {
        blocked[graph.edges.indexOf(edge)] = 1;
      }
    }
    const regions = findEnclosedHexRegions(graph, blocked);
    assert.deepEqual(
      regions.map((r) => r.sort((a, b) => a - b)),
      [pair.sort((a, b) => a - b)],
    );
    const dye = new Float32Array(graph.cells.length * 3);
    dye[0] = 1;
    diffuseHexDye(graph, dye, blocked, 1);
    let outside = 0;
    for (let i = 0; i < graph.cells.length; i++)
      if (!pair.includes(i)) outside += dye[i * 3];
    assert.equal(outside, 0);
    blocked.fill(0);
    assert.deepEqual(findEnclosedHexRegions(graph, blocked), []);
    diffuseHexDye(graph, dye, blocked, 1);
    assert.ok(Math.abs(dye.reduce((a, b) => a + b, 0) - 1) < 0.00001);
    assert.ok(dye.every((v) => v >= 0));
    assert.ok(dye[graph.cells[0].neighbors[1].cell * 3] > 0);

    const flow = createHexFlow(graph, seededRandom());
    const count = flow.walkers.length;
    for (let frame = 0; frame < 600; frame++) {
      advanceHexFlow(flow, 1 / 30);
      assert.equal(
        flow.occupancy.reduce((a, b) => a + b, 0),
        count,
      );
      assert.ok(flow.occupancy.every((v) => v <= HEX_EDGE_CAPACITY));
      assert.ok(flow.dye.every((v) => Number.isFinite(v) && v >= 0 && v <= 1));
    }
    assert.ok(flow.dye.some((v) => v > 0.01));
  });
}

test("五种主题有不同的几何结构与可辨识的轮廓", () => {
  assert.equal(new Set(backdropThemes.map((t) => t.kind)).size, 5);
  assert.equal(new Set(backdropThemes.map((t) => t.mark)).size, 5);
  const signatures = backdropThemes.map(({ kind }) => {
    const graph = createBackdropGraph(kind);
    return graph.cells
      .slice(0, 8)
      .map((c) => c.path)
      .join("|");
  });
  assert.equal(new Set(signatures).size, 5);
});
