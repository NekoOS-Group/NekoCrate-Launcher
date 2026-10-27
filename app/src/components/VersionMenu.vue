<script setup lang="ts">
import { t } from "../i18n";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { ChevronUp, Plus, SlidersHorizontal } from "lucide-vue-next";
import type { Instance, Task } from "../types";
import VersionStatusIcon from "./VersionStatusIcon.vue";
import { versionDownloadState } from "../versionProgress";
import { versionDescription, versionName } from "../versionDescription";

const props = defineProps<{
  instance?: Instance;
  disabled: boolean;
  locked: boolean;
  tasks: readonly Task[];
}>();
const versions = computed(
  () =>
    props.instance?.versions.map((version) => ({
      ...version,
      selected: version.id === props.instance?.selected_version,
      download: versionDownloadState(props.instance!.id, version, props.tasks),
    })) ?? [],
);
const emit = defineEmits<{
  select: [version: string];
  download: [];
  manage: [];
}>();
const selectedName = computed(() => {
  const version = props.instance?.versions.find(
    (v) => v.id === props.instance?.selected_version,
  );
  return version ? versionName(version) : props.instance?.selected_version;
});
const root = ref<HTMLElement>();
const trigger = ref<HTMLButtonElement>();
const menu = ref<HTMLElement>();
const expanded = ref(false);

function items() {
  return Array.from(
    menu.value?.querySelectorAll<HTMLButtonElement>("[data-menu-item]") || [],
  );
}
function focusItem(index: number) {
  const buttons = items();
  const button = buttons[(index + buttons.length) % buttons.length];
  button?.focus({ preventScroll: true });
  button?.scrollIntoView({ block: "nearest" });
}
function close(restoreFocus = false) {
  expanded.value = false;
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}
async function open(position: "selected" | "first" | "last" = "selected") {
  if (props.disabled) return;
  expanded.value = true;
  await nextTick();
  if (!expanded.value) return;
  const selected =
    props.instance?.versions.findIndex(
      (v) => v.id === props.instance?.selected_version,
    ) ?? -1;
  focusItem(
    position === "last"
      ? items().length - 1
      : position === "first"
        ? 0
        : Math.max(0, selected),
  );
}
function navigate(event: KeyboardEvent) {
  const index = items().indexOf(document.activeElement as HTMLButtonElement);
  let next: number;
  switch (event.key) {
    case "ArrowDown":
      next = index + 1;
      break;
    case "ArrowUp":
      next = index - 1;
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = items().length - 1;
      break;
    default:
      return;
  }
  event.preventDefault();
  focusItem(next);
}
function choose(version: string) {
  if (props.locked) return;
  close(true);
  emit("select", version);
}
function download() {
  if (props.locked) return;
  close(true);
  emit("download");
}
function outside(event: PointerEvent) {
  if (
    expanded.value &&
    event.target instanceof Node &&
    !root.value?.contains(event.target)
  )
    close();
}
function leave(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !root.value?.contains(event.relatedTarget)
  )
    close();
}
watch(
  () => props.instance?.id,
  () => close(),
);
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) close();
  },
);
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", outside, true),
);
defineExpose({
  revealForDrop() {
    if (!expanded.value && !props.locked) void open();
  },
});
</script>

<template>
  <div
    ref="root"
    class="version-menu-root"
    @focusout="leave"
    @keydown.esc.stop.prevent="close(true)"
  >
    <button
      ref="trigger"
      class="version-trigger"
      data-resource-menu-trigger
      :disabled="disabled"
      :aria-label="t('选择游戏版本')"
      aria-haspopup="menu"
      aria-controls="launch-version-menu"
      :aria-expanded="expanded"
      :title="
        instance?.selected_version
          ? t('当前 {version} · 选择版本', {
              version: selectedName || instance.selected_version,
            })
          : t('选择或下载版本')
      "
      @click="expanded ? close() : open()"
      @keydown.down.prevent="open('first')"
      @keydown.up.prevent="open('last')"
    >
      <ChevronUp :size="16" />
    </button>
    <Transition name="version-menu">
      <div
        v-if="expanded"
        id="launch-version-menu"
        ref="menu"
        class="version-menu glass-surface"
        role="menu"
        :aria-label="t('游戏版本')"
        @keydown="navigate"
      >
        <div class="version-menu-list" role="none">
          <p v-if="!instance?.versions.length" class="version-menu-empty">
            {{ t("暂无游戏版本") }}
          </p>
          <button
            v-for="version in versions"
            :key="version.id"
            class="version-menu-item"
            :data-resource-instance="instance?.id"
            :data-resource-version="version.id"
            role="menuitemradio"
            :aria-checked="version.selected"
            :aria-disabled="locked"
            :title="
              version.installed
                ? versionName(version)
                : `${versionName(version)} · ${t('已下载约 {progress}%', { progress: Math.round(version.download.progress) })}`
            "
            tabindex="-1"
            data-menu-item
            @click="choose(version.id)"
          >
            <VersionStatusIcon
              :selected="version.selected"
              :installed="version.installed"
              :progress="version.download.progress"
              :indeterminate="version.download.indeterminate"
            />
            <span class="version-menu-label">
              <span class="version-menu-name">{{ versionName(version) }}</span>
              <small class="version-menu-detail">{{
                versionDescription(version)
              }}</small>
            </span>
            <small v-if="!version.installed" class="version-menu-state">{{
              t("待下载")
            }}</small>
          </button>
        </div>
        <div
          v-if="instance?.versions.length"
          class="version-menu-divider"
          role="separator"
        />
        <div class="version-menu-footer" role="none">
          <button
            class="version-menu-item version-menu-download"
            role="menuitem"
            :aria-disabled="locked"
            tabindex="-1"
            data-menu-item
            @click="download"
          >
            <Plus :size="16" /><span> {{ t("添加新的版本") }} </span>
          </button>
          <button
            class="version-menu-item version-menu-manage"
            role="menuitem"
            tabindex="-1"
            data-menu-item
            :aria-label="t('管理版本')"
            :title="t('管理版本')"
            @click="
              close(true);
              emit('manage');
            "
          >
            <SlidersHorizontal :size="16" />
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>
