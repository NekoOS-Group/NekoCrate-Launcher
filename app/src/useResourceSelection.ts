import { computed, shallowRef } from "vue";

export function useResourceSelection<T>(
  items: () => readonly T[],
  key: (item: T) => string,
) {
  const keys = shallowRef(new Set<string>());
  let anchor: string | undefined;
  const selected = computed(() =>
    items().filter((item) => keys.value.has(key(item))),
  );
  function clear() {
    keys.value = new Set();
    anchor = undefined;
  }
  function click(
    event: Pick<MouseEvent, "shiftKey" | "metaKey" | "ctrlKey">,
    item: T,
  ) {
    const id = key(item),
      all = items();
    if (event.shiftKey) {
      const start = all.findIndex((entry) => key(entry) === anchor);
      const end = all.findIndex((entry) => key(entry) === id);
      if (end < 0) return false;
      const next =
        event.metaKey || event.ctrlKey
          ? new Set(keys.value)
          : new Set<string>();
      for (const entry of all.slice(
        Math.min(start < 0 ? end : start, end),
        Math.max(start < 0 ? end : start, end) + 1,
      ))
        next.add(key(entry));
      keys.value = next;
      anchor ??= id;
      return true;
    }
    if (event.metaKey || event.ctrlKey) {
      const next = new Set(keys.value);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      keys.value = next;
      anchor = id;
      return true;
    }
    clear();
    anchor = id;
    return false;
  }
  function keyboard(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if (
      target?.closest?.(
        "input, textarea, select, [contenteditable=true], .cm-editor",
      )
    )
      return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      event.stopPropagation();
      keys.value = new Set(items().map(key));
      anchor = items()[0] ? key(items()[0]!) : undefined;
    } else if (event.key === "Escape" && keys.value.size) {
      event.preventDefault();
      event.stopPropagation();
      clear();
    }
  }
  return {
    keys,
    selected,
    clear,
    click,
    keyboard,
    contains: (item: T) => keys.value.has(key(item)),
    drag: (item: T) => (keys.value.has(key(item)) ? selected.value : [item]),
  };
}

export async function changeSelectedMods<T>(
  items: readonly T[],
  enabled: boolean,
  state: (item: T) => boolean,
  change: (item: T, enabled: boolean) => Promise<string>,
  updated: (item: T, file: string) => void,
) {
  const failures: { item: T; error: unknown }[] = [];
  for (const item of items) {
    if (state(item) === enabled) continue;
    try {
      updated(item, await change(item, enabled));
    } catch (error) {
      failures.push({ item, error });
    }
  }
  return failures;
}
