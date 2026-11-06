<script setup lang="ts">
import { ref } from "vue";
import { Archive, Package, X } from "lucide-vue-next";
import ContentSwitch from "./ContentSwitch.vue";
import ModpackBrowser from "./ModpackBrowser.vue";
import ResourceLibraryBrowser from "./ResourceLibraryBrowser.vue";
import WindowControls from "./WindowControls.vue";
import { t } from "../i18n";
import type { VersionManifest } from "../types";
import type { ResourcePlatform } from "../resources";
const props = defineProps<{
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
const tab = ref("modpacks");
const active = ref(false);
const searchTarget = ref<HTMLElement>();
const filterTarget = ref<HTMLElement>();
function downloading(value: boolean) {
  active.value = value;
  emit("busy", value);
}
</script>
<template>
  <section class="modal glass-surface resource-store">
    <header class="modal-header">
      <h2 class="dialog-heading" tabindex="-1">{{ t("商城") }}</h2>
      <nav class="store-tabs" :aria-label="t('资源类型')">
        <button
          :class="{ active: tab === 'modpacks' }"
          :aria-pressed="tab === 'modpacks'"
          :disabled="active"
          @click="tab = 'modpacks'"
        >
          <Archive :size="16" />{{ t("整合包") }}
        </button>
        <button
          :class="{ active: tab === 'resources' }"
          :aria-pressed="tab === 'resources'"
          :disabled="active"
          @click="tab = 'resources'"
        >
          <Package :size="16" />{{ t("游戏资源") }}
        </button>
      </nav>
      <div ref="searchTarget" class="store-search" />
      <WindowControls />
      <button
        class="icon-button"
        :aria-label="t('关闭对话框')"
        :disabled="active"
        @click="emit('close')"
      >
        <X :size="18" />
      </button>
    </header>
    <div ref="filterTarget" class="store-filters" v-show="tab === 'modpacks'" />
    <ContentSwitch :content-key="tab" class="store-body">
      <ModpackBrowser
        v-if="tab === 'modpacks'"
        :rpc="rpc"
        :busy="busy"
        :download="downloadPack"
        :search-target="searchTarget"
        :filter-target="filterTarget"
        @error="emit('error', $event)"
        @link="emit('link', $event)"
      />
      <ResourceLibraryBrowser
        v-else
        embedded
        :rpc="rpc"
        :manifest="manifest"
        kind="mods"
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
.store-tabs {
  display: flex;
  gap: 16px;
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
}
.store-tabs button.active,
.store-tabs button:hover {
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.store-search {
  flex: 1;
  min-width: 0;
}
.store-filters {
  display: flex;
  justify-content: flex-end;
  padding: 0 var(--panel-inset) 10px;
}
.store-filters:empty {
  display: none;
}
.store-body {
  min-height: 0;
  flex: 1;
}
.store-body :deep(.pack-browser) {
  padding: 0 var(--panel-inset) var(--panel-inset);
  min-height: 320px;
}
.store-body :deep(.resource-header h2) {
  display: none;
}
</style>
