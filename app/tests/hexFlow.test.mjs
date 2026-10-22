import test from "node:test";
import assert from "node:assert/strict";
import {
  createHexGraph,
  createHexFlow,
  advanceHexFlow,
  sampleHexTransition,
  restingIntensity,
  HEX_STEP_SECONDS,
  HEX_EDGE_CAPACITY,
  HEX_ACTIVE_INTENSITY,
  restingWidth,
  findEnclosedHexRegions,
  diffuseHexDye,
} from "../src/hexFlow.ts";

function seededRandom(seed = 42) {
  return () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
}

test("六边形共享边唯一，每条边有四条相邻边，周期边界仍连通", () => {
  const graph = createHexGraph();
  assert.equal(graph.edges.length, 18 * 12 * 3);
  const keys = new Set(
    graph.edges.map((edge) => [...edge.vertices].sort().join("|")),
  );
  assert.equal(keys.size, graph.edges.length);
  let wrapped = 0;
  graph.edges.forEach((edge, index) => {
    assert.equal(edge.neighbors.length, 4);
    assert.equal(edge.cells.length, 2);
    for (const cell of edge.cells)
      assert.ok(graph.cells[cell].edges.includes(index));
    for (const neighbor of edge.neighbors) {
      const adjacent = graph.edges[neighbor];
      assert.ok(adjacent.neighbors.includes(index));
      assert.ok(
        edge.vertices.some((vertex) => adjacent.vertices.includes(vertex)),
      );
    }
    if (edge.path.includes(" M")) wrapped++;
    for (const segment of edge.path.split("M").slice(1)) {
      const [x1, y1, x2, y2] = segment.trim().split(/[ L]+/).map(Number);
      assert.ok(Math.abs(Math.hypot(x2 - x1, y2 - y1) - 40) < 0.001);
    }
  });
  assert.ok(wrapped > 0);
  assert.equal(graph.cells.length, 216);
  for (const cell of graph.cells) {
    assert.equal(new Set(cell.edges).size, 6);
    assert.equal(cell.neighbors.length, 6);
  }
  const visited = new Set([0]);
  for (const index of visited)
    for (const neighbor of graph.edges[index].neighbors) visited.add(neighbor);
  assert.equal(visited.size, graph.edges.length);
});

test("转移核只取当前边：10% 停留，四条邻边各 22.5%，允许折返", () => {
  const graph = createHexGraph(6, 4);
  const edge = 17;
  const counts = new Map([
    [edge, 0],
    ...graph.edges[edge].neighbors.map((i) => [i, 0]),
  ]);
  // Uniform samples of the complete probability interval, not a flaky Monte Carlo check.
  for (let i = 0; i < 10000; i++) {
    const next = sampleHexTransition(graph, edge, () => (i + 0.5) / 10000);
    assert.ok(
      counts.has(next),
      "probability outside adjacent edges must be zero",
    );
    counts.set(next, counts.get(next) + 1);
  }
  assert.equal(counts.get(edge), 1000);
  for (const neighbor of graph.edges[edge].neighbors) {
    assert.equal(counts.get(neighbor), 2250);
    const reverseSlot = graph.edges[neighbor].neighbors.indexOf(edge);
    const reverseDraw = 0.1 + (reverseSlot + 0.5) * 0.225;
    assert.equal(
      sampleHexTransition(graph, neighbor, () => reverseDraw),
      edge,
    );
  }
});

function putWalkers(flow, edges) {
  flow.occupancy.fill(0);
  flow.walkers = edges.map((edge) => {
    flow.occupancy[edge]++;
    return { edge, elapsed: 0 };
  });
}

