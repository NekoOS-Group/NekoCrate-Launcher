<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight, Code2 } from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import { t } from "../i18n";
import type {
  InstanceLaunchOptions,
  LaunchDirectory,
  IsolationMode,
} from "../instanceLaunch";
const props = defineProps<{
  modelValue: InstanceLaunchOptions;
  disabled?: boolean;
  directory?: LaunchDirectory | null;
  isolationDisabled?: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: InstanceLaunchOptions];
}>();
function update<K extends keyof InstanceLaunchOptions>(
  key: K,
  value: InstanceLaunchOptions[K],
) {
  emit("update:modelValue", { ...props.modelValue, [key]: value });
}
const expanded = ref(false);
const mode = computed({
  get: () => props.modelValue.window_mode,
  set: (value) => update("window_mode", value),
});
const isolation = computed({
  get: (): IsolationMode =>
    props.modelValue.isolation ?? props.directory?.mode ?? "shared",
  set: (value: IsolationMode) => update("isolation", value),
});
const logs = computed({
  get: () => props.modelValue.log_behavior,
  set: (value) => update("log_behavior", value),
});
const parameterTab = ref<"jvm_args" | "game_args" | "environment">("jvm_args");
const parameterTabs = [
  { id: "jvm_args", label: "JVM" },
  { id: "game_args", label: "游戏参数" },
  { id: "environment", label: "环境变量" },
] as const;
function moveParameterTab(event: KeyboardEvent, offset: number) {
  const index =
    (parameterTabs.findIndex((tab) => tab.id === parameterTab.value) +
      offset +
      3) %
    3;
  parameterTab.value = parameterTabs[index]!.id;
  const parent = (event.currentTarget as HTMLElement).parentElement;
  (parent?.children[index] as HTMLElement)?.focus();
}
const parameters = computed({
  get: () => props.modelValue[parameterTab.value].join("\n"),
  set: (value: string) => update(parameterTab.value, value.split("\n")),
});
</script>
<template>
  <div class="instance-launch-options">
    <div class="instance-policy-row">
      <div class="instance-window-choice">
        <label id="instance-isolation-label" for="instance-isolation-trigger">{{
          t("版本隔离")
        }}</label>
        <GlassSelect
          id="instance-isolation"
          v-model="isolation"
          :disabled="disabled || isolationDisabled || !directory"
          :options="[
            { value: 'version', label: '各版本独立' },
            { value: 'shared', label: '共享目录' },
          ]"
          floating
          compact
        />
      </div>
      <div class="instance-window-choice">
        <label id="instance-logs-label" for="instance-logs-trigger">{{
          t("日志行为")
        }}</label>
        <GlassSelect
          id="instance-logs"
          v-model="logs"
          :disabled="disabled"
          :options="[
            { value: 'crash', label: '崩溃时打开' },
            { value: 'startup', label: '启动时打开' },
            { value: 'manual', label: '手动查看' },
          ]"
          floating
          compact
        />
      </div>
    </div>
    <p v-if="directory?.changed" class="isolation-note">
      {{ t("原文件保留在原目录，不会自动迁移。") }}
    </p>
    <div class="instance-window-row">
      <div class="instance-window-choice">
        <label id="instance-window-label" for="instance-window-trigger">{{
          t("游戏窗口")
        }}</label>
        <GlassSelect
          id="instance-window"
          v-model="mode"
          :disabled="disabled"
          :options="[
            { value: 'default', label: '跟随游戏' },
            { value: 'windowed', label: '窗口' },
            { value: 'fullscreen', label: '全屏' },
          ]"
          floating
          compact
        />
      </div>
      <Transition name="instance-resolution">
        <div v-if="mode === 'windowed'" class="instance-resolution">
          <input
            :value="modelValue.width"
            type="number"
            min="320"
            max="7680"
            :disabled="disabled"
            :aria-label="t('窗口宽度')"
            @input="
              update('width', Number(($event.target as HTMLInputElement).value))
            "
          />
          <span>×</span>
          <input
            :value="modelValue.height"
            type="number"
            min="240"
            max="4320"
            :disabled="disabled"
            :aria-label="t('窗口高度')"
            @input="
              update(
                'height',
                Number(($event.target as HTMLInputElement).value),
              )
            "
          />
          <small>px</small>
        </div>
      </Transition>
      <div class="toggle-row instance-launch-toggle">
        <span>{{ t("启动后最小化") }}</span>
        <input
          type="checkbox"
          role="switch"
          :checked="modelValue.minimize_launcher"
          :aria-label="t('游戏启动后最小化')"
          :disabled="disabled || logs === 'startup'"
          :title="logs === 'startup' ? t('启动时优先显示日志') : undefined"
          @change="update('minimize_launcher', !modelValue.minimize_launcher)"
        />
      </div>
    </div>
    <div class="instance-jvm-options" :class="{ expanded }">
      <button
        type="button"
        class="jvm-heading"
        :aria-expanded="expanded"
        aria-controls="instance-jvm-editor"
        @click="expanded = !expanded"
      >
        <Code2 :size="14" />{{ t("启动参数") }}<ChevronRight :size="13" />
      </button>
      <div id="instance-jvm-editor" class="jvm-editor" :inert="!expanded">
        <div>
          <div class="parameter-editor">
            <div
              class="parameter-tabs"
              role="tablist"
              :aria-label="t('启动参数')"
            >
              <button
                v-for="tab in parameterTabs"
                :id="`parameter-tab-${tab.id}`"
                :key="tab.id"
                type="button"
                role="tab"
                :aria-selected="parameterTab === tab.id"
                aria-controls="parameter-content"
                :tabindex="parameterTab === tab.id ? 0 : -1"
                :class="{ active: parameterTab === tab.id }"
                @click="parameterTab = tab.id"
                @keydown.right.prevent="moveParameterTab($event, 1)"
                @keydown.left.prevent="moveParameterTab($event, -1)"
              >
                {{ t(tab.label) }}
              </button>
            </div>
            <div
              id="parameter-content"
              role="tabpanel"
              :aria-labelledby="`parameter-tab-${parameterTab}`"
            >
              <textarea
                v-model="parameters"
                :disabled="disabled"
                :aria-label="
                  t(parameterTabs.find((tab) => tab.id === parameterTab)!.label)
                "
                :placeholder="
                  parameterTab === 'environment'
                    ? 'NAME=value'
                    : t('每行一个参数')
                "
                spellcheck="false"
                rows="2"
                maxlength="16384"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
