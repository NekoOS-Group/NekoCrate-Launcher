import type { ResourceRpc } from "./projectPresentation";

export const cacheResourceParts = {
  java: { label: "Java", color: "#e7bc76", tab: "java" },
  skins: { label: "皮肤", color: "#b49aef", tab: "skins" },
  mods: { label: "Mods", color: "#69dcb5", tab: "resources" },
  resourcepacks: { label: "资源包", color: "#f19a82", tab: "resources" },
  shaderpacks: { label: "光影", color: "#80d9ed", tab: "resources" },
  datapacks: { label: "数据包", color: "#7dbfee", tab: "resources" },
} as const;
export type CacheResourceKind = keyof typeof cacheResourceParts;

/** Reuse local-content presentation components without inventing a game instance. */
export function createResourceLibraryRpc(rpc: ResourceRpc): ResourceRpc {
  const operations: Record<string, string> = {
    version_content_presentation: "cached_resource_presentation",
    version_content_metadata: "cached_resource_metadata",
    version_content_source: "cached_resource_source",
    open_content_folder: "open_cached_resource",
  };
  return (op, data) => {
    const action = operations[op];
    if (!action || data?.id !== "" || data?.version !== "")
      return rpc(op, data);
    return rpc("resource_manager", {
      action: {
        kind: action,
        content: data.kind,
        file: data.file,
        source: data.source,
      },
    });
  };
}
