<script setup>
import {
  ref,
  computed,
  watchEffect,
  defineAsyncComponent,
  nextTick,
  watch,
  onMounted,
  onBeforeUnmount,
} from "vue";
import { ArrowUpRight, Github, Languages } from "lucide-vue-next";
import WorldBackdrop from "../../app/src/components/WorldBackdrop.vue";
import { backdropThemes } from "../../app/src/backdropThemes";
import logo from "./icon.webp";
import Download from "./pages/Download.vue";
import { repositoryUrl } from "./releases.mjs";
import { route, href, pages as pageIds } from "./router";
import { lang, t, toggleLang } from "./i18n";

const views = {
  download: Download,
  features: defineAsyncComponent(() => import("./pages/Features.vue")),
  docs: defineAsyncComponent(() => import("./pages/Docs.vue")),
};
// The page on screen lags the route by one fade so the swap is never visible.
const shown = ref(route.value.page);
const view = computed(() => views[shown.value]);
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)");
let swapping = Promise.resolve();
watch(
  () => route.value.page,
  (next) => {
    swapping = swapping.then(async () => {
      if (next !== route.value.page || next === shown.value) return;
      const el = main.value;
      const animate = el?.animate && !reduceMotion.matches;
      if (animate)
        // Never let a throttled animation hold the page swap hostage.
        await Promise.race([
          el.animate(
            [{ opacity: 1 }, { opacity: 0, transform: "translateY(-4px)" }],
            { duration: 140, easing: "ease-in", fill: "forwards" },
          ).finished.catch(() => {}),
          new Promise((resolve) => setTimeout(resolve, 180)),
        ]);
      shown.value = route.value.page;
      await nextTick();
      window.scrollTo({ top: 0, behavior: "instant" });
      if (animate) {
        el.getAnimations().forEach((a) => a.cancel());
        el.animate(
          [{ opacity: 0, transform: "translateY(10px)" }, { opacity: 1, transform: "none" }],
          { duration: 380, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
        );
      }
      el?.focus({ preventScroll: true });
    });
  },
);
const main = ref();
const nav = ref();
const indicator = ref({ x: 0, w: 0, ready: false });
const scrolled = ref(false);

// The nav highlight slides between links instead of jumping.
function placeIndicator() {
  const link = nav.value?.querySelector('[aria-current="page"]');
  if (!link) return;
  indicator.value = {
    x: link.offsetLeft,
    w: link.offsetWidth,
    ready: indicator.value.ready || indicator.value.w > 0,
  };
}
watch([() => route.value.page, lang], () => nextTick(placeIndicator));
// Changing language re-renders every string; a quick fade hides the reflow.
watch(lang, () => {
  if (!reduceMotion.matches)
    main.value?.animate([{ opacity: 0.3 }, { opacity: 1 }], { duration: 260, easing: "ease-out" });
});
function onScroll() {
  scrolled.value = window.scrollY > 8;
}
onMounted(() => {
  placeIndicator();
  document.fonts?.ready.then(placeIndicator);
  window.addEventListener("resize", placeIndicator);
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();
});
onBeforeUnmount(() => {
  window.removeEventListener("resize", placeIndicator);
  window.removeEventListener("scroll", onScroll);
});

watchEffect(() => {
  if (route.value.page !== "docs") document.title = t.value.meta[route.value.page];
});

const themeKey = "nekocrate-site-theme";
const theme = ref("tide");
try {
  const saved = localStorage.getItem(themeKey);
  if (backdropThemes.some((item) => item.id === saved)) theme.value = saved;
} catch {
  /* Theme persistence is optional. */
}
function setTheme(id) {
  theme.value = id;
  try {
    localStorage.setItem(themeKey, id);
  } catch {
    /* optional */
  }
}
const themeName = (item) =>
  lang.value === "zh" ? item.name : item.id[0].toUpperCase() + item.id.slice(1);
</script>

<template>
  <div class="site-shell" :data-theme="theme">
    <a class="skip-link" href="#main" @click.prevent="main?.focus()">{{
      t.skip
    }}</a>
    <div class="backdrop-host" aria-hidden="true">
      <WorldBackdrop :key="theme" :theme="theme" :paused="false" />
    </div>
    <header class="site-header" :class="{ scrolled }">
      <div class="site-header-inner wrap">
      <a :href="href('download')" class="brand" :aria-label="t.home">
        <img :src="logo" width="36" height="36" alt="" />
        <span>NekoCrate</span>
      </a>
      <nav ref="nav" class="site-nav" :aria-label="t.nav.main">
        <span
          class="nav-indicator"
          :class="{ ready: indicator.ready }"
          :style="{ translate: `${indicator.x}px 0`, width: `${indicator.w}px` }"
          aria-hidden="true"
        />
        <a
          v-for="id in pageIds"
          :key="id"
          :href="href(id)"
          :aria-current="route.page === id ? 'page' : undefined"
          >{{ t.nav[id] }}</a
        >
      </nav>
      <div class="header-tools">
        <button
          class="lang-button"
          :aria-label="t.language"
          :title="t.language"
          @click="toggleLang"
        >
          <Languages :size="17" /><span>{{ t.languageShort }}</span>
        </button>
        <a
          class="source-link"
          :href="repositoryUrl"
          target="_blank"
          rel="noopener noreferrer"
          :aria-label="t.repo"
          ><Github :size="18" /><span>GitHub</span></a
        >
      </div>
      </div>
    </header>
    <main id="main" ref="main" class="wrap" tabindex="-1">
      <KeepAlive>
        <component :is="view" :key="shown" />
      </KeepAlive>
    </main>
    <footer class="site-footer wrap">
      <div class="theme-picker" role="group" :aria-label="t.themes">
        <button
          v-for="item in backdropThemes"
          :key="item.id"
          :title="themeName(item)"
          :aria-label="`${themeName(item)}${t.themeSuffix}`"
          :aria-pressed="theme === item.id"
          :style="{ '--theme-color': item.color }"
          @click="setTheme(item.id)"
        >
          <svg viewBox="0 0 13 14" width="17" height="18" aria-hidden="true">
            <path :d="item.mark" />
          </svg>
        </button>
      </div>
      <p class="footer-links">
        <span>© {{ new Date().getFullYear() }} NekoOS Group</span>
        <a
          :href="`${repositoryUrl}/blob/neko_dev/LICENSE`"
          target="_blank"
          rel="noopener noreferrer"
          >GPL-3.0</a
        >
        <a
          :href="`${repositoryUrl}/issues`"
          target="_blank"
          rel="noopener noreferrer"
          >{{ t.feedback }}<ArrowUpRight :size="12"
        /></a>
      </p>
      <p class="trademark">{{ t.trademark }}</p>
    </footer>
  </div>
</template>
