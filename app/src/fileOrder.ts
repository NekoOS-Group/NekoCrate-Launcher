import type { DownloadFile } from "./types.ts";

interface RankIndex {
  positions: number[];
  present: Uint8Array;
  counts: Int32Array;
  count: number;
}

export interface FileOrder {
  files: DownloadFile[];
  positions: Map<string, number>;
  groups: Uint8Array;
  slots: Int32Array;
  pending: RankIndex;
  active: RankIndex;
  completed: RankIndex;
  revision: number;
}

function createRankIndex(): RankIndex {
  return {
    positions: [],
    present: new Uint8Array(16),
    counts: new Int32Array(17),
    count: 0,
  };
}
export function createFileOrder(): FileOrder {
  return {
    files: [],
    positions: new Map(),
    groups: new Uint8Array(16),
    slots: new Int32Array(16),
    pending: createRankIndex(),
    active: createRankIndex(),
    completed: createRankIndex(),
    revision: 0,
  };
}

function prefix(index: RankIndex, length: number) {
  let sum = 0;
  for (let i = length; i > 0; i -= i & -i) sum += index.counts[i]!;
  return sum;
}
function append(index: RankIndex, position: number, present: number) {
  const slot = index.positions.length;
  if (slot === index.present.length) {
    const flags = new Uint8Array(index.present.length * 2);
    flags.set(index.present);
    index.present = flags;
    const counts = new Int32Array(flags.length + 1);
    counts.set(index.counts);
    index.counts = counts;
  }
  index.positions.push(position);
  index.present[slot] = present;
  const n = slot + 1;
  index.counts[n] =
    prefix(index, n - 1) - prefix(index, n - (n & -n)) + present;
  index.count += present;
  return slot;
}
function setPresent(index: RankIndex, slot: number, present: number) {
  const delta = present - index.present[slot]!;
  index.present[slot] = present;
  for (let i = slot + 1; i <= index.positions.length; i += i & -i)
    index.counts[i] += delta;
  index.count += delta;
}
function group(file: DownloadFile) {
  if (file.status === "done") return 2;
  return ["downloading", "verifying", "reusing", "installing"].includes(
    file.status,
  )
    ? 1
    : 0;
}
function partition(order: FileOrder, group: number) {
  return group === 1
    ? order.active
    : group === 2
      ? order.completed
      : order.pending;
}

/** Active transfers stay in start order through verification/installation.
 * Completed files use reverse completion order; other files keep plan order.
 * Fenwick indexes update/select in O(log n), without sorting the whole list.
 * Duplicate completion events and byte-only updates never change a rank.
 */
export function indexFile(order: FileOrder, file: DownloadFile) {
  let position = order.positions.get(file.id);
  const next = group(file);
  if (position !== undefined) {
    const previous = order.groups[position]!;
    if (previous === next) return;
    setPresent(partition(order, previous), order.slots[position]!, 0);
  } else {
    position = order.files.length;
    if (position === order.groups.length) {
      const groups = new Uint8Array(order.groups.length * 2);
      groups.set(order.groups);
      order.groups = groups;
      const slots = new Int32Array(groups.length);
      slots.set(order.slots);
      order.slots = slots;
    }
    order.positions.set(file.id, position);
    order.files.push(file);
    // Every file keeps its original pending slot, including after cancellation.
    append(order.pending, position, 0);
  }
  order.groups[position] = next;
  if (next === 0) {
    setPresent(order.pending, position, 1);
    order.slots[position] = position;
  } else order.slots[position] = append(partition(order, next), position, 1);
  order.revision++;
}

/** Select a rank without scanning offscreen files or retired retry slots. */
function atRank(index: RankIndex, rank: number) {
  let slot = 0;
  const length = index.positions.length;
  for (
    let stride = 2 ** Math.floor(Math.log2(Math.max(1, length)));
    stride >= 1;
    stride /= 2
  ) {
    const next = slot + stride;
    if (next > length) continue;
    const count = index.counts[next]!;
    if (count <= rank) {
      rank -= count;
      slot = next;
    }
  }
  return index.positions[slot]!;
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
  for (let i = start; i < Math.min(start + 10, order.files.length); i++) {
    const rank = i - order.active.count;
    const position =
      rank < 0
        ? atRank(order.active, i)
        : rank < order.completed.count
          ? atRank(order.completed, order.completed.count - rank - 1)
          : atRank(order.pending, rank - order.completed.count);
    items.push(order.files[position]!);
  }
  return { start, items, height };
}

export function finishFileOrder(order: FileOrder) {
  // Keep earlier completion ranks; only reconcile statuses changed at shutdown.
  for (const file of order.files) indexFile(order, file);
}