<style scoped>
.instance-launch-options {
  grid-column: 1 / -1;
  min-width: 0;
}
.instance-policy-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  font-size: 11px;
}
.instance-policy-row .instance-window-choice :deep(.source-select) {
  width: 144px;
}
.isolation-note {
  color: var(--muted);
  font-size: 10px;
  line-height: 14px;
  margin: 6px 0 0;
}
.instance-window-row {
  margin-top: 12px;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  white-space: nowrap;
}
.instance-window-choice {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex-shrink: 0;
}
.instance-window-choice :deep(.source-select) {
  width: 110px;
  flex-shrink: 0;
}
.instance-window-choice :deep(.source-select-trigger) {
  height: 32px;
  min-height: 32px;
  font-size: 11px;
  padding-inline: 9px;
  gap: 4px;
}
.instance-launch-toggle {
  min-height: 22px;
  margin: 0 0 0 auto;
  gap: 8px;
}
.instance-launch-toggle input {
  width: 30px;
  height: 18px;
}
.instance-launch-toggle input::after {
  width: 12px;
  height: 12px;
}
.instance-launch-toggle input:checked::after {
  transform: translateX(12px);
}
.instance-resolution {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 148px;
  flex-shrink: 0;
  overflow: hidden;
}
.instance-resolution input {
  width: 56px;
  height: 32px;
  min-height: 32px;
  flex-shrink: 0;
  min-width: 0;
  padding: 5px 2px;
  text-align: center;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  appearance: textfield;
}
.instance-resolution input::-webkit-inner-spin-button,
.instance-resolution input::-webkit-outer-spin-button {
  appearance: none;
  margin: 0;
}
.instance-resolution span,
.instance-resolution small {
  opacity: 0.5;
  font-size: 10px;
}
.instance-resolution-enter-active,
.instance-resolution-leave-active {
  transition:
    width 180ms cubic-bezier(0.2, 0.8, 0.2, 1),
    opacity 140ms ease;
}
.instance-resolution-enter-from,
.instance-resolution-leave-to {
  width: 0;
  opacity: 0;
}
.instance-jvm-options {
  margin-top: 10px;
}
.jvm-heading {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  width: 100%;
  padding: 0;
  background: none;
  border: 0;
  color: var(--muted);
  font-size: 12px;
}
.jvm-heading svg:last-child {
  margin-left: auto;
  transition: transform 180ms ease;
}
.expanded > .jvm-heading svg:last-child {
  transform: rotate(90deg);
}
.jvm-editor {
  display: grid;
  grid-template-rows: 0fr;
  opacity: 0;
  transition:
    grid-template-rows 180ms cubic-bezier(0.2, 0.8, 0.2, 1),
    opacity 140ms ease;
}
.jvm-editor > div {
  overflow: hidden;
  min-height: 0;
}
.expanded > .jvm-editor {
  grid-template-rows: 1fr;
  opacity: 1;
}
.parameter-editor {
  margin-top: 8px;
  border: 1px solid var(--glass-stroke);
  background: color-mix(in srgb, var(--night) 48%, transparent);
}
.parameter-editor:focus-within {
  border-color: var(--blue);
}
.parameter-tabs {
  display: flex;
  gap: 14px;
  padding: 7px 10px 0;
}
.parameter-tabs button {
  padding: 0;
  border: 0;
  background: none;
  color: var(--muted);
  font: 11px var(--sans);
  cursor: pointer;
  transition:
    color 150ms,
    text-shadow 150ms;
}
.parameter-tabs button.active {
  color: var(--ink);
  text-shadow: 0 0 10px var(--blue);
}
.instance-jvm-options textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  padding: 7px 10px;
  resize: none;
  height: 56px;
  border: 0;
  border-radius: 0;
  color: var(--ink);
  background: none;
  font: 12px/1.7 var(--mono);
}
.instance-jvm-options textarea:focus {
  outline: none;
}
.instance-jvm-options textarea::placeholder {
  color: var(--muted);
}
@media (prefers-reduced-motion: reduce) {
  .jvm-editor,
  .instance-resolution-enter-active,
  .instance-resolution-leave-active {
    transition: none;
  }
}
</style>
