<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { activeDialog, registerDialog } from "../dialogSurface";
import { forgetDialogFocus, rememberDialogFocus } from "../dialogMotion";
import { dropInteraction } from "../dropSurface";
import { bubblePosition } from "../bubblePosition";

const props = defineProps<{
  label: string;
  anchor?: HTMLElement;
  placement?: "above" | "below";
  locked?: boolean;
}>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
const frame = ref<HTMLElement>();
const position = ref<ReturnType<typeof bubblePosition>>();
let observer: ResizeObserver | undefined;
let unregister: (() => void) | undefined;
let disposed = false;

function fit() {
  if (!frame.value || dialog.value?.dataset.closing) return;
  // offset dimensions are unaffected by the entrance animation's transform.
  const anchor = props.anchor?.isConnected
    ? props.anchor.getBoundingClientRect()
    : undefined;
  position.value = bubblePosition(
    anchor,
    {
      width: frame.value.offsetWidth,
      height: frame.value.offsetHeight,
    },
    { width: window.innerWidth, height: window.innerHeight },
    props.placement || "below",
  );
}
function close(restoreFocus = true) {
  if (
    !props.locked &&
    !dropInteraction.value &&
    !dialog.value?.dataset.closing
  ) {
    // Commit lazy text fields before outside dismissal removes their inputs.
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && dialog.value?.contains(focused))
      focused.blur();
    if (!restoreFocus && dialog.value) forgetDialogFocus(dialog.value);
    emit("close");
  }
}
function outside(event: PointerEvent) {
  if (
    event.target instanceof Node &&
    !dialog.value?.contains(event.target) &&
    !props.anchor?.contains(event.target)
  )
    close(false);
}
function keyboard(event: KeyboardEvent) {
  // Nested selectors consume Escape first. A later modal owns its own Escape.
  if (
    event.key === "Escape" &&
    !event.defaultPrevented &&
    activeDialog.value === dialog.value
  ) {
    event.preventDefault();
    close();
  }
}
watch(
  () => props.anchor,
  (anchor, previous) => {
    if (previous) observer?.unobserve(previous);
    if (anchor) observer?.observe(anchor);
    fit();
  },
);
onMounted(async () => {
  if (!dialog.value) return;
  rememberDialogFocus(dialog.value);
  dialog.value.show();
  unregister = registerDialog(dialog.value, false);
  await nextTick();
  if (disposed || !frame.value) return;
  fit();
  observer = new ResizeObserver(fit);
  observer.observe(frame.value);
  if (props.anchor) observer.observe(props.anchor);
  window.addEventListener("resize", fit);
  window.addEventListener("nekocrate-home-layout", fit);
  document.addEventListener("scroll", fit, true);
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("keydown", keyboard);
  frame.value
    .querySelector<HTMLElement>(".dialog-heading")
    ?.focus({ preventScroll: true });
});
onBeforeUnmount(() => {
  disposed = true;
  observer?.disconnect();
  unregister?.();
  window.removeEventListener("resize", fit);
  window.removeEventListener("nekocrate-home-layout", fit);
  document.removeEventListener("scroll", fit, true);
  document.removeEventListener("pointerdown", outside, true);
  document.removeEventListener("keydown", keyboard);
});
</script>

<template>
  <dialog
    ref="dialog"
    class="settings-bubble"
    :aria-label="label"
    data-drop-kind="blocked"
    @cancel.prevent="close()"
  >
    <div
      ref="frame"
      class="settings-bubble-frame"
      :data-side="position?.side"
      :style="{
        left: position ? `${position.left}px` : undefined,
        top: position ? `${position.top}px` : undefined,
        visibility: position ? undefined : 'hidden',
        '--bubble-tip': `${position?.tip || 0}px`,
        '--bubble-origin': position?.origin,
      }"
    >
      <slot />
    </div>
  </dialog>
</template>

<style>
.settings-bubble {
  position: fixed;
  inset: 0;
  z-index: 95;
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  margin: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ink);
  overflow: visible;
  pointer-events: none;
}
.settings-bubble-frame {
  position: absolute;
  width: max-content;
  max-width: calc(100vw - 24px);
  pointer-events: auto;
  transform-origin: var(--bubble-origin, 50% 50%);
}
.settings-bubble-frame > section {
  --glass-fill: color-mix(in srgb, var(--surface) 92%, transparent);
  max-height: calc(100vh - 24px);
  border-radius: 12px;
  overflow: auto;
  scrollbar-width: none;
  box-shadow:
    var(--glass-rim),
    0 12px 36px #00000038;
}
.settings-bubble-frame > section::before,
.settings-bubble-frame > section::after {
  display: none;
}
/* Floating selectors teleport into this dialog, outside the scrollable body. */
.settings-bubble > :not(.settings-bubble-frame) {
  pointer-events: auto;
}
.settings-bubble-frame::before {
  content: "";
  display: none;
  position: absolute;
  z-index: 1;
  left: calc(var(--bubble-tip) - 5px);
  width: 10px;
  height: 10px;
  background: var(--surface);
  border: 1px solid var(--glass-stroke);
  transform: rotate(45deg);
  pointer-events: none;
}
.settings-bubble-frame[data-side="below"]::before {
  display: block;
  top: -5px;
  border-right: 0;
  border-bottom: 0;
}
.settings-bubble-frame[data-side="above"]::before {
  display: block;
  bottom: -5px;
  border-left: 0;
  border-top: 0;
}
.settings-bubble.dialog-enter-active .settings-bubble-frame,
.settings-bubble.dialog-leave-active .settings-bubble-frame {
  transition: transform var(--motion-panel) var(--motion-ease);
}
.settings-bubble.dialog-enter-from .settings-bubble-frame,
.settings-bubble.dialog-leave-to .settings-bubble-frame {
  transform: translateY(4px) scale(0.98);
}
.settings-bubble.dialog-leave-active .settings-bubble-frame {
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .settings-bubble .settings-bubble-frame {
    transition: none !important;
    transform: none !important;
  }
}
</style>
