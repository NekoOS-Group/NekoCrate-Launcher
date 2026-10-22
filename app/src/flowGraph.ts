type Point = readonly [number, number];
export type FlowEdge = {
  path: string;
  vertices: readonly [string, string];
  neighbors: number[];
  cells: number[];
};
export type FlowCell = {
  path: string;
  edges: number[];
  neighbors: { cell: number; edge: number; dx: number; dy: number }[];
};
export type FlowGraph = {
  width: number;
  height: number;
  edges: FlowEdge[];
  cells: FlowCell[];
  washes: string[];
};
type Face = { points: Point[]; center: Point; wash?: boolean };
const wrap = (n: number, size: number) => ((n % size) + size) % size;

/** All topology lives on an integer lattice; rendering alone scales to pixels.
 * Shared edges and their seam copies retain one state in the periodic graph.
 */
function periodicGraph(
  faces: Face[],
  width: number,
  height: number,
  unitX: number,
  unitY: number,
): FlowGraph {
  const vertexKey = ([x, y]: Point) => `${wrap(x, width)}:${wrap(y, height)}`;
  function path(points: Point[], close = false) {
    const pieces: string[] = [];
    for (const dx of [-width, 0, width]) {
      for (const dy of [-height, 0, height]) {
        const shifted = points.map(([x, y]) => [x + dx, y + dy] as const);
        if (
          shifted.every(([x]) => x < 0) ||
          shifted.every(([x]) => x > width) ||
          shifted.every(([, y]) => y < 0) ||
          shifted.every(([, y]) => y > height)
        )
          continue;
        pieces.push(
          "M" +
            shifted
              .map(
                ([x, y]) =>
                  `${(x * unitX).toFixed(5)} ${(y * unitY).toFixed(5)}`,
              )
              .join(" L") +
            (close ? " Z" : ""),
        );
      }
    }
    return pieces.join(" ");
  }
  const edges: FlowEdge[] = [];
  const cells: FlowCell[] = [];
  const unique = new Map<string, number>();
  const incident = new Map<string, number[]>();
  for (const { points } of faces) {
    const cellIndex = cells.length;
    const cell: FlowCell = {
      path: path(points, true),
      edges: [],
      neighbors: [],
    };
    cells.push(cell);
    for (let side = 0; side < points.length; side++) {
      const a = points[side]!,
        b = points[(side + 1) % points.length]!;
      const vertices = [vertexKey(a), vertexKey(b)] as const;
      const key = [...vertices].sort().join("|");
      const previous = unique.get(key);
      if (previous !== undefined) {
        edges[previous]!.cells.push(cellIndex);
        cell.edges.push(previous);
        continue;
      }
      const index = edges.length;
      unique.set(key, index);
      edges.push({
        path: path([a, b]),
        vertices,
        neighbors: [],
        cells: [cellIndex],
      });
      cell.edges.push(index);
      for (const vertex of vertices) {
        const list = incident.get(vertex) || [];
        list.push(index);
        incident.set(vertex, list);
      }
    }
  }
  edges.forEach((edge, index) => {
    if (edge.cells.length !== 2)
      throw new Error("A periodic edge must join two faces");
    edge.neighbors = [
      ...new Set(edge.vertices.flatMap((v) => incident.get(v)!)),
    ].filter((neighbor) => neighbor !== index);
    const [a, b] = edge.cells as [number, number];
    const dx =
      wrap(faces[b]!.center[0] - faces[a]!.center[0] + width / 2, width) -
      width / 2;
    const dy =
      wrap(faces[b]!.center[1] - faces[a]!.center[1] + height / 2, height) -
      height / 2;
    cells[a]!.neighbors.push({ cell: b, edge: index, dx, dy });
    cells[b]!.neighbors.push({ cell: a, edge: index, dx: -dx, dy: -dy });
  });
  return {
    width: width * unitX,
    height: height * unitY,
    edges,
    cells,
    washes: faces
      .filter((face) => face.wash)
      .map((face) => path(face.points, true)),
  };
}

