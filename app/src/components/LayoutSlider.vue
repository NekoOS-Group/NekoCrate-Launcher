<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId } from "vue";

const props = defineProps<{
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit: string;
}>();
const emit = defineEmits<{ change: [value: number] }>();
const id = useId();
const anchor = ref<HTMLButtonElement>();
const panel = ref<HTMLElement>();
const open = ref(false);
const side = ref("bottom");
const position = ref({ left: "0px", top: "0px" });
let timer: ReturnType<typeof setTimeout> | undefined;
let dragging = false;
let drag: { id: number; y: number; value: number; height: number } | undefined;
let observer: ResizeObserver | undefined;
function keep() {
  clearTimeout(timer);
}
function place() {
  if (!anchor.value) return;
  const box = anchor.value.getBoundingClientRect();
  const bar =
    anchor.value.closest(".layout-label")?.getBoundingClientRect() ?? box;
  side.value = bar.bottom + 100 <= window.innerHeight ? "bottom" : "top";
  position.value = {
    left: `${Math.round(Math.max(8, Math.min(window.innerWidth - 42, box.left + box.width / 2 - 17)))}px`,
    top: `${Math.round(Math.max(8, side.value === "bottom" ? bar.bottom : bar.top - 100))}px`,
  };
}
function show() {
  keep();
  if (open.value) return;
  document.dispatchEvent(
    new CustomEvent("nekocrate-layout-slider", { detail: id }),
  );
  place();
  open.value = true;
}
function anotherSlider(event: Event) {
  if ((event as CustomEvent).detail !== id) hide();
}
function hide() {
  keep();
  dragging = false;
  drag = undefined;
  open.value = false;
}
function leave() {
  keep();
  timer = setTimeout(() => {
    if (!dragging) hide();
  }, 150);
}
function blur(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    (!panel.value?.contains(event.relatedTarget) &&
      event.relatedTarget !== anchor.value)
  )
    leave();
}
function outside(event: PointerEvent) {
  if (
    event.target instanceof Node &&
    !panel.value?.contains(event.target) &&
    !anchor.value?.contains(event.target)
  )
    hide();
}
function release() {
  dragging = false;
  drag = undefined;
  if (!panel.value?.matches(":hover") && !anchor.value?.matches(":hover"))
    leave();
}
function updateValue(value: number) {
  const step = props.step ?? 1;
  emit(
    "change",
    Math.max(
      props.min,
      Math.min(
        props.max,
        props.min + Math.round((value - props.min) / step) * step,
      ),
    ),
  );
}
function startDrag(event: PointerEvent) {
  if (event.button !== 0 || !(event.currentTarget instanceof HTMLInputElement))
    return;
  const input = event.currentTarget;
  input.focus({ preventScroll: true });
  input.setPointerCapture(event.pointerId);
  const box = input.getBoundingClientRect();
  const height = Math.max(1, box.height - 16);
  const value =
    props.min +
    Math.max(0, Math.min(1, (box.bottom - 8 - event.clientY) / height)) *
      (props.max - props.min);
  drag = { id: event.pointerId, y: event.clientY, value, height };
  dragging = true;
  updateValue(value);
}
function moveDrag(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.id) return;
  // Font size can move the toolbar. Measure pointer travel from the drag origin,
  // rather than feeding the moving slider's position back into its own value.
  updateValue(
    drag.value +
      ((drag.y - event.clientY) / drag.height) * (props.max - props.min),
  );
}
function escape(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  // Return keyboard focus before closing; its focus handler may reopen the control.
  anchor.value?.focus({ preventScroll: true });
  void nextTick(hide);
}
onMounted(() => {
  observer = new ResizeObserver(() => {
    if (open.value) place();
  });
  const item = anchor.value?.closest(".home-layout-item");
  if (item) observer.observe(item);
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("nekocrate-layout-slider", anotherSlider);
  document.addEventListener("pointerup", release);
  document.addEventListener("pointercancel", release);
  window.addEventListener("blur", hide);
  window.addEventListener("resize", hide);
  window.addEventListener("nekocrate-home-layout", placeIfOpen);
});
function placeIfOpen() {
  if (open.value) place();
}
onBeforeUnmount(() => {
  keep();
  observer?.disconnect();
  document.removeEventListener("pointerdown", outside, true);
  document.removeEventListener("nekocrate-layout-slider", anotherSlider);
  document.removeEventListener("pointerup", release);
  document.removeEventListener("pointercancel", release);
  window.removeEventListener("blur", hide);
  window.removeEventListener("resize", hide);
  window.removeEventListener("nekocrate-home-layout", placeIfOpen);
});
</script>

