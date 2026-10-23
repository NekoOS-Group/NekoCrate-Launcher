import type { FlowGraph } from "./flowGraph.ts";

type Point = readonly [number, number];
const corners: Point[] = [
  [-1, -1],
  [1, -1],
  [-1, 1],
  [-1, 1],
  [1, -1],
  [1, 1],
];

/** The graph's absolute M/L/Z paths, including all periodic seam copies. */
export function flowPolygons(path: string): Point[][] {
  return path
    .split("M")
    .filter(Boolean)
    .map((part) => {
      const values = part.match(/-?\d+(?:\.\d+)?/g)!.map(Number);
      const points: Point[] = [];
      for (let i = 0; i < values.length; i += 2)
        points.push([values[i]!, values[i + 1]!]);
      return points;
    });
}

/** Fixed geometry stays on the GPU. Each quad is an independently blended
 * path, in the original wash/cell/edge order; no merging of overlapping edges.
 */
export function createFlowGeometry(graph: FlowGraph) {
  const fills: number[] = [],
    fillIds: number[] = [];
  const lines: number[] = [],
    lineIds: number[] = [];
  function polygon(points: Point[], id: number) {
    if (points.length < 3 || points.length > 8)
      throw new Error("Unsupported flow polygon");
    const xs = points.map((p) => p[0]),
      ys = points.map((p) => p[1]);
    const left = Math.min(...xs) - 1,
      top = Math.min(...ys) - 1,
      right = Math.max(...xs) + 1,
      bottom = Math.max(...ys) + 1;
    let area = 0;
    for (let i = 0; i < points.length; i++) {
      const a = points[i]!,
        b = points[(i + 1) % points.length]!;
      area += a[0] * b[1] - b[0] * a[1];
    }
    const planes = points.map((a, i) => {
      const b = points[(i + 1) % points.length]!,
        dx = b[0] - a[0],
        dy = b[1] - a[1];
      const scale = (area > 0 ? 1 : -1) / Math.hypot(dx, dy);
      const nx = -dy * scale,
        ny = dx * scale;
      return [nx, ny, -nx * a[0] - ny * a[1]];
    });
    for (const corner of corners) {
      const x = corner[0] < 0 ? left : right,
        y = corner[1] < 0 ? top : bottom;
      fills.push(x, y);
      for (let i = 0; i < 8; i++) {
        const p = planes[i];
        fills.push(p ? p[0]! * x + p[1]! * y + p[2]! : 1e5);
      }
      fillIds.push(id);
    }
  }
  for (const wash of graph.washes)
    for (const points of flowPolygons(wash)) polygon(points, -1);
  graph.cells.forEach((cell, i) => {
    for (const points of flowPolygons(cell.path)) polygon(points, i);
  });
  graph.edges.forEach((edge, i) => {
    for (const points of flowPolygons(edge.path)) {
      if (points.length !== 2) throw new Error("Unsupported flow edge");
      for (const corner of corners) {
        lines.push(...points[0]!, ...points[1]!, ...corner);
        lineIds.push(i);
      }
    }
  });
  return {
    fills: new Float32Array(fills),
    fillIds: new Int32Array(fillIds),
    lines: new Float32Array(lines),
    lineIds: new Int32Array(lineIds),
  };
}

/** One ordered mesh allows instancing complete tiles without changing alpha
 * compositing order at seams: washes, cells, edges, then the next tile.
 */
export function createFlowBatchGeometry(graph: FlowGraph) {
  const original = createFlowGeometry(graph);
  const count = original.fillIds.length + original.lineIds.length;
  const vertices = new Float32Array(count * 11);
  const ids = new Int32Array(count);
  const fillCount = original.fillIds.length;
  ids.set(original.fillIds);
  ids.set(original.lineIds, fillCount);
  for (let i = 0; i < fillCount; i++) {
    // Polygon: point.xy, eight signed boundary distances, kind=0.
    vertices.set(original.fills.subarray(i * 10, i * 10 + 10), i * 11);
  }
  for (let i = 0; i < original.lineIds.length; i++) {
    // Line: endpoints.xyxy, corner.xy, padding, kind=1.
    const at = (fillCount + i) * 11;
    vertices.set(original.lines.subarray(i * 6, i * 6 + 6), at);
    vertices[at + 10] = 1;
  }
  return { vertices, ids, fillCount };
}
