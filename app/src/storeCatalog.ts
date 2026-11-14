import type {
  ResourcePlatform,
  ResourceProject,
  ResourceVersion,
} from "./resources";
import type { ProjectDetails } from "./projectPresentation";
export type StoreKind = "plugins" | "maps" | "servers";
export interface StoreScope {
  kind: StoreKind;
  platform: ResourcePlatform;
  minecraft: string;
  loader: string;
  category: string;
}
export interface StoreVersion extends ResourceVersion {
  files: { name: string; primary: boolean }[];
  games: string[];
  loaders: string[];
}
export interface StoreServer {
  project: ResourceProject;
  address: string | null;
  online: number | null;
  details: ProjectDetails;
}
export function storeScope(
  kind: StoreKind,
  platform: ResourcePlatform,
  minecraft: string,
  loader: string,
  category = "",
): StoreScope {
  return {
    kind,
    platform:
      kind === "maps"
        ? "curseforge"
        : kind === "servers"
          ? "modrinth"
          : platform,
    minecraft,
    loader: kind === "plugins" && platform === "modrinth" ? loader : "",
    category: kind === "plugins" ? "" : category,
  };
}
export function storeFilename(version: StoreVersion): string | undefined {
  const primary = version.files.filter((file) => file.primary);
  const name = (
    primary.length === 1
      ? primary[0]
      : version.files.length === 1
        ? version.files[0]
        : undefined
  )?.name;
  return name && !/[\\/\x00-\x1f]/.test(name) ? name : undefined;
}
