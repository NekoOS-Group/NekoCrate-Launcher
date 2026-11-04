import type { Instance } from "./types.ts";
import { t } from "./i18n.ts";
export type ResourceKind =
  "mods" | "resourcepacks" | "datapacks" | "shaderpacks" | "saves";
export type ResourcePlatform = "modrinth" | "curseforge";
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

/** An explicit target is a constraint, never a hint to reinterpret a ZIP. */
export function resourceDropIssue(
  files: readonly DetectedResource[],
  kind?: string,
): string | undefined {
  if (!files.length) return t("没有可导入的文件");
  const unknown = files.find((file) => file.format === "unknown");
  if (unknown) return unknown.issue || t("无法识别文件内容");
  if (!kind) return;
  if (["metadata", "libraries", "cache", "blocked", "resources"].includes(kind))
    return t("请拖到目标版本或对应的资源区域");
  const mismatch = files.find((file) =>
    kind === "version"
      ? !["resource", "version_directory"].includes(file.format)
      : kind === "instance"
        ? !["resource", "modpack", "version_directory"].includes(file.format)
        : kind === "directory"
          ? ![
              "directory",
              "versions_directory",
              "version_directory",
              "instance_archive",
            ].includes(file.format)
          : kind === "modpack"
            ? file.format !== kind
            : file.format !== "resource" ||
              !file.kinds.includes(kind as ResourceKind),
  );
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
