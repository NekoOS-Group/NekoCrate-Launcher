import { ref, onMounted, onBeforeUnmount } from "vue";
import fallback from "./release.json";
import { listReleases, normalizeRelease, releaseApi } from "./releases.mjs";

const cacheKey = "nekocrate-site-releases-v2";
const maxAge = 30 * 60 * 1000;
const newest = (list) => Date.parse(list[0]?.publishedAt ?? 0);

// Newest first. Starts with the bundled snapshot so links work offline.
export function useReleases() {
  const releases = ref([normalizeRelease(fallback)].filter(Boolean));
  let controller;
  onBeforeUnmount(() => controller?.abort());
  onMounted(async () => {
    try {
      const cached = JSON.parse(localStorage.getItem(cacheKey) || "null");
      const saved = listReleases(cached?.releases);
      if (saved.length && newest(saved) >= newest(releases.value)) {
        releases.value = saved;
        const age = Date.now() - cached.at;
        if (age >= 0 && age < maxAge) return;
      }
    } catch {
      /* Storage may be unavailable; the bundled release stays usable. */
    }
    controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch(releaseApi, {
        signal: controller.signal,
        headers: { Accept: "application/vnd.github+json" },
      });
      if (!response.ok) return;
      const current = listReleases(await response.json());
      if (!current.length || newest(current) < newest(releases.value)) return;
      releases.value = current;
      try {
        localStorage.setItem(
          cacheKey,
          JSON.stringify({ at: Date.now(), releases: current }),
        );
      } catch {
        /* optional cache */
      }
    } catch {
      /* Network failures never replace working download links. */
    } finally {
      clearTimeout(timer);
    }
  });
  return releases;
}
