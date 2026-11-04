import { shallowRef } from "vue";
export const resourceArrival = shallowRef<{
  instance: string;
  version: string;
  kind: string;
  files: string[];
  time: number;
}>();
export function arrivalFileKey(kind: string, file: string) {
  const path = file.replaceAll("\\", "/");
  return /^(mods|resourcepacks|shaderpacks|datapacks|saves)\//.test(path)
    ? path
    : `${kind}/${path}`;
}
export function announceResources(
  instance: string,
  version: string,
  kind: string,
  files: string[],
) {
  resourceArrival.value = {
    instance,
    version,
    kind,
    files: files.map((p) => arrivalFileKey(kind, p)),
    time: Date.now(),
  };
}
