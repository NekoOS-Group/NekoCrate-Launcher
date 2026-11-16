<script setup lang="ts">
import { computed, ref } from "vue";
import {
  Archive,
  FileCode2,
  Image,
  Map,
  Package,
  Plug,
  Server,
  Shirt,
  Sparkles,
  X,
} from "lucide-vue-next";
import ContentSwitch from "./ContentSwitch.vue";
import ModpackBrowser from "./ModpackBrowser.vue";
import ResourceLibraryBrowser from "./ResourceLibraryBrowser.vue";
import StoreCatalogBrowser from "./StoreCatalogBrowser.vue";
import StoreSkinBrowser from "./StoreSkinBrowser.vue";
import { t } from "../i18n";
import type { VersionManifest } from "../types";
import type { ResourceKind, ResourcePlatform } from "../resources";
defineProps<{
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
  manifest: VersionManifest | null;
  busy: boolean;
  downloadPack: (
    platform: ResourcePlatform,
    project: string,
    version: string,
  ) => Promise<boolean>;
}>();
const emit = defineEmits<{
  close: [];
  error: [value: unknown];
  link: [url: string];
  busy: [value: boolean];
  downloaded: [];
}>();
type ResourceTab = Exclude<ResourceKind, "saves">;
type StoreTab =
  "modpacks" | ResourceTab | "plugins" | "servers" | "maps" | "skins";
const tab = ref<StoreTab>("modpacks");
const resourceCategories = [
  { id: "mods", label: "Mods", icon: Package, color: "#69dcb5" },
  { id: "resourcepacks", label: "资源包", icon: Image, color: "#b49aef" },
  { id: "datapacks", label: "数据包", icon: FileCode2, color: "#e7bc76" },
  { id: "shaderpacks", label: "光影", icon: Sparkles, color: "#e6c68a" },
] as const;
const extraCategories = [
  { id: "plugins", label: "插件", icon: Plug, color: "#8bc5e9" },
  { id: "servers", label: "服务器", icon: Server, color: "#79d3d0" },
  { id: "maps", label: "地图", icon: Map, color: "#7dbfee" },
  { id: "skins", label: "皮肤", icon: Shirt, color: "#e7a9c5" },
] as const;
const resourceKind = computed<ResourceTab>(
  () =>
    resourceCategories.find((category) => category.id === tab.value)?.id ||
    "mods",
);
const contentKey = computed(() =>
  resourceCategories.some((category) => category.id === tab.value)
    ? "resources"
    : tab.value,
);
const active = ref(false);
const searchTarget = ref<HTMLElement>();
const filterTarget = ref<HTMLElement>();
function downloading(value: boolean) {
  active.value = value;
  emit("busy", value);
}
function scrollTabs(event: WheelEvent) {
  const nav = event.currentTarget as HTMLElement;
  if (
    nav.scrollWidth <= nav.clientWidth ||
    Math.abs(event.deltaX) >= Math.abs(event.deltaY)
  )
    return;
  event.preventDefault();
  nav.scrollLeft += event.deltaY;
}
</script>
<template>
  <section class="modal glass-surface resource-store">
    <header class="modal-header">
      <h2 class="dialog-heading" tabindex="-1">{{ t("商城") }}</h2>
      <nav class="store-tabs" :aria-label="t('资源类型')" @wheel="scrollTabs">
        <button
          :class="{ active: tab === 'modpacks' }"
          :aria-pressed="tab === 'modpacks'"
          :disabled="active"
          @click="tab = 'modpacks'"
        >
          <Archive :size="16" />{{ t("整合包") }}
        </button>
        <button
          v-for="category in resourceCategories"
          :key="category.id"
          :data-drop-kind="category.id"
          data-resource-library
          :class="{ active: tab === category.id }"
          :style="{ '--category-color': category.color }"
          :aria-pressed="tab === category.id"
          :disabled="active"
          @click="tab = category.id"
        >
          <component :is="category.icon" :size="16" />{{ t(category.label) }}
        </button>
        <button
          v-for="category in extraCategories"
          :key="category.id"
          :class="{ active: tab === category.id }"
          :style="{ '--category-color': category.color }"
          :aria-pressed="tab === category.id"
          :disabled="active"
          @click="tab = category.id"
        >
          <component :is="category.icon" :size="16" />{{ t(category.label) }}
        </button>
      </nav>
      <button
        class="icon-button"
        :aria-label="t('关闭对话框')"
        :disabled="active"
        @click="emit('close')"
      >
        <X :size="18" />
      </button>
    </header>
    <ContentSwitch :content-key="contentKey" class="store-body">
      <template v-if="tab === 'modpacks'">
        <div class="catalog-toolbar store-toolbar">
          <div ref="searchTarget" class="store-search" />
          <div ref="filterTarget" class="catalog-filter-group" />
        </div>
        <ModpackBrowser
          :rpc="rpc"
          :busy="busy"
          :download="downloadPack"
          :search-target="searchTarget"
          :filter-target="filterTarget"
          @error="emit('error', $event)"
          @link="emit('link', $event)"
        />
      </template>
      <StoreSkinBrowser
        v-else-if="tab === 'skins'"
        :rpc="rpc"
        :busy="busy && !active"
        @error="emit('error', $event)"
        @link="emit('link', $event)"
        @busy="downloading"
        @downloaded="emit('downloaded')"
      />
      <StoreCatalogBrowser
        v-else-if="tab === 'plugins' || tab === 'maps' || tab === 'servers'"
        :kind="tab"
        :rpc="rpc"
        :manifest="manifest"
        :busy="busy && !active"
        @error="emit('error', $event)"
        @link="emit('link', $event)"
        @busy="downloading"
        @downloaded="emit('downloaded')"
      />
      <ResourceLibraryBrowser
        v-else
        embedded
        hide-categories
        :rpc="rpc"
        :manifest="manifest"
        :kind="resourceKind"
        :busy="busy && !active"
        @back="emit('close')"
        @busy="downloading"
        @downloaded="emit('downloaded')"
        @error="emit('error', $event)"
        @link="emit('link', $event)"
      />
    </ContentSwitch>
  </section>
