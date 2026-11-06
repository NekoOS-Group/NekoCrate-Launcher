import type {
  ContentKind,
  ContentLoadOptions,
  ContentMetadata,
  VersionContentCounts,
  VersionMetadata,
} from "./types.ts";
import type { ResourceRpc } from "./projectPresentation.ts";
import type { createVersionContentLoader } from "./versionContentLoader.ts";

/** Resolve the owning window's target at call time; never read the global editor. */
export function createVersionManagerActions(
  instanceId: () => string,
  rpc: ResourceRpc,
  load: ReturnType<typeof createVersionContentLoader>,
  sync: () => Promise<void>,
) {
  return {
    prefetchVersionContent: (version: string, signal: AbortSignal) =>
      load.preload(instanceId(), version, signal),
    readVersionContent: (
      version: string,
      kind: ContentKind,
      options?: ContentLoadOptions,
    ) => load(instanceId(), version, kind, options),
    readVersionContentCounts: (version: string) =>
      rpc<VersionContentCounts>("version_content_counts", {
        id: instanceId(),
        version,
      }),
    readVersionMetadata: (version: string) =>
      rpc<VersionMetadata>("version_metadata", { id: instanceId(), version }),
    readContentMetadata: (
      id: string,
      version: string,
      kind: ContentKind,
      file: string,
    ) =>
      rpc<ContentMetadata>("version_content_metadata", {
        id,
        version,
        kind,
        file,
      }),
    openContentFolder: (
      id: string,
      version: string,
      kind: ContentKind,
      file?: string,
      source?: string,
    ) => rpc<void>("open_content_folder", { id, version, kind, file, source }),
    readContentSource: (
      id: string,
      version: string,
      kind: ContentKind,
      file: string,
      source: string,
    ) =>
      rpc<string>("version_content_source", {
        id,
        version,
        kind,
        file,
        source,
      }),
    async renameVersion(version: string, name: string) {
      await rpc("rename_version", { id: instanceId(), version, name });
      await sync();
    },
    async deleteVersion(version: string) {
      await rpc("delete_version", { id: instanceId(), version });
      await sync();
    },
  };
}
