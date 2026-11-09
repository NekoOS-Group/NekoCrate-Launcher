<script setup>
import {
  computed,
  ref,
  onMounted,
  onBeforeUnmount,
  defineAsyncComponent,
} from "vue";
import { ArrowUpRight, Github, Menu, X } from "lucide-vue-next";
import WorldBackdrop from "../../app/src/components/WorldBackdrop.vue";
import { backdropThemes } from "../../app/src/backdropThemes";
import logo from "./icon.webp";
import Download from "./pages/Download.vue";
import { repositoryUrl } from "./releases.mjs";

const Features = defineAsyncComponent(() => import("./pages/Features.vue"));
const Docs = defineAsyncComponent(() => import("./pages/Docs.vue"));
const base = import.meta.env.BASE_URL;
const section = /\/features\/?(?:index.html)?$/.test(location.pathname)
  ? "features"
  : /\/docs\/?(?:index.html)?$/.test(location.pathname)
    ? "docs"
    : "download";
const page = computed(
  () => ({ download: Download, features: Features, docs: Docs })[section],
);
const menu = ref(false);
const theme = ref("tide");
try {
  const saved = localStorage.getItem("nekocrate-site-theme");
  if (backdropThemes.some((item) => item.id === saved)) theme.value = saved;
} catch {
  /* Theme persistence is optional. */
}
function setTheme(id) {
  theme.value = id;
  try {
    localStorage.setItem("nekocrate-site-theme", id);
  } catch {
    /* optional */
  }
}
function escape(event) {
  if (event.key === "Escape") menu.value = false;
}
onMounted(() => document.addEventListener("keydown", escape));
onBeforeUnmount(() => document.removeEventListener("keydown", escape));
</script>

<template>
  <div class="site-shell" :data-theme="theme">
    <a class="skip-link" href="#main">跳到内容</a>
    <div class="backdrop-host">
      <WorldBackdrop :key="theme" :theme="theme" :paused="false" />
    </div>
    <header class="site-header wrap">
      <a :href="base" class="brand" aria-label="猫咪盒子启动器首页">
        <img :src="logo" width="42" height="42" alt="" />
        <span>NekoCrate<span class="brand-sub">猫咪盒子启动器</span></span>
      </a>
      <nav class="desktop-nav" aria-label="主导航">
        <a
          :href="base"
          :aria-current="section === 'download' ? 'page' : undefined"
          >下载</a
        >
        <a
          :href="`${base}features/`"
          :aria-current="section === 'features' ? 'page' : undefined"
          >功能与设计</a
        >
        <a
          :href="`${base}docs/`"
          :aria-current="section === 'docs' ? 'page' : undefined"
          >文档</a
        >
      </nav>
      <a
        class="source-link"
        :href="repositoryUrl"
        target="_blank"
        rel="noopener noreferrer"
        ><Github :size="18" /><span>GitHub</span><ArrowUpRight :size="13"
      /></a>
      <button
        class="icon-button mobile-menu"
        aria-label="切换导航"
        :aria-expanded="menu"
        aria-controls="mobile-nav"
        @click="menu = !menu"
      >
        <X v-if="menu" :size="21" /><Menu v-else :size="21" />
      </button>
      <nav
        v-if="menu"
        id="mobile-nav"
        class="mobile-nav glass"
        aria-label="移动端导航"
      >
        <a
          :href="base"
          :aria-current="section === 'download' ? 'page' : undefined"
          >下载</a
        >
        <a
          :href="`${base}features/`"
          :aria-current="section === 'features' ? 'page' : undefined"
          >功能与设计</a
        >
        <a
          :href="`${base}docs/`"
          :aria-current="section === 'docs' ? 'page' : undefined"
          >文档</a
        >
      </nav>
    </header>
    <main id="main" class="wrap page-enter" tabindex="-1">
      <component :is="page" />
    </main>
    <footer class="site-footer wrap">
      <div class="theme-picker" role="group" aria-label="主题配色">
        <button
          v-for="item in backdropThemes"
          :key="item.id"
          :aria-label="`${item.name}主题`"
          :aria-pressed="theme === item.id"
          :style="{ '--theme-color': item.color }"
          @click="setTheme(item.id)"
        >
          <svg viewBox="0 0 13 14" width="17" height="18" aria-hidden="true">
            <path :d="item.mark" />
          </svg>
        </button>
      </div>
      <div class="footer-links">
        <span>© {{ new Date().getFullYear() }} NekoOS Group</span
        ><a
          :href="`${repositoryUrl}/blob/neko_dev/LICENSE`"
          target="_blank"
          rel="noopener noreferrer"
          >GPL-3.0</a
        ><a
          :href="`${repositoryUrl}/issues`"
          target="_blank"
          rel="noopener noreferrer"
          >反馈问题 <ArrowUpRight :size="12"
        /></a>
      </div>
    </footer>
    <p class="trademark wrap">
      独立开源项目，与 Mojang 或 Microsoft 无隶属关系。
    </p>
  </div>
</template>
