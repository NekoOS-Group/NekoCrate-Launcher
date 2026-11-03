import { onBeforeUnmount, ref, shallowRef } from "vue";

/** Append pages without replacing existing rows; invalidate old requests on filter changes. */
export function useCatalogFeed<T extends { id: string }>(
  fetchPage: (offset: number) => Promise<{ hits: T[]; total: number }>,
  onError?: (error: unknown) => void,
) {
  const hits = shallowRef<T[]>([]);
  const loading = ref(false);
  const failure = ref("");
  const more = ref(false);
  const revision = ref(0);
  const scrollTop = ref(0);
  let generation = 0,
    nextOffset = 0,
    disposed = false;
  function invalidate(waiting = false) {
    generation++;
    nextOffset = 0;
    hits.value = [];
    more.value = false;
    failure.value = "";
    loading.value = waiting;
    scrollTop.value = 0;
    revision.value++;
  }
  async function search(append = false) {
    if (disposed || (append && (loading.value || !more.value))) return;
    if (!append) invalidate();
    const request = generation;
    const offset = nextOffset;
    loading.value = true;
    failure.value = "";
    try {
      const result = await fetchPage(offset);
      if (disposed || request !== generation) return;
      const seen = new Set(hits.value.map((item) => item.id));
      const added = result.hits.filter((item) => {
        if (seen.has(item.id)) return false;
        seen.add(item.id);
        return true;
      });
      hits.value = [...hits.value, ...added];
      nextOffset = offset + result.hits.length;
      // Stop empty/repeated pages and respect the providers' search window.
      more.value =
        added.length > 0 && nextOffset < result.total && nextOffset < 10000;
    } catch (error) {
      if (!disposed && request === generation) {
        failure.value = String(error);
        onError?.(error);
      }
    } finally {
      if (!disposed && request === generation) loading.value = false;
    }
  }
  const retry = () => search(hits.value.length > 0);
  onBeforeUnmount(() => {
    disposed = true;
    generation++;
  });
  return {
    hits,
    loading,
    failure,
    more,
    revision,
    scrollTop,
    invalidate,
    search,
    retry,
  };
}
