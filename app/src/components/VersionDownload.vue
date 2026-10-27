<script setup lang="ts">
import { computed, ref } from "vue";
import {
  Archive,
  Box,
  Package,
  Download,
  LoaderCircle,
  X,
} from "lucide-vue-next";
import DownloadDestination from "./DownloadDestination.vue";
import VersionBrowser from "./VersionBrowser.vue";
import VersionCube from "./VersionCube.vue";
import GlassSelect from "./GlassSelect.vue";
import ContentSwitch from "./ContentSwitch.vue";
import { listRevealStyle } from "../listMotion";
import { versionCategories, versionCategory } from "../versionCatalog";
import { t } from "../i18n";
import type { Instance, VersionManifest } from "../types";

const props = defineProps<{
  section?: "game" | "modpacks" | "resources";
  target: Instance | null;
  instances: readonly Instance[];
  manifest: VersionManifest | null;
  version: string;
  loader: string;
  loaderVersion: string;
  loaderOptions: readonly { value: string; label: string; detail?: string }[];
  loaderLoading: boolean;
  unavailableLoaders: readonly string[];
  versionsLoading: boolean;
  disabled: boolean;
  submitting: boolean;
  canSubmit: boolean;
  name: string;
  icon: number;
  location: string;
  directory: string;
  root: string;
  picking: boolean;
  running: boolean;
}>();
defineEmits<{
  "update:section": [value: "game" | "modpacks" | "resources"];
  "update:version": [value: string];
  "update:loader": [value: string];
  "update:loaderVersion": [value: string];
  "update:name": [value: string];
  "update:icon": [value: number];
  importPack: [];
  retryLoader: [];
  "update:location": [value: string];
  target: [id: string | null];
  refresh: [];
  browse: [];
  submit: [];
  close: [];
}>();
const destination = ref<InstanceType<typeof DownloadDestination>>();
const sections = [
  { id: "game", label: "游戏版本", icon: Box },
  { id: "modpacks", label: "整合包", icon: Archive },
  { id: "resources", label: "资源", icon: Package },
] as const;
const loaderKinds = [
  { id: "vanilla", label: "原版" },
  { id: "fabric", label: "Fabric" },
  { id: "forge", label: "Forge" },
  { id: "neoforge", label: "NeoForge" },
];
const selected = computed(() =>
  props.manifest?.versions.find((item) => item.id === props.version),
);
const selectionDescription = computed(() => {
  if (!selected.value) return t("请选择版本");
  const loader =
    props.loader === "vanilla"
      ? t("原版")
      : [
          loaderKinds.find((item) => item.id === props.loader)?.label,
          props.loaderVersion,
        ]
          .filter(Boolean)
          .join(" ");
  return `${selected.value.id} · ${loader}`;
});
const selectionCategory = computed(() =>
  selected.value ? versionCategory(selected.value) : "release",
);
const selectionColor = computed(() =>
  selected.value
    ? versionCategories.find((item) => item.id === selectionCategory.value)
        ?.color
    : "var(--muted)",
);
</script>

<template>
  <section class="modal glass-surface version-download">
    <header class="modal-header">
      <h2 class="dialog-heading" tabindex="-1" autofocus>
        {{ t("添加新的版本") }}
      </h2>
      <DownloadDestination
        ref="destination"
        :target="target"
        :instances="instances"
        :name="name"
        :icon="icon"
        :location="location"
        :directory="directory"
        :root="root"
        :disabled="disabled"
        :picking="picking"
        @target="$emit('target', $event)"
        @update:name="$emit('update:name', $event)"
        @update:icon="$emit('update:icon', $event)"
        @update:location="$emit('update:location', $event)"
        @browse="$emit('browse')"
      />
      <button
        type="button"
        class="icon-button"
        :disabled="submitting"
        :aria-label="t('关闭对话框')"
        @click="$emit('close')"
      >
        <X :size="16" />
      </button>
    </header>
    <nav class="addition-sections" :aria-label="t('添加内容')">
      <button
        v-for="(item, index) in sections"
        :key="item.id"
        type="button"
        class="list-reveal-item"
        :style="listRevealStyle(index)"
        :class="{ active: (section || 'game') === item.id }"
        :aria-pressed="(section || 'game') === item.id"
        :disabled="disabled"
        @click="$emit('update:section', item.id)"
      >
        <component :is="item.icon" :size="16" />{{ t(item.label) }}
      </button>
    </nav>
    <ContentSwitch :content-key="section || 'game'">
      <form
        v-if="!section || section === 'game'"
        class="version-download-form"
        @submit.prevent="destination?.finishRename() && $emit('submit')"
      >
        <div class="version-download-body">
          <VersionBrowser
            :model-value="version"
            :manifest="manifest"
            :loading="versionsLoading"
            :disabled="disabled"
            @update:model-value="$emit('update:version', $event)"
            @refresh="$emit('refresh')"
          />
          <aside class="download-options" :aria-label="t('安装选项')">
            <div
              class="download-loader list-reveal-item"
              :style="listRevealStyle(2)"
            >
              <span class="loader-label">{{ t("加载器") }}</span>
              <div class="loader-kinds" role="group" :aria-label="t('加载器')">
                <button
                  v-for="item in loaderKinds"
                  :key="item.id"
                  type="button"
                  class="loader-kind"
                  :class="{ selected: loader === item.id }"
                  :aria-pressed="loader === item.id"
                  :disabled="
                    disabled ||
                    !selected ||
                    unavailableLoaders.includes(item.id)
                  "
                  @click="
                    loader === item.id && item.id !== 'vanilla'
                      ? $emit('retryLoader')
                      : $emit('update:loader', item.id)
                  "
                >
                  {{ t(item.label) }}
                </button>
              </div>
              <Transition name="loader-options">
                <div v-if="loader !== 'vanilla'" class="loader-version-row">
                  <span class="loader-label">{{ t("版本") }}</span>
                  <LoaderCircle
                    v-if="loaderLoading"
                    :size="14"
                    class="spin"
                    :aria-label="t('正在获取加载器版本…')"
                  />
                  <GlassSelect
                    v-else-if="loaderOptions.length"
                    id="download-loader-version"
                    :model-value="loaderVersion"
                    :options="loaderOptions"
                    :disabled="disabled"
                    floating
                    compact
                    literal-labels
                    @update:model-value="$emit('update:loaderVersion', $event)"
                  />
                  <button
                    v-else
                    type="button"
                    class="text-button"
                    :disabled="disabled"
                    @click="$emit('retryLoader')"
                  >
                    {{ t("重试") }}
                  </button>
                </div>
              </Transition>
            </div>
            <span v-if="running" class="download-detail" role="status">{{
              t("请先停止此实例的游戏")
            }}</span>
          </aside>
        </div>
        <footer
          class="modal-actions list-reveal-item"
          :style="listRevealStyle(3)"
          data-list-footer
        >
          <div class="download-selection" :title="selectionDescription">
            <VersionCube
              :variant="selectionCategory"
              :size="28"
              :style="{ color: selectionColor }"
            />
            <span>{{ selectionDescription }}</span>
          </div>
          <button
            type="button"
            class="text-button"
            :disabled="disabled"
            @click="$emit('importPack')"
          >
            {{ t("导入") }}
          </button>
          <button type="submit" class="button primary" :disabled="!canSubmit">
            <LoaderCircle v-if="submitting" :size="16" class="spin" /><Download
              v-else
              :size="16"
            />{{ t("下载并安装") }}
          </button>
        </footer>
      </form>
      <div v-else class="addition-content"><slot name="content" /></div>
    </ContentSwitch>
  </section>
