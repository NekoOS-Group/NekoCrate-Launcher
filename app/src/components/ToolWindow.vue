<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  provide,
} from "vue";
import { t } from "../i18n";
import { activateDialog, registerDialog } from "../dialogSurface";
import { dropInteraction } from "../dropSurface";
import { rememberDialogFocus } from "../dialogMotion";
import {
  focusToolWindow,
  raiseToolWindow,
  registerToolWindow,
  toolOrder,
  windowControlsKey,
} from "../toolWindows";
import {
  fitWindow,
  resizeWindow,
  snapAt,
  snappedWindow,
  validWindowLayout,
  type WindowLayout,
  type WindowRect,
  type WindowSnap,
} from "../windowGeometry";

const props = defineProps<{
  windowId: string;
  label: string;
  locked?: boolean;
  layoutKey?: string;
  initialSize?: { width: number; height: number };
}>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
const frame = ref<HTMLElement>();
const rect = ref<WindowRect>();
const snap = ref<WindowSnap>();
const preview = ref<WindowSnap>();
const interacting = ref(false);
let restored: WindowRect;
let unregister: (() => void) | undefined;
let unregisterSurface: (() => void) | undefined;
let disposed = false;
let pointer:
  | {
      id: number;
      x: number;
      y: number;
      start: WindowRect;
      edge: string;
      moved: boolean;
      snap?: WindowSnap;
    }
  | undefined;
const edges = ["n", "e", "s", "w", "ne", "nw", "se", "sw"];
const area = () => ({ width: window.innerWidth, height: window.innerHeight });
const active = computed(() => toolOrder.value.at(-1) === props.windowId);
const layer = computed(
  () =>
    50 +
    (40 * (Math.max(0, toolOrder.value.indexOf(props.windowId)) + 1)) /
      (toolOrder.value.length + 1),
);
const style = (box?: WindowRect) =>
  box
    ? {
        left: `${box.x}px`,
        top: `${box.y}px`,
        width: `${box.width}px`,
        height: `${box.height}px`,
      }
    : {};
const previewStyle = computed(() =>
  style(preview.value ? snappedWindow(preview.value, area()) : undefined),
);
const storageKey = () =>
  `nekocrate.window.${props.layoutKey || props.windowId}`;
