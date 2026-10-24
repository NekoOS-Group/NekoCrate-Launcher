import type { InstanceVersion } from "./types";
import { t } from "./i18n.ts";

export function versionDescription(version: InstanceVersion): string {
  const game = version.minecraft_version || version.id;
  if (!version.loader) return `${game} · ${t("原版")}`;
  const names: Record<string, string> = {
    fabric: "Fabric",
    forge: "Forge",
    neoforge: "NeoForge",
    quilt: "Quilt",
  };
  return `${game} · ${names[version.loader.kind] || version.loader.kind} ${version.loader.version}`;
}
