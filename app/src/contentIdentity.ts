import type { LocalContentFile } from "./types.ts";

// These suffixes change loading state, not the identity of the Mod.
export function modFileName(file: string) {
  return file.replace(/\.(disabled|off)$/i, "");
}
export function modRowKeys(
  files: readonly Pick<LocalContentFile, "file_name">[],
) {
  const counts = new Map<string, number>();
  for (const file of files) {
    const name = modFileName(file.file_name);
    counts.set(name, (counts.get(name) || 0) + 1);
  }
  return new Map(
    files.map((file) => [
      file.file_name,
      counts.get(modFileName(file.file_name)) === 1
        ? `mod:${modFileName(file.file_name)}`
        : `file:${file.file_name}`,
    ]),
  );
}
export function anchorModFiles(
  previous: readonly LocalContentFile[],
  next: readonly LocalContentFile[],
  pending?: ReadonlySet<string>,
) {
  const remaining = new Map(next.map((file) => [file.file_name, file]));
  const oldNames = new Set(previous.map((file) => file.file_name));
  const renamed = new Map<string, LocalContentFile[]>();
  for (const file of next) {
    if (oldNames.has(file.file_name)) continue;
    const name = modFileName(file.file_name);
    const group = renamed.get(name) || [];
    group.push(file);
    renamed.set(name, group);
  }
  const files = previous.flatMap((old) => {
    let fresh = remaining.get(old.file_name);
    if (!fresh) {
      const matches = (renamed.get(modFileName(old.file_name)) || []).filter(
        (file) => remaining.has(file.file_name),
      );
      if (matches.length === 1) fresh = matches[0];
    }
    if (!fresh) return [];
    remaining.delete(fresh.file_name);
    // Keep displayed metadata until the pending archive read has a replacement.
    // Only presentation fields are retained; disk state and filename stay fresh.
    if (
      pending?.has(fresh.file_name) &&
      old.size === fresh.size &&
      (!old.declaration || old.declaration.status === "local") &&
      (!fresh.declaration || fresh.declaration.status === "local")
    ) {
      return [
        {
          ...fresh,
          name: old.name,
          version: old.version,
          mod_ids: old.mod_ids,
          has_metadata: fresh.has_metadata ?? old.has_metadata,
        },
      ];
    }
    return [fresh];
  });
  return [...files, ...remaining.values()];
}
