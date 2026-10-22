import type { FlowGraph } from "./flowGraph.ts";
export { createHexGraph } from "./flowGraph.ts";
// Compatibility names for the existing simulation API; it now accepts every tiling.
export type HexGraph = FlowGraph;

export const HEX_STEP_SECONDS = 0.3;
export const HEX_HOLD_PROBABILITY = 0.1;
export const HEX_EDGE_CAPACITY = 4;
export const HEX_ACTIVE_INTENSITY = 0.82;

// The full state includes particle positions and their staggered clocks. A
// walker proposes an adjacent move; a full destination rejects it.
type Walker = { edge: number; elapsed: number };
export type HexFlow = {
  graph: HexGraph;
  random: () => number;
  time: number;
  occupancy: Uint8Array;
  blocked: Uint8Array;
  intensity: Float32Array;
  widths: Float32Array;
  walkers: Walker[];
  dye: Float32Array;
  dyeScratch: Float32Array;
  sources: Int8Array;
  regions: number[][];
};

export function restingIntensity() {
  return 0.13;
}
export function restingWidth() {
  return 0.65;
}

/** Hold 10%; distribute the rest equally among incident edges (22.5% on hexes). */
export function sampleHexTransition(
  graph: HexGraph,
  edge: number,
  random: () => number,
): number {
  const draw = random();
  if (draw < HEX_HOLD_PROBABILITY) return edge;
  const neighbors = graph.edges[edge]!.neighbors;
  const slot = Math.floor(
    ((draw - HEX_HOLD_PROBABILITY) / (1 - HEX_HOLD_PROBABILITY)) *
      neighbors.length,
  );
  return neighbors[Math.min(slot, neighbors.length - 1)]!;
}

/** Find bounded connected faces, including multi-cell enclosures across a seam.
 * Lift the periodic graph into the plane: a nonzero winding is an open exterior,
 * not a closed region. Tile boundaries themselves never act as walls.
 */
export function findEnclosedHexRegions(
  graph: HexGraph,
  blocked: Uint8Array,
): number[][] {
  const visited = new Uint8Array(graph.cells.length);
  const xs = new Int32Array(graph.cells.length);
  const ys = new Int32Array(graph.cells.length);
  const regions: number[][] = [];
  for (let root = 0; root < graph.cells.length; root++) {
    if (visited[root]) continue;
    const component = [root];
    visited[root] = 1;
    let winding = false;
    for (let head = 0; head < component.length; head++) {
      const index = component[head]!;
      for (const neighbor of graph.cells[index]!.neighbors) {
        if (blocked[neighbor.edge]) continue;
        const x = xs[index]! + neighbor.dx;
        const y = ys[index]! + neighbor.dy;
        if (!visited[neighbor.cell]) {
          visited[neighbor.cell] = 1;
          xs[neighbor.cell] = x;
          ys[neighbor.cell] = y;
          component.push(neighbor.cell);
        } else if (xs[neighbor.cell] !== x || ys[neighbor.cell] !== y) {
          winding = true;
        }
      }
    }
    if (!winding) regions.push(component);
  }
  return regions;
}

/** Symmetric, conservative dye exchange across unoccupied edges only.
 * Three independent pigments mix by concentration. Small substeps keep the
 * explicit diffusion nonnegative even when advance receives a long frame.
 */
export function diffuseHexDye(
  graph: HexGraph,
  dye: Float32Array,
  blocked: Uint8Array,
  seconds: number,
  scratch = new Float32Array(dye.length),
) {
  if (!Number.isFinite(seconds) || seconds <= 0) return;
  const steps = Math.ceil(seconds / (1 / 60));
  const rate = (0.9 * seconds) / steps;
  for (let step = 0; step < steps; step++) {
    scratch.fill(0);
    graph.edges.forEach((edge, index) => {
      if (blocked[index]) return;
      const a = edge.cells[0]! * 3,
        b = edge.cells[1]! * 3;
      for (let pigment = 0; pigment < 3; pigment++) {
        const flux = (dye[b + pigment]! - dye[a + pigment]!) * rate;
        scratch[a + pigment] += flux;
        scratch[b + pigment] -= flux;
      }
    });
    for (let i = 0; i < dye.length; i++) dye[i] += scratch[i]!;
  }
}

