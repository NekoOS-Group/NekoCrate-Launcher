<script setup lang="ts">
import { t } from "../i18n";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
} from "vue";
import { backdropForTheme, backdropThemes } from "../backdropThemes";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ "update:modelValue": [theme: string] }>();
const current = computed(() => backdropForTheme(props.modelValue));
const alternatives = computed(() =>
  backdropThemes.filter((theme) => theme.id !== current.value.id),
);
const expanded = ref(false);
const root = ref<HTMLElement>();
const trigger = ref<HTMLButtonElement>();
const options = ref<HTMLElement>();
const optionsId = useId();
let closeTimer: ReturnType<typeof setTimeout> | undefined;
let pointerInside = false;
let triggerPointerType = "mouse";
let press: { id: number; theme: string; target: HTMLButtonElement } | null =
  null;
function keepOpen() {
  clearTimeout(closeTimer);
  expanded.value = true;
}
function toggle(event: MouseEvent) {
  // Mouse hover already opened this target; clicking must not undo that.
  if (event.detail > 0 && triggerPointerType !== "touch") keepOpen();
  else if (expanded.value) close();
  else keepOpen();
}

function close(restoreFocus = false) {
  clearTimeout(closeTimer);
  cancelPress();
  expanded.value = false;
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}
function choose(theme: string) {
  emit("update:modelValue", theme);
  close(true);
}
function leave(event: FocusEvent) {
  // Safari may blur a focused button to body on pointerdown without focusing
  // the new button. That must not hide the choices before pointerup/click.
  if (press || pointerInside) return;
  if (
    !(event.relatedTarget instanceof Node) ||
    !root.value?.contains(event.relatedTarget)
  )
    close();
}
function leavePointer() {
  pointerInside = false;
  if (press) return;
  const focused = document.activeElement;
  if (!root.value?.contains(focused) || !focused?.matches(":focus-visible"))
    closeTimer = setTimeout(() => close(), 260);
}
function enterPointer(event: PointerEvent) {
  pointerInside = true;
  if (event.pointerType !== "touch") keepOpen();
}
function startPress(event: PointerEvent, theme: string) {
  if (!event.isPrimary || event.button !== 0) return;
  event.preventDefault();
  keepOpen();
  const target = event.currentTarget as HTMLButtonElement;
  press = { id: event.pointerId, theme, target };
  target.setPointerCapture(event.pointerId);
}
function cancelPress() {
  const previous = press;
  press = null;
  if (previous?.target.hasPointerCapture(previous.id))
    previous.target.releasePointerCapture(previous.id);
}
function releasePress(event: PointerEvent) {
  if (!press || press.id !== event.pointerId) return;
  const { target, theme } = press;
  const box = target.getBoundingClientRect();
  cancelPress();
  if (
    event.clientX >= box.left &&
    event.clientX <= box.right &&
    event.clientY >= box.top &&
    event.clientY <= box.bottom
  )
    choose(theme);
  else leavePointer();
}
function outside(event: PointerEvent) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
    close();
}
async function navigate(event: KeyboardEvent) {
  if (
    ![
      "ArrowRight",
      "ArrowLeft",
      "ArrowDown",
      "ArrowUp",
      "Home",
      "End",
    ].includes(event.key)
  )
    return;
  event.preventDefault();
  expanded.value = true;
  await nextTick();
  const buttons = Array.from(
    options.value?.querySelectorAll<HTMLButtonElement>("button") || [],
  );
  if (!buttons.length) return;
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  const backwards = event.key === "ArrowLeft" || event.key === "ArrowUp";
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? buttons.length - 1
        : index < 0
          ? backwards
            ? buttons.length - 1
            : 0
          : (index + (backwards ? -1 : 1) + buttons.length) % buttons.length;
  buttons[next]?.focus({ preventScroll: true });
}
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() => {
  clearTimeout(closeTimer);
  cancelPress();
  document.removeEventListener("pointerdown", outside, true);
});
</script>

<template>
  <div
    ref="root"
    class="theme-picker"
    :class="{ expanded }"
    role="group"
    :aria-label="t('主题配色')"
    @pointerenter="enterPointer"
    @pointerleave="leavePointer"
    @focusout="leave"
    @keydown="navigate"
    @keydown.esc.stop.prevent="close(true)"
  >
    <button
      ref="trigger"
      class="theme-swatch active"
      :style="{ '--swatch': current.color }"
      :aria-label="t('{name}主题，展开配色选项', { name: t(current.name) })"
      :title="`${t(current.name)} · ${t(current.pattern)}`"
      :aria-expanded="expanded"
      :aria-controls="optionsId"
      @pointerdown="triggerPointerType = $event.pointerType"
      @click="toggle"
    >
      <svg
        viewBox="0 0 12 14"
        width="14"
        height="16"
        fill="none"
        stroke="currentColor"
        aria-hidden="true"
      >
        <path :d="current.mark" />
      </svg>
    </button>
    <div
      :id="optionsId"
      ref="options"
      class="theme-options"
      :inert="!expanded"
      :aria-hidden="!expanded"
    >
      <button
        v-for="color in alternatives"
        :key="color.id"
        class="theme-swatch"
        :style="{ '--swatch': color.color }"
        :aria-label="
          t('{name}主题，{pattern}背景', {
            name: t(color.name),
            pattern: t(color.pattern),
          })
        "
        :title="`${t(color.name)} · ${t(color.pattern)}`"
        @pointerdown="startPress($event, color.id)"
        @pointerup="releasePress"
        @pointercancel="cancelPress"
        @lostpointercapture="cancelPress"
        @click="if ($event.detail === 0) choose(color.id);"
      >
        <svg
          viewBox="0 0 12 14"
          width="14"
          height="16"
          fill="none"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path :d="color.mark" />
        </svg>
      </button>
    </div>
  </div>
</template>
