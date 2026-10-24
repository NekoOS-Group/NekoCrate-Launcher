import type { VersionManifest } from "./types";

export type CatalogVersion = VersionManifest["versions"][number];
export type VersionCategory = "release" | "snapshot" | "development" | "april";

export const versionCategories = [
  { id: "release", label: "正式版", color: "#8bdeb0" },
  { id: "snapshot", label: "快照版", color: "#80cde8" },
  { id: "development", label: "开发版", color: "#e8bf7e" },
  { id: "april", label: "愚人节版", color: "#c7a4ec" },
] as const;

// Mojang's version_manifest_v2 labels these as snapshots. Match exact IDs:
// ordinary releases can also land on April 1 (e.g. 26.1.1).
// https://www.minecraft.net/en-us/article/the-herdcraft-update
const aprilFools = new Set([
  "15w14a",
  "1.RV-Pre1",
  "3D Shareware v1.34",
  "20w14infinite",
  "22w13oneblockatatime",
  "23w13a_or_b",
  "24w14potato",
  "25w14craftmine",
  "26w14a",
]);

export function versionCategory(version: CatalogVersion): VersionCategory {
  if (aprilFools.has(version.id)) return "april";
  if (version.type === "release") return "release";
  if (version.type === "old_alpha" || version.type === "old_beta")
    return "development";
  return "snapshot";
}

export function filterVersions(
  versions: readonly CatalogVersion[],
  category: VersionCategory,
  query: string,
): CatalogVersion[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  // Preserve the official catalog's chronology; lexical sorting breaks IDs.
  return versions.filter(
    (version) =>
      versionCategory(version) === category &&
      terms.every((term) => version.id.toLowerCase().includes(term)),
  );
}
