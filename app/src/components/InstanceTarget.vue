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
  context?: "download" | "manage";
  iconOnly?: boolean;
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
  if (props.disabled || (props.context === "manage" && id === null)) return;
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
    class="instance-target"
    :class="{ management: context === 'manage' }"
    @focusout="leave"
    @keydown.esc.stop.prevent="close(true)"
  >
    <button
      ref="trigger"
      type="button"
      class="instance-target-trigger"
      :disabled="disabled"
      :aria-label="
        t(
          context === 'manage'
            ? '管理 {name}，点击切换实例'
            : '安装到 {name}，点击切换实例',
          {
            name: target?.name || t('新建实例'),
          },
        )
      "
      aria-haspopup="menu"
      aria-controls="instance-target-menu"
      :aria-expanded="expanded"
      @click="expanded ? close() : open()"
      @keydown.down.prevent="open"
      @keydown.up.prevent="open"
    >
      <span
        v-if="context !== 'manage' && !iconOnly"
        class="instance-target-prefix"
        data-slide-key="instance-prefix"
        >{{ t("到") }}</span
      >
      <span
        v-if="!iconOnly"
        class="target-icon-stage"
        data-slide-key="instance-icon"
      >
        <Transition name="target-icon">
          <span
            :key="`${target?.id || 'new'}:${target?.icon}`"
            class="target-icon"
          >
            <InstanceIcon
              v-if="target"
              :icon="resolveInstanceIcon(target.icon, target.id)"
            />
            <Plus v-else :size="22" />
          </span>
        </Transition>
      </span>
      <span
        v-if="!iconOnly"
        class="target-name-stage"
        data-slide-key="instance-name"
      >
        <Transition name="target-name">
          <span :key="target?.name || 'new'" class="target-name">{{
            target?.name || t("新建实例")
          }}</span>
        </Transition>
      </span>
      <span class="target-chevron" data-slide-key="instance-chevron">
        <ChevronDown :size="13" :class="{ expanded }" />
      </span>
    </button>
    <Transition name="source-menu">
      <div
        v-if="expanded"
        id="instance-target-menu"
        ref="menu"
        class="instance-target-menu glass-surface"
        role="menu"
        :aria-label="
          t(context === 'manage' ? '选择管理的实例' : '选择安装实例')
        "
        @keydown="navigate"
      >
        <button
          v-for="item in instances"
          :key="item.id"
          type="button"
          role="menuitemradio"
          :aria-checked="target?.id === item.id"
          :class="{ selected: target?.id === item.id }"
          @pointerdown.prevent
          @click="choose(item.id)"
        >
          <InstanceIcon :icon="resolveInstanceIcon(item.icon, item.id)" />
          <span>{{ item.name }}</span>
        </button>
        <button
          v-if="context !== 'manage'"
          type="button"
          role="menuitemradio"
          :aria-checked="!target"
          :class="{ selected: !target }"
          @pointerdown.prevent
          @click="choose(null)"
        >
          <Plus :size="24" /><span>{{ t("新建实例") }}</span>
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.instance-target {
  position: relative;
  flex-shrink: 0;
  max-width: 42%;
}
.instance-target.management {
  min-width: 0;
  max-width: none;
  flex-shrink: 1;
}
.instance-target.management .instance-target-menu {
  left: 0;
  right: auto;
}
.instance-target-trigger {
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
.instance-target-trigger span {
  font-size: 12px;
}
.instance-target-trigger .instance-target-prefix {
  flex-shrink: 0;
  color: var(--muted);
}
.instance-target span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}
.instance-target :deep(.instance-art) {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
}
.instance-target svg {
  flex-shrink: 0;
}
.target-chevron svg {
  transition: transform 220ms var(--motion-ease);
}
.target-chevron svg.expanded {
  transform: rotate(180deg);
}
.instance-target button {
  transition:
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.instance-target button:hover:not(:disabled),
.instance-target button:focus-visible,
.instance-target .selected {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
  outline: none;
}
.instance-target button:focus-visible :deep(.instance-art) {
  filter: drop-shadow(0 0 5px var(--glow));
}
.instance-target-menu {
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
.instance-target-menu button {
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
.instance-target-menu button > svg {
  width: 28px;
}
.target-icon-stage {
  position: relative;
  width: 28px;
  height: 28px;
  flex: 0 0 28px;
}
.target-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}
.target-name-stage {
  position: relative;
  display: block;
  min-width: 0;
}
.target-name {
  display: block;
}
.target-chevron {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  overflow: visible;
}
.target-icon-enter-active,
.target-icon-leave-active,
.target-name-enter-active,
.target-name-leave-active {
  transition:
    opacity 180ms ease-out,
    transform 360ms cubic-bezier(0.16, 1, 0.3, 1);
}
.target-icon-enter-from,
.target-name-enter-from {
  opacity: 0;
  transform: translateX(6px);
}
.target-icon-leave-to,
.target-name-leave-to {
  opacity: 0;
  transform: translateX(-4px);
}
.target-icon-leave-active,
.target-name-leave-active {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .target-icon-enter-active,
  .target-icon-leave-active,
  .target-name-enter-active,
  .target-name-leave-active {
    transition: none;
  }
}
</style>
