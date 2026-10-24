<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ChevronDown, Plus } from "lucide-vue-next";
import InstanceIcon from "./InstanceIcon.vue";
import { resolveInstanceIcon } from "../instanceIcons";
import { t } from "../i18n";
import type { Instance } from "../types";

const props = defineProps<{
  target: Instance | null;
  instances: readonly Instance[];
  disabled: boolean;
}>();
const emit = defineEmits<{ select: [id: string | null] }>();
const root = ref<HTMLElement>();
const trigger = ref<HTMLButtonElement>();
const menu = ref<HTMLElement>();
const expanded = ref(false);
function buttons() {
  return Array.from(
    menu.value?.querySelectorAll<HTMLButtonElement>("button") || [],
  );
}
function focus(index: number) {
  const options = buttons();
  const option = options[(index + options.length) % options.length];
  option?.focus({ preventScroll: true });
  option?.scrollIntoView({ block: "nearest" });
}
function close(restore = false) {
  expanded.value = false;
  if (restore) trigger.value?.focus({ preventScroll: true });
}
async function open() {
  if (props.disabled) return;
  expanded.value = true;
  await nextTick();
  if (!expanded.value) return;
  focus(
    props.target
      ? Math.max(
          0,
          props.instances.findIndex((item) => item.id === props.target?.id),
        )
      : props.instances.length,
  );
}
function navigate(event: KeyboardEvent) {
  const index = buttons().indexOf(document.activeElement as HTMLButtonElement);
  const next =
    event.key === "ArrowDown"
      ? index + 1
      : event.key === "ArrowUp"
        ? index - 1
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? buttons().length - 1
            : null;
  if (next === null) return;
  event.preventDefault();
  focus(next);
}
function choose(id: string | null) {
  if (props.disabled) return;
  close(true);
  emit("select", id);
}
function outside(event: PointerEvent) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
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
  () => props.disabled,
  (disabled) => {
    if (disabled) close();
  },
);
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", outside, true),
);
</script>

<template>
  <div
    ref="root"
    class="download-target"
    @focusout="leave"
    @keydown.esc.stop.prevent="close(true)"
  >
    <button
      ref="trigger"
      type="button"
      class="download-target-trigger"
      :disabled="disabled"
      :aria-label="
        t('安装到 {name}，点击切换实例', {
          name: target?.name || t('新建实例'),
        })
      "
      aria-haspopup="menu"
      aria-controls="download-target-menu"
      :aria-expanded="expanded"
      @click="expanded ? close() : open()"
      @keydown.down.prevent="open"
      @keydown.up.prevent="open"
    >
      <span class="download-target-prefix">{{ t("到") }}</span>
      <InstanceIcon
        v-if="target"
        :icon="resolveInstanceIcon(target.icon, target.id)"
      />
      <Plus v-else :size="22" />
      <span>{{ target?.name || t("新建实例") }}</span>
      <ChevronDown :size="13" :class="{ expanded }" />
    </button>
    <Transition name="source-menu">
      <div
        v-if="expanded"
        id="download-target-menu"
        ref="menu"
        class="download-target-menu glass-surface"
        role="menu"
        :aria-label="t('选择安装实例')"
        @keydown="navigate"
      >
        <button
          v-for="item in instances"
          :key="item.id"
          type="button"
          role="menuitemradio"
          :aria-checked="target?.id === item.id"
          :class="{ selected: target?.id === item.id }"
          @click="choose(item.id)"
        >
          <InstanceIcon :icon="resolveInstanceIcon(item.icon, item.id)" />
          <span>{{ item.name }}</span>
        </button>
        <button
          type="button"
          role="menuitemradio"
          :aria-checked="!target"
          :class="{ selected: !target }"
          @click="choose(null)"
        >
          <Plus :size="24" /><span>{{ t("新建实例") }}</span>
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.download-target {
  position: relative;
  flex-shrink: 0;
  max-width: 42%;
}
.download-target-trigger {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 36px;
  padding: 2px 6px;
  border: 0;
  background: transparent;
  color: var(--muted);
}
.download-target-trigger span {
  font-size: 12px;
}
.download-target-trigger .download-target-prefix {
  flex-shrink: 0;
  color: var(--muted);
}
.download-target span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}
.download-target :deep(.instance-art) {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
}
.download-target svg {
  flex-shrink: 0;
}
.download-target-trigger > svg:last-child {
  transition: transform 220ms var(--motion-ease);
}
.download-target-trigger > svg.expanded {
  transform: rotate(180deg);
}
.download-target button {
  transition:
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.download-target button:hover:not(:disabled),
.download-target button:focus-visible,
.download-target .selected {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
  outline: none;
}
.download-target button:focus-visible :deep(.instance-art) {
  filter: drop-shadow(0 0 5px var(--glow));
}
.download-target-menu {
  --glass-fill: color-mix(in srgb, var(--surface) 96%, transparent);
  position: absolute;
  z-index: 12;
  top: calc(100% + 10px);
  right: 0;
  width: 240px;
  max-height: min(300px, 55dvh);
  overflow-y: auto;
  padding: 8px;
  box-shadow:
    var(--glass-rim),
    0 10px 32px #0005;
}
.download-target-menu button {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 44px;
  padding: 8px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 13px;
}
.download-target-menu button > svg {
  width: 28px;
}
</style>