function save() {
  if (!rect.value) return;
  try {
    localStorage.setItem(
      storageKey(),
      JSON.stringify({
        rect: snap.value ? restored : rect.value,
        snap: snap.value,
      } satisfies WindowLayout),
    );
  } catch {
    /* Storage may be unavailable. */
  }
}
function focus() {
  if (!dialog.value || dialog.value.dataset.closing) return;
  activateDialog(dialog.value);
  const target =
    dialog.value.querySelector<HTMLElement>(".dialog-heading") || frame.value;
  target?.focus({ preventScroll: true });
}
function raise() {
  if (dialog.value?.dataset.closing) return;
  if (!active.value) raiseToolWindow(props.windowId);
  if (dialog.value) activateDialog(dialog.value);
}
function close() {
  if (!props.locked && !dropInteraction.value) emit("close");
}
function titleTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement) || !frame.value?.contains(target))
    return false;
  return (
    !!target.closest(
      ".modal-header, .utility-panel-header, .log-panel > header, .workflow-header, [data-window-title]",
    ) &&
    !target.closest(
      "button, input, textarea, select, a, [role=button], [role=tab], [contenteditable=true], [data-no-window-drag]",
    )
  );
}
function down(event: PointerEvent, edge = "") {
  if (
    event.button !== 0 ||
    !event.isPrimary ||
    !rect.value ||
    dropInteraction.value
  )
    return;
  raise();
  if (!edge && !titleTarget(event.target)) return;
  event.preventDefault();
  frame.value?.focus({ preventScroll: true });
  const start = { ...rect.value };
  pointer = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    start,
    edge,
    moved: false,
    snap: snap.value,
  };
  // Suppress selection from pointerdown, before the drag threshold is reached.
  document.documentElement.classList.add("tool-window-manipulating");
  document.addEventListener("selectstart", preventSelection, true);
  document.addEventListener("pointermove", move, { passive: false });
  document.addEventListener("pointerup", up);
  document.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", cancel);
}
function preventSelection(event: Event) {
  if (pointer) event.preventDefault();
}
function move(event: PointerEvent) {
  if (!pointer || pointer.id !== event.pointerId) return;
  event.preventDefault();
  let dx = event.clientX - pointer.x,
    dy = event.clientY - pointer.y;
  if (!pointer.moved && Math.hypot(dx, dy) < 4) return;
  if (!pointer.moved) {
    pointer.moved = true;
    frame.value?.setPointerCapture(event.pointerId);
    interacting.value = true;
    window.getSelection()?.removeAllRanges();
    if (snap.value && !pointer.edge) {
      const ratio = (pointer.x - pointer.start.x) / pointer.start.width;
      pointer.start = fitWindow(
        {
          ...restored,
          x: event.clientX - restored.width * ratio,
          y: event.clientY - Math.min(pointer.y - pointer.start.y, 50),
        },
        area(),
      );
      pointer.x = event.clientX;
      pointer.y = event.clientY;
      dx = dy = 0;
    }
    snap.value = undefined;
  }
  if (pointer.edge)
    rect.value = resizeWindow(pointer.start, pointer.edge, dx, dy, area());
  else {
    rect.value = fitWindow(
      { ...pointer.start, x: pointer.start.x + dx, y: pointer.start.y + dy },
      area(),
    );
    preview.value = snapAt(event.clientX, event.clientY, area());
  }
}
function finish(commit: boolean) {
  const current = pointer;
  if (!current) return;
  pointer = undefined;
  if (!commit) {
    snap.value = current.snap;
    rect.value = snap.value
      ? snappedWindow(snap.value, area())
      : fitWindow(current.start, area());
  } else if (current.moved && preview.value && rect.value) {
    restored = { ...rect.value };
    snap.value = preview.value;
    rect.value = snappedWindow(snap.value, area());
  }
  preview.value = undefined;
  interacting.value = false;
  document.documentElement.classList.remove("tool-window-manipulating");
  document.removeEventListener("selectstart", preventSelection, true);
  document.removeEventListener("pointermove", move);
  document.removeEventListener("pointerup", up);
  document.removeEventListener("pointercancel", cancel);
  window.removeEventListener("blur", cancel);
  if (frame.value?.hasPointerCapture(current.id))
    frame.value.releasePointerCapture(current.id);
  if (commit && current.moved) save();
}
function up(event: PointerEvent) {
  if (pointer?.id === event.pointerId) finish(true);
}
function cancel() {
  finish(false);
}
function maximize() {
  if (!rect.value) return;
  if (snap.value) {
    rect.value = fitWindow(restored, area());
    snap.value = undefined;
  } else {
    restored = { ...rect.value };
    snap.value = "maximized";
    rect.value = snappedWindow(snap.value, area());
  }
  save();
}
provide(windowControlsKey, {
  expanded: computed(() => !!snap.value),
  toggle: maximize,
});
function doubleClick(event: MouseEvent) {
  if (titleTarget(event.target)) {
    event.preventDefault();
    maximize();
  }
}
function keyboard(event: KeyboardEvent) {
  if (!active.value || event.defaultPrevented || dropInteraction.value) return;
  if (event.key === "Escape") {
    if (
      (event.target as Element)?.closest(
        '[role="listbox"], [aria-expanded="true"], .cm-editor',
      )
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    if (pointer) cancel();
    else close();
  } else if (event.altKey && event.key === "Enter") {
    event.preventDefault();
    maximize();
  } else if (event.altKey && event.key.startsWith("Arrow") && rect.value) {
    event.preventDefault();
    const dx =
      event.key === "ArrowLeft" ? -20 : event.key === "ArrowRight" ? 20 : 0;
    const dy =
      event.key === "ArrowUp" ? -20 : event.key === "ArrowDown" ? 20 : 0;
    snap.value = undefined;
    rect.value = event.shiftKey
      ? resizeWindow(rect.value, "se", dx, dy, area())
      : fitWindow(
          { ...rect.value, x: rect.value.x + dx, y: rect.value.y + dy },
          area(),
        );
    save();
  }
}
function viewportChanged() {
  if (pointer) cancel();
  if (rect.value)
    rect.value = snap.value
      ? snappedWindow(snap.value, area())
      : fitWindow(rect.value, area());
}
onMounted(async () => {
  if (!dialog.value) return;
  rememberDialogFocus(dialog.value);
  dialog.value.show();
  unregisterSurface = registerDialog(dialog.value, false);
  await nextTick();
  if (!frame.value || disposed) return;
  const measured = frame.value.getBoundingClientRect();
  const offset = (toolOrder.value.length % 5) * 24;
  const width =
    props.initialSize?.width || (measured.width > 200 ? measured.width : 760);
  const height =
    props.initialSize?.height ||
    (measured.height > 100 ? measured.height : 520);
  rect.value = fitWindow(
    {
      x: (innerWidth - width) / 2 + offset,
      y: (innerHeight - height) / 2 + offset,
      width,
      height,
    },
    area(),
  );
  try {
    const saved: unknown = JSON.parse(
      localStorage.getItem(storageKey()) || "null",
    );
    if (validWindowLayout(saved)) {
      rect.value = fitWindow(saved.rect, area());
      snap.value = saved.snap;
    }
  } catch {
    /* Ignore old/corrupt layouts. */
  }
  restored = { ...rect.value };
  if (snap.value) rect.value = snappedWindow(snap.value, area());
  unregister = registerToolWindow({ id: props.windowId, focus, close });
  focusToolWindow(props.windowId);
  window.addEventListener("resize", viewportChanged);
});
onBeforeUnmount(() => {
  disposed = true;
  cancel();
  window.removeEventListener("resize", viewportChanged);
  unregisterSurface?.();
  unregister?.();
});
</script>

<template>
  <dialog
    ref="dialog"
    class="tool-window"
    data-drop-kind="blocked"
    :style="{ zIndex: layer }"
    :data-window-id="windowId"
    :aria-label="label"
    @keydown="keyboard"
    @cancel.prevent="close"
  >
    <div
      v-if="preview"
      class="tool-snap-preview"
      :style="previewStyle"
      aria-hidden="true"
    />
    <div
      ref="frame"
      class="tool-window-frame"
      :class="{
        'is-ready': rect,
        'is-active': active,
        'is-interacting': interacting,
      }"
      :style="style(rect)"
      tabindex="-1"
      @pointerdown.capture="raise"
      @focusin="raise"
      @pointerdown="down($event)"
      @dblclick="doubleClick"
    >
      <Transition name="tool-panel"><slot /></Transition>
      <div
        v-for="edge in edges"
        :key="edge"
        class="tool-resize"
        :class="`tool-resize-${edge}`"
        aria-hidden="true"
        @pointerdown.stop="down($event, edge)"
      />
    </div>
  </dialog>
