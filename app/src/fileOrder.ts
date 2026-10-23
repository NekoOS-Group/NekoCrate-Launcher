import type { DownloadFile } from "./types.ts";

export interface FileOrder {
  files: DownloadFile[];
  positions: Map<string, number>;
  active: Uint8Array;
  counts: Int32Array;
  activeCount: number;
  revision: number;
}

export function createFileOrder(): FileOrder {
  return {
    files: [],
    positions: new Map(),
    active: new Uint8Array(16),
    counts: new Int32Array(17),
    activeCount: 0,
    revision: 0,
  };
}

function prefix(order: FileOrder, length: number) {
  let sum = 0;
  for (let i = length; i > 0; i -= i & -i) sum += order.counts[i]!;
  return sum;
}

/** Stable insertion order within the active and remaining partitions. The
 * Fenwick tree supports status updates and visible-row selection in O(log n).
 * Typed arrays are deliberately nonreactive; revision invalidates the window
 * only when a file enters/leaves the downloading partition.
 */
export function indexFile(order: FileOrder, file: DownloadFile) {
  const previous = order.positions.get(file.id);
  const active = file.status === "downloading" ? 1 : 0;
  if (previous !== undefined) {
    const delta = active - order.active[previous]!;
    if (!delta) return;
    order.active[previous] = active;
    for (let i = previous + 1; i <= order.files.length; i += i & -i)
      order.counts[i] += delta;
    order.activeCount += delta;
    order.revision++;
    return;
  }
  const position = order.files.length;
  if (position === order.active.length) {
    const flags = new Uint8Array(order.active.length * 2);
    flags.set(order.active);
    order.active = flags;
    const counts = new Int32Array(flags.length + 1);
    counts.set(order.counts);
    order.counts = counts;
  }
  order.positions.set(file.id, position);
  order.files.push(file);
  order.active[position] = active;
  const n = position + 1;
  order.counts[n] = prefix(order, n - 1) - prefix(order, n - (n & -n)) + active;
  order.activeCount += active;
  order.revision++;
}

/** Select a rank in either partition without scanning offscreen file objects. */
function atRank(order: FileOrder, rank: number, active: boolean) {
  let index = 0;
  const files = order.files;
  const counts = order.counts;
  const length = files.length;
  for (
    let stride = 2 ** Math.floor(Math.log2(Math.max(1, length)));
    stride >= 1;
    stride /= 2
  ) {
    const next = index + stride;
    if (next > length) continue;
    const count = active ? counts[next]! : stride - counts[next]!;
    if (count <= rank) {
      rank -= count;
      index = next;
    }
  }
  return files[index]!;
}

export const fileRowHeight = 48;
export function fileWindowBounds(length: number, scrollTop: number) {
  const start = Math.max(
    0,
    Math.min(
      Math.floor(scrollTop / fileRowHeight) - 3,
      Math.max(0, length - 4),
    ),
  );
  return { start, height: Math.min(192, length * fileRowHeight) };
}

export function indexedFileWindow(order: FileOrder, scrollTop: number) {
  // Vue tracks this scalar instead of every file's status.
  void order.revision;
  const { start, height } = fileWindowBounds(order.files.length, scrollTop);
  const items: DownloadFile[] = [];
  for (let i = start; i < Math.min(start + 10, order.files.length); i++)
    items.push(
      i < order.activeCount
        ? atRank(order, i, true)
        : atRank(order, i - order.activeCount, false),
    );
  return { start, items, height };
}

export function finishFileOrder(order: FileOrder) {
  if (!order.activeCount) return;
  order.active.fill(0);
  order.counts.fill(0);
  order.activeCount = 0;
  order.revision++;
}