</template>
<style scoped>
.resource-store {
  width: 820px;
  display: flex;
  flex-direction: column;
  min-height: 360px;
}
.resource-store > .modal-header {
  align-items: center;
  gap: 16px;
}
.resource-store > .modal-header h2 {
  flex: none;
}
.store-tabs {
  display: flex;
  flex: 1;
  min-width: 0;
  gap: 14px;
  overflow-x: auto;
  scrollbar-width: none;
}
.store-tabs::-webkit-scrollbar {
  display: none;
}
.store-tabs button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 0;
  padding: 4px 0;
  background: transparent;
  color: var(--muted);
  white-space: nowrap;
  flex-shrink: 0;
  font-size: 12px;
}
.store-tabs button.active,
.store-tabs button:hover:not(:disabled) {
  color: var(--category-color, var(--blue));
  text-shadow: 0 0 10px var(--category-color, var(--glow));
}
.store-tabs button.active svg {
  filter: drop-shadow(0 0 7px var(--category-color, var(--glow)));
}
.store-toolbar {
  padding: 0 var(--panel-inset) 10px;
}
.store-toolbar:not(:has(.pack-search, .pack-filters)) {
  display: none;
}
.store-search {
  min-width: 0;
}
.store-body {
  min-height: 0;
  flex: 1;
}
.store-body :deep(.content-switch-leave-active .catalog-toolbar) {
  /* Keep the outgoing list in place without drawing a second filter row. */
  visibility: hidden;
}
.store-body :deep(.pack-browser) {
  padding: 0 var(--panel-inset) var(--panel-inset);
  min-height: 320px;
}
.store-body :deep(.resource-embedded) {
  padding: 0 var(--panel-inset) var(--panel-inset);
}
.store-body :deep(.resource-header h2) {
  display: none;
}
</style>