export function createHexGraph(
  columns = 18,
  rows = 12,
  radius = 40,
): FlowGraph {
  if (
    !Number.isInteger(columns) ||
    columns < 4 ||
    columns % 2 ||
    !Number.isInteger(rows) ||
    rows < 4 ||
    !Number.isFinite(radius) ||
    radius <= 0
  ) {
    throw new Error(
      "Hex grid requires even columns >= 4, rows >= 4 and a positive radius",
    );
  }
  const offsets: Point[] = [
    [2, 0],
    [1, 1],
    [-1, 1],
    [-2, 0],
    [-1, -1],
    [1, -1],
  ];
  const faces: Face[] = [];
  for (let c = 0; c < columns; c++)
    for (let r = 0; r < rows; r++) {
      const center = [c * 3, r * 2 + (c % 2)] as const;
      faces.push({
        center,
        points: offsets.map(([x, y]) => [center[0] + x, center[1] + y]),
        wash: (c % 6 === 2 && r % 4 === 0) || (c % 6 === 5 && r % 4 === 2),
      });
    }
  return periodicGraph(
    faces,
    columns * 3,
    rows * 2,
    radius / 2,
    (Math.sqrt(3) * radius) / 2,
  );
}

export type BackdropKind =
  "honeycomb" | "chunks" | "diamonds" | "facets" | "circuit";

export function createBackdropGraph(kind: BackdropKind): FlowGraph {
  if (kind === "honeycomb") return createHexGraph(16, 10, 46);
  const faces: Face[] = [];
  if (kind === "chunks") {
    // Alternating narrow/wide tracks make a chunk-like hierarchy of squares and rectangles.
    const track = [0];
    for (let i = 0; i < 16; i++) track.push(track[i]! + (i % 2 ? 4 : 2));
    for (let c = 0; c < 16; c++)
      for (let r = 0; r < 12; r++) {
        const x = track[c]!,
          y = track[r]!,
          right = track[c + 1]!,
          bottom = track[r + 1]!;
        faces.push({
          center: [(x + right) / 2, (y + bottom) / 2],
          points: [
            [x, y],
            [right, y],
            [right, bottom],
            [x, bottom],
          ],
          wash: c % 4 === 1 && r % 4 === 1,
        });
      }
    return periodicGraph(faces, track[16]!, track[12]!, 22, 22);
  }
  if (kind === "diamonds") {
    for (let r = 0; r < 24; r++)
      for (let c = 0; c < 10; c++) {
        const x = 2 * c + (r % 2);
        faces.push({
          center: [x, r],
          points: [
            [x, r - 1],
            [x + 1, r],
            [x, r + 1],
            [x - 1, r],
          ],
          wash: c % 5 === 2 && r % 6 === 1,
        });
      }
    return periodicGraph(faces, 20, 24, 54, 32);
  }
  if (kind === "facets") {
    for (let c = 0; c < 12; c++)
      for (let r = 0; r < 10; r++) {
        const x = c * 6,
          y = r * 6;
        const a: Point = [x, y],
          b: Point = [x + 6, y],
          d: Point = [x, y + 6],
          e: Point = [x + 6, y + 6];
        const triangles =
          (c + r) % 2
            ? [
                [a, b, d],
                [b, e, d],
              ]
            : [
                [a, b, e],
                [a, e, d],
              ];
        for (const points of triangles)
          faces.push({
            points,
            center: [
              points.reduce((sum, p) => sum + p[0], 0) / 3,
              points.reduce((sum, p) => sum + p[1], 0) / 3,
            ],
            wash: c % 4 === 0 && r % 4 === 0,
          });
      }
    return periodicGraph(faces, 72, 60, 14, 12);
  }
  // Octagonal circuit bays are joined by small diamond junctions (4.8.8 tiling).
  const octagon: Point[] = [
    [1, 0],
    [5, 0],
    [6, 1],
    [6, 5],
    [5, 6],
    [1, 6],
    [0, 5],
    [0, 1],
  ];
  for (let c = 0; c < 12; c++)
    for (let r = 0; r < 9; r++) {
      const x = c * 6,
        y = r * 6;
      faces.push({
        center: [x + 3, y + 3],
        points: octagon.map(([dx, dy]) => [x + dx, y + dy]),
      });
      faces.push({
        center: [x, y],
        points: [
          [x, y - 1],
          [x + 1, y],
          [x, y + 1],
          [x - 1, y],
        ],
        wash: true,
      });
    }
  return periodicGraph(faces, 72, 54, 15, 15);
}
