import type { LogRow } from "./logPresentation.ts";

const minimumRowHeight = 28;
const overscan = 3;

/** Variable-height window; preserve the reader's anchor when rows wrap or expire. */
export class LogViewport {
  private rows: readonly LogRow[] = [];
  private offsets = [0];
  private heights = new Map<number, number>();
  private indices = new Map<number, number>();
  private width = 0;
  private height = 300;
  top = 0;
  following = true;

  private indexAt(offset: number) {
    let low = 0,
      high = this.rows.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (this.offsets[mid + 1] <= offset) low = mid + 1;
      else high = mid;
    }
    return Math.min(low, Math.max(0, this.rows.length - 1));
  }
  private get maxTop() {
    return Math.max(0, this.offsets.at(-1)! - this.height);
  }
  private rebuild(next: readonly LogRow[] = this.rows) {
    const index = this.indexAt(this.top);
    const anchor = this.rows[index]?.id;
    const within = this.top - this.offsets[index];
    this.rows = next;
    this.indices.clear();
    this.offsets = [0];
    for (let i = 0; i < next.length; i++) {
      const row = next[i];
      this.indices.set(row.id, i);
      this.offsets.push(
        this.offsets.at(-1)! + (this.heights.get(row.id) || minimumRowHeight),
      );
    }
    const at = anchor === undefined ? -1 : (this.indices.get(anchor) ?? -1);
    this.top = this.following
      ? this.maxTop
      : Math.min(
          this.maxTop,
          Math.max(
            0,
            at < 0
              ? this.top
              : this.offsets[at] +
                  Math.min(within, this.offsets[at + 1] - this.offsets[at] - 1),
          ),
        );
  }
  setRows(rows: readonly LogRow[], reset = false) {
    if (reset) this.following = true;
    const previousLength = this.rows.length;
    // Stream appends keep existing offsets and measurements; no history scan.
    if (
      !reset &&
      previousLength > 0 &&
      rows.length >= previousLength &&
      rows[0] === this.rows[0] &&
      rows[previousLength - 1] === this.rows.at(-1)
    ) {
      for (let i = previousLength; i < rows.length; i++) {
        this.indices.set(rows[i].id, i);
        this.offsets.push(this.offsets.at(-1)! + minimumRowHeight);
      }
      this.rows = rows;
      if (this.following) this.top = this.maxTop;
      return;
    }
    const ids = new Set(rows.map((row) => row.id));
    for (const id of this.heights.keys())
      if (!ids.has(id)) this.heights.delete(id);
    this.rebuild(rows);
  }
  resize(width: number, height: number) {
    if (width <= 0 || height <= 0) return false;
    const changed = width !== this.width || height !== this.height;
    if (width !== this.width) this.heights.clear();
    this.width = width;
    this.height = height;
    if (changed) this.rebuild();
    return changed;
  }
  measure(measurements: readonly { id: number; height: number }[]) {
    let changed = false;
    for (const { id, height } of measurements) {
      if (!this.indices.has(id) || !Number.isFinite(height) || height <= 0)
        continue;
      const h = Math.max(minimumRowHeight, height);
      if (Math.abs((this.heights.get(id) || minimumRowHeight) - h) < 0.5)
        continue;
      this.heights.set(id, h);
      changed = true;
    }
    if (changed) this.rebuild();
    return changed;
  }
  scroll(top: number) {
    this.top = Math.max(0, Math.min(top, this.maxTop));
    this.following = this.maxTop - this.top < 24;
  }
  latest() {
    this.following = true;
    this.top = this.maxTop;
  }
  window() {
    const start = Math.max(0, this.indexAt(this.top) - overscan);
    const end = Math.min(
      this.rows.length,
      this.indexAt(this.top + this.height) + 1 + overscan,
    );
    return {
      start,
      items: this.rows.slice(start, end),
      offset: this.offsets[start],
      height: this.offsets.at(-1)!,
      top: this.top,
      following: this.following,
    };
  }
}
