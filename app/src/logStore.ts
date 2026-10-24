import type { LogLine } from "./logPresentation.ts";

export const logRefreshMs = 50;
export type LogStore = ReturnType<typeof createLogStore>;

/** O(1) ingestion without eviction. Only an open viewer subscribes to updates. */
export function createLogStore() {
  let lines: LogLine[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  const listeners = new Set<() => void>();
  const changed = () => {
    if (!listeners.size || timer !== undefined) return;
    timer = setTimeout(() => {
      timer = undefined;
      for (const listener of listeners) listener();
    }, logRefreshMs);
  };
  const snapshot = () => lines.slice();
  return {
    append(line: LogLine) {
      lines.push({ ...line });
      changed();
    },
    snapshot,
    clear(scope = "all") {
      lines =
        scope === "all"
          ? []
          : lines.filter((line) => (line.instanceId || "launcher") !== scope);
      changed();
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (!listeners.size && timer !== undefined) {
          clearTimeout(timer);
          timer = undefined;
        }
      };
    },
    dispose() {
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      listeners.clear();
    },
  };
}