test("每 0.3 秒向邻边转移，满容量的边拒绝进入且不丢失信号", () => {
  assert.equal(HEX_STEP_SECONDS, 0.3);
  assert.equal(HEX_EDGE_CAPACITY, 4);
  const graph = createHexGraph(6, 4);
  const flow = createHexFlow(graph, seededRandom());
  const source = 17,
    destination = graph.edges[source].neighbors[0];
  putWalkers(flow, [source]);
  flow.random = () => 0.2125;
  advanceHexFlow(flow, 0.1);
  assert.equal(flow.walkers[0].edge, source);
  advanceHexFlow(flow, 0.1);
  assert.equal(flow.walkers[0].edge, source);
  advanceHexFlow(flow, 0.1);
  assert.equal(flow.walkers[0].edge, destination);

  putWalkers(flow, [source, ...Array(HEX_EDGE_CAPACITY).fill(destination)]);
  let draw = 0;
  flow.random = () => (draw++ === 0 ? 0.2125 : 0.05);
  advanceHexFlow(flow, 0.1);
  advanceHexFlow(flow, 0.1);
  advanceHexFlow(flow, 0.1);
  assert.equal(flow.walkers[0].edge, source);
  assert.equal(flow.occupancy[destination], HEX_EDGE_CAPACITY);
  assert.equal(
    flow.occupancy.reduce((a, b) => a + b, 0),
    HEX_EDGE_CAPACITY + 1,
  );
});

test("同样的激活历史产生相同线色，占用量只改变线宽，启停均有缓动", () => {
  const flow = createHexFlow(createHexGraph(6, 4), seededRandom());
  putWalkers(flow, [0, 10, 10, 10]);
  flow.random = () => 0.05; // All walkers hold their current edge.
  for (let frame = 0; frame < 150; frame++) {
    const previous = flow.intensity[0];
    advanceHexFlow(flow, 1 / 30);
    assert.ok(Math.abs(flow.intensity[0] - flow.intensity[10]) < 1e-6);
    assert.ok(flow.widths[10] > flow.widths[0]);
    assert.ok(flow.intensity[0] >= previous);
    assert.ok(flow.intensity[0] - previous < 0.055);
  }
  assert.ok(Math.abs(flow.intensity[0] - HEX_ACTIVE_INTENSITY) < 0.001);
  assert.ok(Math.abs(flow.widths[10] - (restingWidth() + 3 * 0.7)) < 0.002);
  putWalkers(flow, []);
  const before = flow.intensity[0];
  advanceHexFlow(flow, 1 / 30);
  assert.ok(
    flow.intensity[0] < before && flow.intensity[0] > restingIntensity(),
  );
  assert.ok(flow.widths[10] > restingWidth());
});

function enclose(graph, cells) {
  const members = new Set(cells);
  const blocked = new Uint8Array(graph.edges.length);
  graph.edges.forEach((edge, index) => {
    if (members.has(edge.cells[0]) !== members.has(edge.cells[1]))
      blocked[index] = 1;
  });
  return blocked;
}

test("识别单格、多格和跨平铺接缝的封闭区域，开口后连通外部", () => {
  const graph = createHexGraph(6, 4);
  assert.deepEqual(
    findEnclosedHexRegions(graph, new Uint8Array(graph.edges.length)),
    [],
  );
  assert.deepEqual(findEnclosedHexRegions(graph, enclose(graph, [7])), [[7]]);
  // Cell 0 connects to the last column through the periodic boundary.
  const acrossSeam = graph.cells[0].neighbors.find((n) => n.cell >= 20).cell;
  const blocked = enclose(graph, [0, acrossSeam]);
  const regions = findEnclosedHexRegions(graph, blocked);
  assert.equal(regions.length, 1);
  assert.deepEqual(
    regions[0].sort((a, b) => a - b),
    [0, acrossSeam],
  );
  const opening = graph.cells[0].edges.find((edge) => blocked[edge]);
  blocked[opening] = 0;
  assert.deepEqual(findEnclosedHexRegions(graph, blocked), []);
  // A strip wrapping all the way around the tile is not a bounded enclosure.
  assert.deepEqual(
    findEnclosedHexRegions(graph, enclose(graph, [0, 1, 2, 3])),
    [],
  );
});

test("染料不能穿过激活边，缺口打开后泄漏，交换过程守恒且不产生负浓度", () => {
  const graph = createHexGraph(6, 4);
  const cell = 7;
  const blocked = enclose(graph, [cell]);
  const dye = new Float32Array(graph.cells.length * 3);
  dye[cell * 3] = 1;
  const before = Float32Array.from(dye);
  diffuseHexDye(graph, dye, blocked, 1);
  assert.deepEqual(dye, before);
  const opening = graph.cells[cell].neighbors[0];
  blocked[opening.edge] = 0;
  diffuseHexDye(graph, dye, blocked, 0.3);
  assert.ok(dye[cell * 3] < 1);
  assert.ok(dye[opening.cell * 3] > 0.1);
  assert.ok(Math.abs(dye.reduce((a, b) => a + b, 0) - 1) < 1e-5);
  assert.ok([...dye].every((v) => Number.isFinite(v) && v >= 0));
});