</template>

<style scoped>
.version-download {
  width: min(840px, calc(100vw - 64px));
  height: min(590px, calc(100dvh - 64px));
}
.addition-sections {
  display: flex;
  justify-content: space-evenly;
  gap: 16px;
  padding: 8px 24px 12px;
  flex-shrink: 0;
}
.addition-sections button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  padding: 10px 20px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 13px;
  cursor: pointer;
}
.addition-sections button.active,
.addition-sections button:hover {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.addition-sections button.active svg {
  filter: drop-shadow(0 0 5px var(--glow));
}
.addition-content {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
}
.version-download .modal-header {
  margin-bottom: 0;
  gap: 16px;
}
.version-download .modal-header h2 {
  font-size: 16px;
}
.version-download .version-download-form {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
  overflow: hidden;
}
.version-download-body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 284px;
  flex: 1;
  min-height: 0;
}
.download-options {
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 24px;
  border-left: 1px solid var(--line);
}
.download-selection {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: 1;
  min-width: 0;
  margin-right: auto;
  font: 500 16px/24px var(--mono);
  color: var(--ink);
}
.download-selection > span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.download-loader {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.loader-label {
  font-size: 11px;
  color: var(--muted);
}
.loader-kinds {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}
.loader-kind {
  padding: 10px 6px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--muted);
  font: 500 12px/18px var(--mono);
  cursor: pointer;
  transition:
    color 180ms,
    text-shadow 180ms,
    border-color 180ms;
}
.loader-kind:hover,
.loader-kind.selected {
  color: var(--ink);
  text-shadow: 0 0 12px var(--accent);
}
.loader-kind.selected {
  border-color: var(--line);
}
.loader-kind:disabled {
  opacity: 0.3;
  cursor: default;
  text-shadow: none;
}
.loader-kind:focus-visible {
  outline: 1px solid var(--accent);
  outline-offset: 2px;
}
.loader-version-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 32px;
}
.loader-version-row .source-select {
  min-width: 0;
  flex: 1;
}
.download-detail {
  display: block;
  margin-top: 12px;
  color: var(--muted);
  font-size: 11px;
  line-height: 18px;
}
.download-create {
  margin-top: 24px;
}
.download-create .field {
  font-size: 11px;
}
.download-create :deep(.instance-location-heading) {
  flex-direction: column;
  align-items: stretch;
  gap: 4px;
}
.download-create :deep(.instance-location-heading .source-select) {
  max-width: 100%;
  width: 100%;
}
.version-download .modal-actions {
  padding: 16px 24px;
  border-top: 1px solid var(--line);
}
.loader-options-enter-active,
.loader-options-leave-active {
  transition:
    opacity 180ms var(--motion-ease),
    transform 240ms var(--motion-ease);
}
.loader-options-enter-from,
.loader-options-leave-to {
  opacity: 0;
  transform: translateY(-5px);
}
.loader-options-leave-active {
  position: absolute;
  pointer-events: none;
}
@media (max-width: 720px) {
  .version-download-body {
    grid-template-columns: minmax(0, 1fr) 230px;
  }
  .download-options {
    padding: 20px 16px;
  }
  .version-download .modal-header {
    padding-inline: 20px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .loader-options-enter-active,
  .loader-options-leave-active {
    transition: none;
  }
}
</style>
