import type { Instance } from "./types.ts";
import { t } from "./i18n.ts";
export type ResourceKind =
  "mods" | "resourcepacks" | "datapacks" | "shaderpacks" | "saves";
export type ResourcePlatform = "modrinth" | "curseforge";
export interface ResourceCatalogScope {
  minecraft: string;
  loader: { kind: string; version: string } | null;
  kind: ResourceKind;
  platform: ResourcePlatform;
}
export interface ResourceLibraryDestination {
  minecraft: string;
  loader: { kind: string; version: string } | null;
  kind: ResourceKind;
}
export interface ResourceTarget {
  instance: string;
  version: string;
  directory: string;
  minecraft: string;
  loader: { kind: string; version: string } | null;
  worlds: string[];
  shared: boolean;
}
export interface ResourceProject {
  updated?: string | null;
  latest_version?: string | null;
  icon_url?: string | null;
  id: string;
  name: string;
  description: string;
  downloads: number;
  authors?: string[];
  categories?: string[];
  url: string;
}
export interface ResourceVersion {
  id: string;
  project: string;
  name: string;
  number: string;
  release_type: string;
  published?: string | null;
}
export interface ResourceSelection {
  icon_url?: string | null;
  platform: ResourcePlatform;
  kind: ResourceKind;
  project: string;
  version: string;
  name: string;
  number: string;
}
export interface DetectedResource {
  path: string;
  name: string;
  kinds: ResourceKind[];
  format:
    | "resource"
    | "modpack"
    | "directory"
    | "unknown"
    | "instance_archive"
    | "version_directory"
    | "versions_directory";
  issue: string | null;
  game_directories?: { prefix: string; name: string }[];
  game_directory?: string;
  version?: string;
}
export interface ResourcePlan {
  id: string;
  target: ResourceTarget;
  created_at: number;
  complete: boolean;
  conflicts: string[];
  warnings: string[];
  entries: {
    destination: string;
    kind: ResourceKind;
    size: number;
    reuse: boolean;
    origin: {
      name: string;
      platform: string;
      project: string | null;
      version: string | null;
    };
  }[];
}
export interface ResourceSession {
  platform?: ResourcePlatform;
  instance: Instance;
  version: string;
  kind: ResourceKind;
  files?: DetectedResource[];
  world?: string;
  project?: ResourceProject;
}
export function resourceDropTarget(
  element: Element | null,
  instances: readonly Instance[],
  fallback?: { instance: string; version: string },
): { instance: Instance; version: string } | undefined {
  const target = element?.closest<HTMLElement>("[data-resource-instance]");
  const instance = instances.find(
    (i) =>
      i.id === (target ? target.dataset.resourceInstance : fallback?.instance),
  );
  const version = target ? target.dataset.resourceVersion : fallback?.version;
  if (instance && version && instance.versions.some((v) => v.id === version))
    return { instance, version };
}

export const resourceKindLabel = (kind: string): string =>
  t(
    (
      {
        mods: "Mods",
        resourcepacks: "资源包",
        shaderpacks: "光影",
        datapacks: "数据包",
        saves: "存档",
        modpack: "整合包",
        directory: "游戏目录",
        instance_archive: "游戏目录压缩包",
        version_directory: "版本目录",
        versions_directory: "版本集合目录",
        version: "游戏资源",
        instance: "游戏资源 / 整合包",
      } as Record<string, string>
    )[kind] || kind,
  );

const inactiveDropKinds = [
  "metadata",
  "libraries",
  "cache",
  "blocked",
  "resources",
];
const nonResourceDropKinds = [...inactiveDropKinds, "skins", "java", "config"];
function resourceFileMatches(file: DetectedResource, kind?: string): boolean {
  // A failed read (e.g. a damaged archive or a denied path) is an actionable
  // error, not evidence that the file belongs to a different resource type.
  if (file.format === "unknown")
    return !!file.issue && (!kind || !nonResourceDropKinds.includes(kind));
  if (!kind) return true;
  if (nonResourceDropKinds.includes(kind)) return false;
  if (kind === "selection")
    return file.format === "resource" && !file.kinds.includes("datapacks");
  if (kind === "version")
    return ["resource", "version_directory"].includes(file.format);
  if (kind === "instance")
    return ["resource", "modpack", "version_directory"].includes(file.format);
  if (kind === "directory")
    return [
      "directory",
      "versions_directory",
      "version_directory",
      "instance_archive",
    ].includes(file.format);
  if (kind === "modpack") return file.format === kind;
  return (
    file.format === "resource" && file.kinds.includes(kind as ResourceKind)
  );
}
/** Type mismatches are not drop targets; operational/compatibility errors can
 * still be shown on an otherwise matching target. A ZIP's detected contents,
 * never its extension or the hovered region, determine its resource type. */
export function resourceDropTypeMatches(
  files: readonly DetectedResource[],
  kind?: string,
): boolean {
  return (
    files.length > 0 && files.every((file) => resourceFileMatches(file, kind))
  );
}

// These dedicated targets don't use game-resource archive detection.
export function dedicatedDropTypeMatches(
  paths: readonly string[],
  kind?: string,
): boolean {
  if (
    !paths.length ||
    kind === "selection" ||
    inactiveDropKinds.includes(kind || "")
  )
    return false;
  if (kind === "skins") return paths.every((path) => /\.png$/i.test(path));
  if (kind === "config")
    return paths.every((path) =>
      /\.(json5?|toml|cfg|conf|properties|yaml|yml|ini)$/i.test(path),
    );
  if (kind === "java")
    return paths.every((path) => /(?:^|[/\\])javaw?(?:\.exe)?$/i.test(path));
  return true;
}

/** An explicit target is a constraint, never a hint to reinterpret a ZIP. */
export function resourceDropIssue(
  files: readonly DetectedResource[],
  kind?: string,
): string | undefined {
  if (!files.length) return t("没有可导入的文件");
  const unknown = files.find((file) => file.format === "unknown");
  if (unknown) return unknown.issue || t("无法识别文件内容");
  if (!kind) return;
  if (kind === "selection") {
    if (files.some((file) => file.kinds.includes("datapacks")))
      return t("数据包需要目标存档，请在版本安装后拖入存档");
    if (files.every((file) => file.format === "resource")) return;
    return t("此处只接收 Mods、资源包、光影和存档");
  }
  if (nonResourceDropKinds.includes(kind))
    return t("请拖到目标版本或对应的资源区域");
  const mismatch = files.find((file) => !resourceFileMatches(file, kind));
  if (mismatch)
    return t("{name} 是{actual}，此处只接收{expected}", {
      name: mismatch.name,
      actual:
        mismatch.format === "resource"
          ? mismatch.kinds.map(resourceKindLabel).join(" / ")
          : resourceKindLabel(mismatch.format),
      expected: resourceKindLabel(kind),
    });
}
export function routeResourceDrop(
  files: readonly DetectedResource[],
): "resources" | "modpack" | "folders" | "game" | "mixed" {
  if (
    files.length === 1 &&
    ["instance_archive", "version_directory"].includes(files[0].format)
  )
    return "game";
  if (
    files.length &&
    files.every(
      (f) =>
        ["directory", "versions_directory"].includes(f.format) ||
        (f.format === "version_directory" && f.game_directory),
    )
  )
    return "folders";
  if (files.length === 1 && files[0].format === "modpack") return "modpack";
  if (files.length && files.every((f) => f.format === "resource"))
    return "resources";
  return "mixed";
}
