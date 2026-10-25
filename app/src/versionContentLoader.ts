import type {
  ContentKind,
  ContentLoadOptions,
  LocalContentFile,
  VersionContent,
} from "./types.ts";

type Rpc = <T>(op: string, data: Record<string, unknown>) => Promise<T>;
type Index = VersionContent & { pending: string[] };
const MAX_CACHE_BYTES = 8 * 1024 * 1024;
const MAX_CACHED_LISTS = 12;

// One loader per launcher UI: cached lists survive closing the dialog, while every
// visit still reconciles with disk. Abort stops scheduling further archive batches.
export function createVersionContentLoader(rpc: Rpc) {
  const cache = new Map<string, { value: VersionContent; bytes: number }>();
  const indexes = new Map<string, Promise<Index>>();
  function readIndex(id: string, version: string) {
    const key = JSON.stringify([id, version]);
    let pending = indexes.get(key);
    if (!pending) {
      pending = rpc<Index>("version_content_index", { id, version }).finally(
        () => indexes.delete(key),
      );
      indexes.set(key, pending);
    }
    return pending;
  }
  let cacheBytes = 0;
  function remember(key: string, value: VersionContent) {
    const old = cache.get(key);
    if (old) {
      cacheBytes -= old.bytes;
      cache.delete(key);
    }
    const bytes =
      2 *
      (value.directory.length +
        value.files.reduce(
          (sum, file) =>
            sum +
            128 +
            file.file_name.length +
            file.name.length +
            (file.version?.length || 0) +
            (file.mod_ids || []).join("").length,
          0,
        ));
    if (bytes > MAX_CACHE_BYTES) return;
    cache.set(key, { value, bytes });
    cacheBytes += bytes;
    while (cache.size > MAX_CACHED_LISTS || cacheBytes > MAX_CACHE_BYTES) {
      const first = cache.keys().next().value!;
      cacheBytes -= cache.get(first)!.bytes;
      cache.delete(first);
    }
  }
  async function load(
    id: string,
    version: string,
    kind: ContentKind,
    options: ContentLoadOptions = {},
  ): Promise<VersionContent> {
    const { signal, onProgress } = options;
    const check = () => {
      if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
    };
    check();
    const key = JSON.stringify([id, version, kind]);
    const cached = cache.get(key);
    if (cached && !options.fresh) onProgress?.(cached.value);
    if (kind !== "mods") {
      const value = await rpc<VersionContent>("version_content", {
        id,
        version,
        kind,
      });
      check();
      remember(key, value);
      return value;
    }
    const index = await readIndex(id, version);
    check();
    // Preserve the visible row order while enriching names; completed names can
    // have a very different alphabetic order from their JAR filenames.
    const indexed = new Map(index.files.map((file) => [file.file_name, file]));
    const anchored = (cached?.value.files || []).flatMap((file) => {
      const fresh = indexed.get(file.file_name);
      indexed.delete(file.file_name);
      return fresh ? [fresh] : [];
    });
    let value: VersionContent = {
      directory: index.directory,
      files: [...anchored, ...indexed.values()],
    };
    onProgress?.(value);
    // The first visible rows arrive sooner; later batches amortize IPC overhead.
    for (let offset = 0; offset < index.pending.length;) {
      check();
      const files = index.pending.slice(
        offset,
        offset + (offset === 0 ? 8 : 24),
      );
      const batch = await rpc<LocalContentFile[]>("version_content_summaries", {
        id,
        version,
        files,
      });
      check();
      const updated = new Map(batch.map((file) => [file.file_name, file]));
      const requested = new Set(files);
      value = {
        directory: index.directory,
        files: value.files.flatMap((file) => {
          const next = updated.get(file.file_name);
          return next ? [next] : requested.has(file.file_name) ? [] : [file];
        }),
      };
      onProgress?.(value);
      offset += files.length;
    }
    const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
    const sorted = {
      ...value,
      files: [...value.files].sort(
        (a, b) =>
          compare(a.name.toLowerCase(), b.name.toLowerCase()) ||
          compare(a.file_name.toLowerCase(), b.file_name.toLowerCase()),
      ),
    };
    // Use the normal alphabetical order next time the page opens, without
    // shuffling the currently visible rows or replaying their entrance animation.
    remember(key, sorted);
    return value;
  }
  return Object.assign(load, {
    async preload(id: string, version: string, signal?: AbortSignal) {
      if (signal?.aborted) return;
      const index = await readIndex(id, version);
      if (signal?.aborted) return;
      const key = JSON.stringify([id, version, "mods"]);
      // Preparing a tab never opens JARs and never replaces an already enriched view.
      if (!cache.has(key))
        remember(key, { directory: index.directory, files: index.files });
    },
  });
}
