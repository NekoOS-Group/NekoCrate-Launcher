import type { ResourcePlatform, ResourceProject } from "./resources.ts";
export interface DroppedProject {
  platform: ResourcePlatform;
  kind: string;
  project: ResourceProject;
}
export function projectDropUrl(value: string): string | undefined {
  const text = value.trim();
  if (text.length > 2048) return;
  try {
    const u = new URL(text);
    if (u.protocol !== "https:" || u.port || u.username || u.password) return;
    const path = u.pathname.split("/").filter(Boolean);
    const mr =
      ["modrinth.com", "www.modrinth.com"].includes(u.hostname) &&
      ["mod", "modpack", "shader", "datapack", "resourcepack"].includes(
        path[0],
      ) &&
      /^[\w-]+$/.test(path[1] || "");
    const cf =
      ["curseforge.com", "www.curseforge.com"].includes(u.hostname) &&
      path[0] === "minecraft" &&
      [
        "mc-mods",
        "modpacks",
        "texture-packs",
        "shaders",
        "data-packs",
        "datapacks",
        "worlds",
      ].includes(path[1]) &&
      /^[\w-]+$/.test(path[2] || "");
    return mr || cf ? u.href : undefined;
  } catch {
    return;
  }
}
export function draggedProjectUrl(data: DataTransfer | null) {
  const text =
    data
      ?.getData("text/uri-list")
      .split(/\r?\n/)
      .find((s) => s && !s.startsWith("#")) ||
    data?.getData("text/plain") ||
    "";
  return projectDropUrl(text);
}