</template>

<style>
.tool-window {
  position: fixed;
  inset: 0;
  margin: 0;
  padding: 0;
  border: 0;
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  background: transparent;
  color: var(--ink);
  overflow: visible;
  pointer-events: none;
}
.tool-window-frame {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  pointer-events: auto;
  max-width: calc(100vw - 24px);
  max-height: calc(100vh - 24px);
  outline: none;
}
.tool-window-frame.is-ready {
  transform: none;
  container: tool-window / inline-size;
  transition:
    left 220ms var(--motion-ease),
    top 220ms var(--motion-ease),
    width 220ms var(--motion-ease),
    height 220ms var(--motion-ease);
}
.tool-window-frame.is-interacting {
  transition: none;
}
.tool-window-frame.is-ready > section {
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  max-width: none;
  max-height: none;
  margin: 0;
}
.tool-window-frame > .glass-surface {
  --glass-fill: color-mix(in srgb, var(--surface) 94%, transparent);
}
.tool-panel-enter-active,
.tool-panel-leave-active {
  transition: opacity var(--motion-fade) var(--motion-ease);
}
.tool-panel-enter-from,
.tool-panel-leave-to {
  opacity: 0;
}
.tool-window-frame > .tool-panel-leave-active {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
.tool-window-frame.is-active > .glass-surface {
  border-color: color-mix(in srgb, var(--blue) 50%, transparent);
}
.tool-window-frame
  :is(
    .modal-header,
    .utility-panel-header,
    .log-panel > header,
    .workflow-header,
    [data-window-title]
  ) {
  cursor: grab;
  touch-action: none;
  -webkit-user-select: none;
  user-select: none;
}
.tool-window-frame :is(button, input, select, a, textarea) {
  touch-action: auto;
}
.tool-window-frame :is(input, textarea, [contenteditable="true"]) {
  -webkit-user-select: text;
  user-select: text;
}
.tool-window-frame .version-manager-body {
  flex: 1;
  height: auto;
  min-height: 0;
}
.tool-window-frame .resource-main,
.tool-window-frame .resource-plan-list {
  flex: 1;
  height: auto;
  min-height: 0;
}
.tool-window-frame .resource-main > .resource-results {
  min-height: 0;
}
.tool-window-frame .resource-header > .window-maximize {
  margin-left: auto;
}
.tool-window-frame .resource-header > .window-maximize + .resource-close {
  margin-left: 0;
}
.tool-window-frame .version-download {
  overflow: hidden;
}
.tool-window-frame .version-download-form {
  flex: 1;
  min-height: 0;
}
.tool-window-frame > .download-panel .utility-panel-content {
  flex: 1;
  min-height: 0;
}
.tool-window > .error-notice,
.tool-window > .toast,
.tool-window > .source-select-menu {
  pointer-events: auto;
}
.tool-window-manipulating,
.tool-window-manipulating * {
  -webkit-user-select: none !important;
  user-select: none !important;
}
.tool-snap-preview {
  position: fixed;
  background: color-mix(in srgb, var(--blue) 9%, transparent);
  border: 1px solid color-mix(in srgb, var(--blue) 65%, transparent);
  box-shadow: inset 0 0 24px var(--glow);
  pointer-events: none;
  transition: all 160ms var(--motion-ease);
}
.tool-resize {
  position: absolute;
  touch-action: none;
  -webkit-user-select: none;
  user-select: none;
}
.tool-resize-n,
.tool-resize-s {
  left: 9px;
  right: 9px;
  height: 8px;
  cursor: ns-resize;
}
.tool-resize-n {
  top: -4px;
}
.tool-resize-s {
  bottom: -4px;
}
.tool-resize-e,
.tool-resize-w {
  top: 9px;
  bottom: 9px;
  width: 8px;
  cursor: ew-resize;
}
.tool-resize-e {
  right: -4px;
}
.tool-resize-w {
  left: -4px;
}
.tool-resize-ne,
.tool-resize-nw,
.tool-resize-se,
.tool-resize-sw {
  width: 14px;
  height: 14px;
}
.tool-resize-ne,
.tool-resize-sw {
  cursor: nesw-resize;
}
.tool-resize-nw,
.tool-resize-se {
  cursor: nwse-resize;
}
.tool-resize-ne {
  top: -4px;
  right: -4px;
}
.tool-resize-nw {
  top: -4px;
  left: -4px;
}
.tool-resize-se {
  bottom: -4px;
  right: -4px;
}
.tool-resize-sw {
  bottom: -4px;
  left: -4px;
}
.tool-resize-se::after {
  content: "";
  position: absolute;
  right: 5px;
  bottom: 5px;
  width: 5px;
  height: 5px;
  border-right: 1px solid var(--muted);
  border-bottom: 1px solid var(--muted);
}
.dialog-enter-active.tool-window .tool-window-frame,
.dialog-leave-active.tool-window .tool-window-frame {
  transition:
    opacity 180ms var(--motion-ease),
    translate 220ms var(--motion-ease);
}
.dialog-enter-from.tool-window .tool-window-frame {
  opacity: 0;
  translate: 0 8px;
}
.dialog-leave-to.tool-window .tool-window-frame {
  opacity: 0;
  translate: 0 -8px;
}
.dialog-leave-active.tool-window .tool-window-frame {
  pointer-events: none;
}
@container tool-window (max-width: 620px) {
  .tool-window-frame.is-ready .version-manager-body {
    grid-template-columns: 128px minmax(0, 1fr) !important;
  }
  .tool-window-frame.is-ready .version-manager .modal-header,
  .tool-window-frame.is-ready .resource-manager .modal-header {
    gap: 6px;
    padding-inline: 12px;
  }
  .tool-window-frame.is-ready .version-manager .modal-header h2 {
    font-size: 12px;
  }
  .tool-window-frame.is-ready .version-manager-tabs {
    overflow-x: auto;
    flex-shrink: 0;
  }
  .tool-window-frame.is-ready .version-manager-tabs .settings-tab {
    padding-inline: 6px;
    gap: 4px;
    font-size: 10px;
  }
  .tool-window-frame.is-ready .rm-toolbar {
    flex-wrap: wrap;
    gap: 4px;
  }
  .tool-window-frame.is-ready .rm-resource-tools {
    margin-left: 0;
    flex: 1;
  }
  .tool-window-frame.is-ready .rm-search {
    flex: 1;
  }
  .tool-window-frame.is-ready .rm-tabs {
    gap: 2px;
  }
  .tool-window-frame.is-ready .rm-tabs button {
    padding-inline: 4px;
    gap: 4px;
    font-size: 12px;
  }
  .tool-window-frame.is-ready .rm-tile-grid {
    gap: 12px;
    grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
  }
  .tool-window-frame.is-ready .rm-tile {
    width: 100%;
  }
  .tool-window-frame.is-ready .resource-manager {
    padding: 14px;
  }
  .tool-window-frame.is-ready .resource-tools {
    flex-wrap: wrap;
    gap: 8px;
  }
  .tool-window-frame.is-ready .log-filters {
    grid-template-columns: 1fr 1fr;
  }
  .tool-window-frame.is-ready .log-search {
    grid-column: 1 / -1;
  }
}
@container tool-window (max-width: 620px) {
  .tool-window-frame.is-ready .resource-header {
    flex-wrap: wrap;
  }
  .tool-window-frame.is-ready .library-filters {
    order: 5;
    flex-basis: 100%;
    margin-left: 0;
    gap: 8px;
  }
  .tool-window-frame.is-ready .library-filters .source-select {
    flex: 1;
    width: auto;
    min-width: 0;
  }
  .tool-window-frame.is-ready .workflow-header-actions {
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .tool-window-frame.is-ready .workflow-header {
    padding: 18px 14px;
    gap: 6px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .tool-window-frame.is-ready,
  .tool-snap-preview,
  .dialog-enter-active.tool-window .tool-window-frame,
  .dialog-leave-active.tool-window .tool-window-frame {
    transition: none;
  }
}
</style>
