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
  id: string;
  name: string;
  description: string;
  downloads: number;
  url: string;
}
export interface ResourceVersion {
  id: string;
  project: string;
  name: string;
  number: string;
  release_type: string;
}
export interface DetectedResource {
  path: string;
  name: string;
  kinds: ResourceKind[];
  format: "resource" | "modpack" | "directory" | "unknown";
  issue: string | null;
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
      ? file.format !== "resource"
      : kind === "instance"
        ? !["resource", "modpack"].includes(file.format)
        : kind === "modpack" || kind === "directory"
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
): "resources" | "modpack" | "folders" | "mixed" {
  if (files.length && files.every((f) => f.format === "directory"))
    return "folders";
  if (files.length === 1 && files[0].format === "modpack") return "modpack";
  if (files.length && files.every((f) => f.format === "resource"))
    return "resources";
  return "mixed";
}
