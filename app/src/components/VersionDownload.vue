<script setup lang="ts">
import { computed } from "vue";
import { Download, LoaderCircle, X } from "lucide-vue-next";
import DownloadTarget from "./DownloadTarget.vue";
import VersionBrowser from "./VersionBrowser.vue";
import VersionCube from "./VersionCube.vue";
import InstanceLocation from "./InstanceLocation.vue";
import GlassSelect from "./GlassSelect.vue";
import { versionCategories, versionCategory } from "../versionCatalog";
import { t } from "../i18n";
import type { Instance, VersionManifest } from "../types";

const props = defineProps<{
  target: Instance | null;
  instances: readonly Instance[];
  manifest: VersionManifest | null;
  version: string;
  loader: string;
  fabricVersion: string;
  fabricOptions: readonly { value: string; label: string; detail?: string }[];
  fabricLoading: boolean;
  versionsLoading: boolean;
  disabled: boolean;
  submitting: boolean;
  canSubmit: boolean;
  name: string;
  location: string;
  directory: string;
  root: string;
  picking: boolean;
  running: boolean;
}>();
defineEmits<{
  "update:version": [value: string];
  "update:loader": [value: string];
  "update:fabricVersion": [value: string];
  "update:name": [value: string];
  "update:location": [value: string];
  target: [id: string | null];
  refresh: [];
  browse: [];
  submit: [];
  close: [];
}>();
const selected = computed(() =>
  props.manifest?.versions.find((item) => item.id === props.version),
);
const selectionDescription = computed(() => {
  if (!selected.value) return t("请选择版本");
  const loader =
    props.loader === "fabric"
      ? ["Fabric", props.fabricVersion].filter(Boolean).join(" ")
      : t("原版");
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
        {{ t("下载游戏版本") }}
      </h2>
      <DownloadTarget
        :target="target"
        :instances="instances"
        :disabled="disabled || picking"
        @select="$emit('target', $event)"
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
    <form class="version-download-form" @submit.prevent="$emit('submit')">
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
          <div class="download-loader toggle-row">
            <span id="download-fabric-label">Fabric</span>
            <div class="download-fabric-controls">
              <Transition name="fabric-choice" mode="out-in">
                <GlassSelect
                  v-if="loader === 'fabric' && fabricOptions.length"
                  id="download-fabric"
                  :model-value="fabricVersion"
                  :options="fabricOptions"
                  :disabled="disabled || fabricLoading"
                  floating
                  compact
                  literal-labels
                  @update:model-value="$emit('update:fabricVersion', $event)"
                />
                <span
                  v-else-if="loader === 'fabric' && fabricLoading"
                  class="download-fabric-loading"
                  role="status"
                  :aria-label="t('正在获取 Fabric 版本…')"
                >
                  <LoaderCircle :size="14" class="spin" />
                </span>
              </Transition>
            </div>
            <input
              type="checkbox"
              role="switch"
              aria-labelledby="download-fabric-label"
              :checked="loader === 'fabric'"
              :disabled="disabled || !selected"
              @change="
                $emit(
                  'update:loader',
                  ($event.target as HTMLInputElement).checked
                    ? 'fabric'
                    : 'vanilla',
                )
              "
            />
          </div>
          <Transition name="loader-options">
            <div v-if="!target" class="download-create">
              <label class="field">
                {{ t("实例名称") }}
                <input
                  :value="name"
                  :disabled="disabled"
                  required
                  maxlength="80"
                  @input="
                    $emit(
                      'update:name',
                      ($event.target as HTMLInputElement).value,
                    )
                  "
                />
              </label>
              <InstanceLocation
                :model-value="location"
                :directory="directory"
                :root="root"
                :disabled="disabled || picking"
                :picking="picking"
                @update:model-value="$emit('update:location', $event)"
                @browse="$emit('browse')"
              />
            </div>
          </Transition>
          <span v-if="running" class="download-detail" role="status">{{
            t("请先停止此实例的游戏")
          }}</span>
        </aside>
      </div>
      <footer class="modal-actions">
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
          class="button"
          :disabled="submitting"
          @click="$emit('close')"
        >
          {{ t("取消") }}
        </button>
        <button type="submit" class="button primary" :disabled="!canSubmit">
          <LoaderCircle v-if="submitting" :size="16" class="spin" /><Download
            v-else
            :size="16"
          />{{ t("下载并安装") }}
        </button>
      </footer>
    </form>
  </section>
</template>

<style scoped>
.version-download {
  width: 840px;
  height: min(590px, calc(100dvh - 64px));
}
.version-download .modal-header {
  margin-bottom: 0;
  gap: 16px;
}
.version-download .modal-header h2 {
  font-size: 16px;
}
.version-download .version-download-form {
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
.download-loader.toggle-row {
  margin-bottom: 0;
  min-height: 38px;
  gap: 10px;
  font-size: 13px;
}
.download-loader > span {
  flex-shrink: 0;
}
.download-fabric-controls {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
  min-height: 36px;
}
.download-fabric-controls .source-select {
  width: 100%;
  min-width: 0;
}
.download-fabric-loading {
  display: flex;
  justify-content: flex-end;
  width: 100%;
  color: var(--muted);
}
.fabric-choice-enter-active,
.fabric-choice-leave-active {
  transition:
    opacity 160ms var(--motion-ease),
    transform 180ms var(--motion-ease);
}
.fabric-choice-enter-from,
.fabric-choice-leave-to {
  opacity: 0;
  transform: translateX(4px);
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
  .fabric-choice-enter-active,
  .fabric-choice-leave-active {
    transition: none;
  }
  .loader-options-enter-active,
  .loader-options-leave-active {
    transition: none;
  }
}
</style>
