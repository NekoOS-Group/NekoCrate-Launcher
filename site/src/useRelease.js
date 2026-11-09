import { ref, onMounted, onBeforeUnmount } from "vue";
import fallback from "./release.json";
import { latestRelease, normalizeRelease, releaseApi } from "./releases.mjs";

const cacheKey = "nekocrate-site-release-v1";
export function useRelease() {
  const release = ref(normalizeRelease(fallback));
  let controller;
  onBeforeUnmount(() => controller?.abort());
  onMounted(async () => {
    try {
      const cached = JSON.parse(localStorage.getItem(cacheKey) || "null");
      const saved = normalizeRelease(cached?.release);
      if (
        saved &&
        Date.parse(saved.publishedAt) >= Date.parse(release.value.publishedAt)
      ) {
        release.value = saved;
        const age = Date.now() - cached.at;
        if (age >= 0 && age < 30 * 60 * 1000) return;
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
      const current = latestRelease(await response.json());
      if (
        !current ||
        Date.parse(current.publishedAt) < Date.parse(release.value.publishedAt)
      )
        return;
      release.value = current;
      try {
        localStorage.setItem(
          cacheKey,
          JSON.stringify({ at: Date.now(), release: current }),
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
  return release;
}
