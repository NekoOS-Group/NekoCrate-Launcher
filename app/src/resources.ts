import type { Instance } from "./types";
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
    (i) => i.id === (target?.dataset.resourceInstance || fallback?.instance),
  );
  const version = target?.dataset.resourceVersion || fallback?.version;
  if (instance && version && instance.versions.some((v) => v.id === version))
    return { instance, version };
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