test("两个区域通过开放边混合颜色，而不影响被封闭的区域", () => {
  const graph = createHexGraph(6, 4);
  const a = 7,
    neighbor = graph.cells[a].neighbors[0],
    b = neighbor.cell;
  const blocked = new Uint8Array(graph.edges.length).fill(1);
  blocked[neighbor.edge] = 0;
  const dye = new Float32Array(graph.cells.length * 3);
  dye[a * 3] = 1;
  dye[b * 3 + 2] = 1;
  diffuseHexDye(graph, dye, blocked, 1);
  for (const cell of [a, b]) {
    assert.ok(dye[cell * 3] > 0.4 && dye[cell * 3] < 0.6);
    assert.ok(dye[cell * 3 + 2] > 0.4 && dye[cell * 3 + 2] < 0.6);
  }
  graph.cells.forEach((_, cell) => {
    if (cell !== a && cell !== b)
      assert.deepEqual([...dye.slice(cell * 3, cell * 3 + 3)], [0, 0, 0]);
  });
  assert.ok(Math.abs(dye.reduce((sum, v) => sum + v, 0) - 2) < 1e-5);
});

test("默认网格更密集，持续运行不超容量、不丢信号，边和区域变化均连续有界", () => {
  const flow = createHexFlow(createHexGraph(), seededRandom());
  const count = flow.walkers.length;
  assert.ok(count >= 324);
  assert.ok(flow.blocked.reduce((a, b) => a + b, 0) > 250);
  let colored = false,
    mixed = false;
  for (let frame = 0; frame < 600; frame++) {
    const previous = Float32Array.from(flow.intensity);
    const widths = Float32Array.from(flow.widths);
    const dye = Float32Array.from(flow.dye);
    const oldEdges = flow.walkers.map((walker) => walker.edge);
    advanceHexFlow(flow, 1 / 30);
    flow.walkers.forEach((walker, i) => {
      assert.ok(
        walker.edge === oldEdges[i] ||
          flow.graph.edges[oldEdges[i]].neighbors.includes(walker.edge),
      );
    });
    assert.equal(
      flow.occupancy.reduce((a, b) => a + b, 0),
      count,
    );
    const actual = new Uint8Array(flow.occupancy.length);
    for (const walker of flow.walkers) actual[walker.edge]++;
    assert.deepEqual(flow.occupancy, actual);
    flow.occupancy.forEach((occupied, index) => {
      assert.ok(occupied <= HEX_EDGE_CAPACITY);
      assert.equal(flow.blocked[index], occupied > 0 ? 1 : 0);
      assert.ok(Math.abs(flow.intensity[index] - previous[index]) < 0.055);
      assert.ok(Math.abs(flow.widths[index] - widths[index]) < 0.19);
      assert.ok(
        flow.intensity[index] >= 0.129 &&
          flow.intensity[index] <= HEX_ACTIVE_INTENSITY + 0.001,
      );
    });
    flow.dye.forEach((value, index) => {
      assert.ok(Number.isFinite(value) && value >= 0 && value < 0.901);
      assert.ok(Math.abs(value - dye[index]) < 0.12);
    });
    flow.graph.cells.forEach((_, cell) => {
      const pigments = flow.dye.slice(cell * 3, cell * 3 + 3);
      const total = pigments.reduce((a, b) => a + b, 0);
      assert.ok(total < 0.901);
      colored ||= total > 0.2;
      mixed ||= [...pigments].filter((v) => v > 0.03).length >= 2;
    });
  }
  assert.ok(colored && mixed);
});

test("无效时间不更新，长停顿不会跳过大量传播步骤", () => {
  const flow = createHexFlow(createHexGraph(6, 4), seededRandom());
  const before = Float32Array.from(flow.intensity);
  for (const dt of [0, -1, Infinity, NaN]) advanceHexFlow(flow, dt);
  assert.equal(flow.time, 0);
  assert.deepEqual(flow.intensity, before);
  advanceHexFlow(flow, 3600);
  assert.equal(flow.time, 0.1);
  assert.ok([...flow.intensity].every(Number.isFinite));
});
