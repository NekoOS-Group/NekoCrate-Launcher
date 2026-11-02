<script setup lang="ts">
import { computed, ref } from "vue";
import {
  Archive,
  Package,
  ChevronRight,
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
import {
  versionCategories,
  versionCategory,
  type VersionCategory,
} from "../versionCatalog";
import { t } from "../i18n";
import type { Instance, VersionManifest } from "../types";
import type { ResourceSelection } from "../resources";

const props = defineProps<{
  section?: "game" | "modpacks" | "resources";
  resources: readonly ResourceSelection[];
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
const emit = defineEmits<{
  "update:section": [value: "game" | "modpacks" | "resources"];
  "update:version": [value: string];
  "update:loader": [value: string];
  "update:loaderVersion": [value: string];
  "update:name": [value: string];
  "update:icon": [value: number];
  importPack: [];
  removeResource: [value: ResourceSelection];
  retryLoader: [];
  "update:location": [value: string];
  target: [id: string | null];
  refresh: [];
  browse: [];
  submit: [];
  close: [];
}>();
const destination = ref<InstanceType<typeof DownloadDestination>>();
const category = ref<VersionCategory>("release");
function chooseCategory(value: VersionCategory) {
  category.value = value;
  emit("update:section", "game");
}
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
  <section
    class="modal glass-surface version-download"
    data-drop-kind="modpack"
  >
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
    <nav class="catalog-filter" :aria-label="t('版本分类')">
      <button
        v-for="(item, index) in versionCategories"
        :key="item.id"
        type="button"
        class="list-reveal-item"
        :style="{ '--category-accent': item.color, ...listRevealStyle(index) }"
        :class="{
          active: (!section || section === 'game') && category === item.id,
        }"
        :aria-pressed="(!section || section === 'game') && category === item.id"
        :disabled="disabled"
        @click="chooseCategory(item.id)"
      >
        <VersionCube :variant="item.id" /><span>{{ t(item.label) }}</span>
      </button>
      <span class="catalog-divider" aria-hidden="true" />
      <button
        type="button"
        class="catalog-modpacks list-reveal-item"
        :style="{ '--category-accent': 'var(--blue)', ...listRevealStyle(4) }"
        :class="{ active: section === 'modpacks' }"
        :aria-pressed="section === 'modpacks'"
        :disabled="disabled"
        @click="$emit('update:section', 'modpacks')"
      >
        <Archive :size="20" /><span>{{ t("整合包") }}</span>
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
            :category="category"
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
            <button
              type="button"
              class="download-resources list-reveal-item"
              :style="listRevealStyle(3)"
              :disabled="
                disabled ||
                !selected ||
                loaderLoading ||
                (loader !== 'vanilla' && !loaderVersion)
              "
              @click="$emit('update:section', 'resources')"
            >
              <Package :size="17" /><span>{{ t("资源") }}</span
              ><small v-if="resources.length">{{ resources.length }}</small
              ><ChevronRight :size="15" />
            </button>
            <TransitionGroup
              name="resource-choice"
              tag="div"
              class="download-resource-choices"
            >
              <button
                v-for="item in resources"
                :key="`${item.platform}/${item.project}`"
                type="button"
                :disabled="disabled"
                :aria-label="`${t('移除')} ${item.name}`"
                @click="$emit('removeResource', item)"
              >
                <span>{{ item.name }}</span
                ><X :size="12" />
              </button>
            </TransitionGroup>
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
.catalog-filter {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr)) 1px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  padding: 8px 24px 10px;
  flex-shrink: 0;
}
.catalog-filter button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 44px;
  min-width: 0;
  padding: 8px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color 200ms var(--motion-ease),
    box-shadow 200ms var(--motion-ease),
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.catalog-filter button:hover:not(:disabled),
.catalog-filter button:focus-visible {
  color: var(--category-accent);
  background: color-mix(in srgb, var(--category-accent) 8%, transparent);
  outline: none;
}
.catalog-filter button.active {
  color: var(--category-accent);
  background: color-mix(in srgb, var(--category-accent) 18%, transparent);
  box-shadow:
    inset 0 0 0 1px color-mix(in srgb, var(--category-accent) 25%, transparent),
    0 0 16px color-mix(in srgb, var(--category-accent) 8%, transparent);
  text-shadow: 0 0 12px
    color-mix(in srgb, var(--category-accent) 24%, transparent);
}
.catalog-filter button:focus-visible {
  box-shadow: inset 0 0 0 1px var(--category-accent);
}
.catalog-divider {
  width: 1px;
  height: 22px;
  background: var(--line);
}
.download-resources {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  margin-top: 24px;
  padding: 12px 6px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
  text-align: left;
  transition:
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.download-resources > span {
  flex: 1;
}
.download-resources:hover:not(:disabled),
.download-resources:focus-visible {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.download-resource-choices {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.download-resource-choices button {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 5px 6px;
  border: 0;
  background: transparent;
  text-align: left;
  color: var(--muted);
  font-size: 11px;
}
.download-resource-choices span {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.download-resource-choices svg {
  flex-shrink: 0;
}
.download-resource-choices button:hover {
  color: var(--blue);
}
.resource-choice-enter-active,
.resource-choice-leave-active,
.resource-choice-move {
  transition:
    opacity 160ms var(--motion-ease),
    transform 220ms var(--motion-ease);
}
.resource-choice-enter-from,
.resource-choice-leave-to {
  opacity: 0;
  transform: translateY(6px);
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
  .catalog-filter {
    padding-inline: 20px;
    gap: 4px;
  }
  .catalog-filter button {
    flex-direction: column;
    gap: 4px;
    padding: 8px 4px;
  }
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
  .catalog-filter button,
  .download-resources,
  .loader-options-enter-active,
  .loader-options-leave-active {
    transition: none;
  }
}
</style>