function updateRegions(flow: HexFlow) {
  flow.regions = findEnclosedHexRegions(flow.graph, flow.blocked);
  const previous = flow.sources;
  const sources = new Int8Array(previous.length).fill(-1);
  for (const region of flow.regions) {
    const retained = region
      .map((i) => previous[i]!)
      .filter((source) => source >= 0);
    let source: number;
    if (retained.length) source = retained[0]!;
    else if (region.some((i) => previous[i] === -2)) source = -2;
    else source = flow.random() < 0.75 ? Math.floor(flow.random() * 3) : -2;
    for (const cell of region) {
      // Merging two colored enclosures preserves both pigments so they mix.
      sources[cell] = previous[cell]! >= 0 ? previous[cell]! : source;
    }
  }
  flow.sources = sources;
}

function updateEdges(flow: HexFlow, dt: number) {
  let boundaryChanged = false;
  const colorEase = 1 - Math.exp(-dt / 0.45);
  const widthEase = 1 - Math.exp(-dt / 0.55);
  flow.occupancy.forEach((count, index) => {
    const active = count > 0 ? 1 : 0;
    if (flow.blocked[index] !== active) boundaryChanged = true;
    flow.blocked[index] = active;
    // Only the binary state chooses a color target; capacity chooses width.
    // The renderer eases toward both independently of the 300 ms Markov clock.
    const colorTarget = active ? HEX_ACTIVE_INTENSITY : restingIntensity();
    const widthTarget = restingWidth() + count * 0.7;
    flow.intensity[index] += (colorTarget - flow.intensity[index]!) * colorEase;
    flow.widths[index] += (widthTarget - flow.widths[index]!) * widthEase;
  });
  if (boundaryChanged) updateRegions(flow);
}

function advanceDye(flow: HexFlow, dt: number) {
  const charge = 1 - Math.exp(-dt / 0.55);
  for (let cell = 0; cell < flow.sources.length; cell++) {
    const source = flow.sources[cell]!;
    if (source < 0) continue;
    const offset = cell * 3;
    const total =
      flow.dye[offset]! + flow.dye[offset + 1]! + flow.dye[offset + 2]!;
    flow.dye[offset + source] += Math.max(0, 0.9 - total) * charge;
  }
  diffuseHexDye(flow.graph, flow.dye, flow.blocked, dt, flow.dyeScratch);
  const fade = Math.exp(-dt / 10);
  for (let i = 0; i < flow.dye.length; i++) flow.dye[i] *= fade;
}

export function createHexFlow(graph: HexGraph, random = Math.random): HexFlow {
  const size = graph.edges.length;
  const flow: HexFlow = {
    graph,
    random,
    time: 0,
    occupancy: new Uint8Array(size),
    blocked: new Uint8Array(size),
    intensity: new Float32Array(size).fill(restingIntensity()),
    widths: new Float32Array(size).fill(restingWidth()),
    walkers: [],
    dye: new Float32Array(graph.cells.length * 3),
    dyeScratch: new Float32Array(graph.cells.length * 3),
    sources: new Int8Array(graph.cells.length).fill(-1),
    regions: [],
  };
  // 486 packets on the default graph, with four slots per edge. Individual
  // phases avoid a synchronized flash of the entire grid every 300 ms.
  const count = Math.max(2, Math.round(size * 0.75));
  for (let i = 0; i < count; i++) {
    let edge = Math.min(size - 1, Math.floor(random() * size));
    while (flow.occupancy[edge]! >= HEX_EDGE_CAPACITY) edge = (edge + 1) % size;
    flow.occupancy[edge]++;
    flow.walkers.push({ edge, elapsed: random() * HEX_STEP_SECONDS });
  }
  updateEdges(flow, 0);
  return flow;
}

export function advanceHexFlow(flow: HexFlow, seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return;
  const dt = Math.min(seconds, 0.1);
  flow.time += dt;
  for (const walker of flow.walkers) {
    walker.elapsed += dt;
    while (walker.elapsed >= HEX_STEP_SECONDS) {
      walker.elapsed -= HEX_STEP_SECONDS;
      const next = sampleHexTransition(flow.graph, walker.edge, flow.random);
      if (next !== walker.edge && flow.occupancy[next]! < HEX_EDGE_CAPACITY) {
        flow.occupancy[walker.edge]--;
        flow.occupancy[next]++;
        walker.edge = next;
      }
    }
  }
  updateEdges(flow, dt);
  advanceDye(flow, dt);
}
