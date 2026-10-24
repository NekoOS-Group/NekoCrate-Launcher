import { computed, reactive } from "vue";
import type { Instance, InstanceVersion } from "./types.ts";

export interface FolderCandidate {
  path: string;
  name: string;
  versions: InstanceVersion[];
  existing_id: string | null;
  blocked: string | null;
}
interface FolderIssue {
  path: string;
  message: string;
}
export interface FolderScan {
  candidates: FolderCandidate[];
  issues: FolderIssue[];
  skipped: number;
}
export interface FolderImportResult {
  added: Instance[];
  existing: Instance[];
  failures: FolderIssue[];
}
interface Options {
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
  sync: () => Promise<void>;
  select: (id: string) => void;
  busy: () => boolean;
  error: (message: string) => void;
}
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function useFolderDrop(options: Options) {
  const state = reactive({
    dragging: false,
    phase: null as null | "scanning" | "preview" | "importing",
    scan: { candidates: [], issues: [], skipped: 0 } as FolderScan,
    selected: new Set<string>(),
    notice: null as null | { name: string; path: string }[],
  });
  let generation = 0;
  let scanId: string | null = null;
  let disposed = false;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(items: { name: string; path: string }[]) {
    if (disposed) return;
    clearTimeout(noticeTimer);
    state.notice = items;
    noticeTimer = setTimeout(() => {
      state.notice = null;
    }, 2000);
  }
  function showLinked(items: { name: string; path: string }[]) {
    if (items.length) showNotice(items);
  }
  const available = computed(() =>
    state.scan.candidates.filter((c) => !c.existing_id && !c.blocked),
  );
  const selected = computed(() =>
    available.value.filter((c) => state.selected.has(c.path)),
  );
  const blocked = computed(() => options.busy() || state.phase === "importing");
  const previewVisible = computed(
    () =>
      state.scan.candidates.length > 1 &&
      (state.phase === "preview" || state.phase === "importing"),
  );
  const cornerBusy = computed(
    () =>
      state.phase === "scanning" ||
      (state.phase === "importing" && state.scan.candidates.length === 1),
  );

  function stopScan() {
    generation++;
    if (scanId) {
      void options
        .rpc("cancel_folder_scan", { scan_id: scanId })
        .catch(() => {});
      scanId = null;
    }
  }
  function close() {
    if (state.phase === "importing") return;
    stopScan();
    state.phase = null;
  }
  function toggle(path: string) {
    if (state.selected.has(path)) state.selected.delete(path);
    else if (available.value.some((c) => c.path === path))
      state.selected.add(path);
  }
  function toggleAll() {
    state.selected =
      selected.value.length === available.value.length
        ? new Set()
        : new Set(available.value.map((c) => c.path));
  }
  async function importSelected() {
    if (disposed || blocked.value || !selected.value.length) return;
    state.phase = "importing";
    const paths = selected.value.map((c) => c.path);
    try {
      const result = await options.rpc<FolderImportResult>("import_folders", {
        paths,
      });
      if (disposed) return;
      // Record committed IDs before refreshing so a refresh failure cannot
      // offer to import them twice.
      const linked = [...result.added, ...result.existing];
      for (const instance of linked) {
        const candidate = state.scan.candidates.find(
          (c) => c.path === instance.game_directory,
        );
        if (candidate) candidate.existing_id = instance.id;
      }
      state.selected = new Set(result.failures.map((f) => f.path));
      showLinked(
        linked.map((i) => ({ name: i.name, path: i.game_directory || "" })),
      );
      state.phase =
        result.failures.length && state.scan.candidates.length > 1
          ? "preview"
          : null;
      if (result.failures.length)
        options.error(
          `${result.failures[0]!.path}\n${result.failures[0]!.message}`,
        );
      await options.sync();
      if (!disposed && linked[0]) options.select(linked[0].id);
    } catch (error) {
      if (!disposed) {
        if (state.phase === "importing")
          state.phase = state.scan.candidates.length > 1 ? "preview" : null;
        options.error(message(error));
      }
    }
  }
  async function scan(paths: string[]) {
    if (disposed || !paths.length) return;
    if (state.phase === "importing") {
      options.error("正在关联实例，请稍候");
      return;
    }
    stopScan();
    const revision = generation;
    const id = crypto.randomUUID();
    scanId = id;
    clearTimeout(noticeTimer);
    state.notice = null;
    state.scan = { candidates: [], issues: [], skipped: 0 };
    state.selected = new Set();
    state.phase = "scanning";
    try {
      const result = await options.rpc<FolderScan>("scan_folders", {
        scan_id: id,
        paths,
      });
      if (disposed || revision !== generation) return;
      scanId = null;
      state.scan = result;
      if (!result.candidates.length) {
        state.phase = null;
        showNotice([]);
        return;
      }
      state.selected = new Set(available.value.map((c) => c.path));
      const only = result.candidates[0];
      if (result.candidates.length === 1 && only) {
        if (only.blocked) {
          state.phase = null;
          options.error(only.blocked);
        } else if (only.existing_id) {
          await options.sync();
          if (disposed || revision !== generation) return;
          options.select(only.existing_id);
          showLinked([{ name: only.name, path: only.path }]);
          state.phase = null;
        } else if (!blocked.value) await importSelected();
        else {
          state.phase = null;
          options.error("请等待当前任务完成后关联");
        }
      } else state.phase = "preview";
    } catch (error) {
      if (!disposed && revision === generation) {
        scanId = null;
        state.phase = null;
        options.error(message(error));
      }
    }
  }
  function handle(event: { type: string; paths?: string[] }) {
    if (disposed) return;
    state.dragging = event.type === "enter" || event.type === "over";
    if (event.type === "drop" && event.paths) void scan(event.paths);
  }
  function dispose() {
    disposed = true;
    clearTimeout(noticeTimer);
    stopScan();
  }
  return {
    state,
    available,
    selected,
    blocked,
    previewVisible,
    cornerBusy,
    handle,
    scan,
    close,
    toggle,
    toggleAll,
    importSelected,
    showLinked,
    dispose,
  };
}
export type FolderDropController = ReturnType<typeof useFolderDrop>;
