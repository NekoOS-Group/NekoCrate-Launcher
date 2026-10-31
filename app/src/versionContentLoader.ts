import { anchorModFiles, modFileName } from "./contentIdentity.ts";
import type {
  ContentKind,
  ContentLoadOptions,
  LocalContentFile,
  VersionContent,
} from "./types.ts";

type Rpc = <T>(op: string, data: Record<string, unknown>) => Promise<T>;
type Index = VersionContent & { pending: string[]; config_ids?: string[] };
const MAX_CACHE_BYTES = 8 * 1024 * 1024;
const MAX_CACHED_LISTS = 12;

function configAvailability(ids: string[] = []) {
  const configIds = new Set(ids);
  return (file: LocalContentFile): LocalContentFile => {
    const ids = file.mod_ids?.length
      ? file.mod_ids
      : [
          modFileName(file.file_name)
            .split("/")
            .pop()!
            .replace(/\.jar$/i, "")
            .split("-")[0]!,
        ];
    return {
      ...file,
      has_config:
        (!file.declaration || file.declaration.status === "local") &&
        ids.some((id) =>
          configIds.has(id.replace(/[A-Z]/g, (ch) => ch.toLowerCase())),
        ),
    };
  };
}

// One loader per launcher UI: cached lists survive closing the dialog, while every
// visit still reconciles with disk. Abort stops scheduling further archive batches.
export function createVersionContentLoader(rpc: Rpc) {
  const cache = new Map<
    string,
    { value: VersionContent; bytes: number; visible: LocalContentFile[] }
  >();
  const indexes = new Map<string, Promise<Index>>();
  function readIndex(id: string, version: string, fresh = false) {
    const key = JSON.stringify([id, version]);
    let pending = fresh ? undefined : indexes.get(key);
    if (!pending) {
      pending = rpc<Index>("version_content_index", { id, version }).finally(
        () => {
          if (indexes.get(key) === pending) indexes.delete(key);
        },
      );
      indexes.set(key, pending);
    }
    return pending;
  }
  let cacheBytes = 0;
  function remember(key: string, value: VersionContent, visible = value.files) {
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
    cache.set(key, { value, bytes, visible });
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
    const index = await readIndex(id, version, options.fresh);
    check();
    // One directory snapshot serves all Mod batches. Config contents remain lazy.
    const withConfig = configAvailability(index.config_ids);
    // Preserve the visible row order while enriching names; completed names can
    // have a very different alphabetic order from their JAR filenames.
    let value: VersionContent = {
      directory: index.directory,
      files: anchorModFiles(
        options.fresh ? cached?.visible || [] : cached?.value.files || [],
        index.files,
        new Set(index.pending),
      ).map(withConfig),
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
      const updated = new Map(
        batch.map((file) => [file.file_name, withConfig(file)]),
      );
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
          compare(
            modFileName(a.file_name).toLowerCase(),
            modFileName(b.file_name).toLowerCase(),
          ),
      ),
    };
    // Use the normal alphabetical order next time the page opens, without
    // shuffling the currently visible rows or replaying their entrance animation.
    remember(key, sorted, value.files);
    return value;
  }
  return Object.assign(load, {
    invalidate(id: string) {
      for (const [key, entry] of cache) {
        if (JSON.parse(key)[0] === id) {
          cacheBytes -= entry.bytes;
          cache.delete(key);
        }
      }
      for (const key of indexes.keys())
        if (JSON.parse(key)[0] === id) indexes.delete(key);
    },
    async preload(id: string, version: string, signal?: AbortSignal) {
      if (signal?.aborted) return;
      const index = await readIndex(id, version);
      if (signal?.aborted) return;
      const key = JSON.stringify([id, version, "mods"]);
      // Preparing a tab never opens JARs and never replaces an already enriched view.
      if (!cache.has(key))
        remember(key, {
          directory: index.directory,
          files: index.files.map(configAvailability(index.config_ids)),
        });
    },
  });
}