<template>
  <button
    ref="anchor"
    type="button"
    class="layout-orientation layout-slider-trigger"
    :aria-label="label"
    :aria-expanded="open"
    :aria-controls="id"
    @pointerenter="show"
    @pointerleave="leave"
    @pointerdown.stop
    @click.stop="show"
    @focus="show"
    @blur="blur"
    @keydown.stop="escape"
  >
    <slot />
  </button>
  <Teleport to=".shell">
    <Transition name="layout-slider">
      <div
        v-if="open"
        :id="id"
        ref="panel"
        class="layout-slider-panel glass-surface"
        :data-side="side"
        :style="position"
        @pointerenter="keep"
        @pointerleave="leave"
        @pointerdown.stop="dragging = true"
        @click.stop
        @keydown.stop="escape"
        @focusout="blur"
      >
        <input
          type="range"
          :min="min"
          :max="max"
          :step="step ?? 1"
          :value="value"
          :aria-label="label"
          aria-orientation="vertical"
          :aria-valuetext="`${value}${unit}`"
          @pointerdown.stop.prevent="startDrag"
          @pointermove="moveDrag"
          @input="
            emit('change', Number(($event.target as HTMLInputElement).value))
          "
        />
        <output>{{ value }}{{ unit }}</output>
      </div>
    </Transition>
  </Teleport>
</template>

<style>
.layout-slider-panel.glass-surface {
  position: fixed;
  z-index: 155;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  width: 34px;
  height: 100px;
  padding: 8px 3px 5px;
  border-radius: 0 0 5px 5px;
  border-top: 0;
  --glass-fill: color-mix(in srgb, var(--surface) 92%, transparent);
  color: var(--blue);
  user-select: none;
  -webkit-user-select: none;
}
.layout-slider-panel[data-side="top"] {
  border-radius: 5px 5px 0 0;
  border-top: 1px solid var(--glass-stroke);
  border-bottom: 0;
}
.layout-slider-panel input[type="range"] {
  writing-mode: vertical-lr;
  direction: rtl;
  width: 20px;
  height: 66px;
  min-height: 0;
  margin: 0;
  padding: 0;
  border: 0;
  background: transparent;
  box-shadow: none;
  accent-color: var(--blue);
  cursor: ns-resize;
  touch-action: none;
}
.layout-slider-panel input:focus-visible {
  outline: none;
  filter: drop-shadow(0 0 4px var(--glow));
}
.layout-slider-panel output {
  font-size: 10px;
  line-height: 16px;
  font-variant-numeric: tabular-nums;
}
.layout-slider-enter-active,
.layout-slider-leave-active {
  /* Growing away from the bar never paints over the trigger icons or moves
     their hover target while WebKit creates/removes the popover layer. */
  transform-origin: top center;
  transition:
    opacity 120ms,
    transform 160ms var(--motion-ease);
}
.layout-slider-panel[data-side="top"] {
  transform-origin: bottom center;
}
.layout-slider-enter-from,
.layout-slider-leave-to {
  opacity: 0;
  transform: scaleY(0.94);
}
@media (prefers-reduced-motion: reduce) {
  .layout-slider-enter-active,
  .layout-slider-leave-active {
    transition: none;
  }
}
</style>
