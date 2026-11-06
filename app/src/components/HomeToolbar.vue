<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { Boxes, Download, Plus, Settings2, Terminal, X } from "lucide-vue-next";
import { homeToolLabels, type HomeTool } from "../homeToolbar";
import { t } from "../i18n";
import { listRevealStyle } from "../listMotion";
const props = defineProps<{
  items: HomeTool[];
  editing: boolean;
  orientation: "horizontal" | "vertical";
  settingsOpen: boolean;
}>();
const emit = defineEmits<{
  update: [items: HomeTool[]];
  activate: [id: HomeTool];
  edit: [];
  settingsAnchor: [element: HTMLElement | undefined];
}>();
const icons = {
  resources: Boxes,
  settings: Settings2,
  logs: Terminal,
  store: Download,
};
const root = ref<HTMLElement>();
const preview = ref<HomeTool[]>();
const order = computed(() => preview.value || props.items);
const dragging = ref<HomeTool>();
const point = ref({ x: 0, y: 0 });
let pointer:
  | { id: number; tool: HomeTool; x: number; y: number; original: HomeTool[] }
  | undefined;
function buttonRef(id: HomeTool, element: unknown) {
  if (id === "settings")
    emit("settingsAnchor", (element || undefined) as HTMLElement | undefined);
}
function select(event: Event) {
  event.preventDefault();
}
function start(event: PointerEvent, tool: HomeTool) {
  if (!props.editing || !event.isPrimary || event.button !== 0 || pointer)
    return;
  event.preventDefault();
  (event.currentTarget as HTMLElement).focus({ preventScroll: true });
  pointer = {
    id: event.pointerId,
    tool,
    x: event.clientX,
    y: event.clientY,
    original: [...props.items],
  };
  root.value?.setPointerCapture(event.pointerId);
  document.addEventListener("selectstart", select, true);
  window.addEventListener("blur", cancel);
}
function move(event: PointerEvent) {
  if (!pointer || pointer.id !== event.pointerId || !root.value) return;
  if (
    !dragging.value &&
    Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) < 5
  )
    return;
  event.preventDefault();
  dragging.value = pointer.tool;
  point.value = { x: event.clientX, y: event.clientY };
  const box = root.value.getBoundingClientRect();
  const offset =
    props.orientation === "vertical"
      ? event.clientY - box.top
      : event.clientX - box.left;
  // Fixed slot centres, rather than animated tile rects, keep stationary drags stable.
  const index = Math.max(
    0,
    Math.min(props.items.length - 1, Math.round((offset - 22) / 50)),
  );
  const next = pointer.original.filter((id) => id !== pointer!.tool);
  next.splice(index, 0, pointer.tool);
  preview.value = next;
}
function finish(commit: boolean) {
  const previous = pointer;
  pointer = undefined;
  if (previous && root.value?.hasPointerCapture(previous.id))
    root.value.releasePointerCapture(previous.id);
  if (commit && preview.value) emit("update", [...preview.value]);
  dragging.value = undefined;
  preview.value = undefined;
  document.removeEventListener("selectstart", select, true);
  window.removeEventListener("blur", cancel);
}
function release(event: PointerEvent) {
  if (event.pointerId !== pointer?.id) return;
  if (dragging.value) move(event);
  const box = root.value?.getBoundingClientRect();
  finish(
    !!box &&
      event.clientX >= box.left - 16 &&
      event.clientX <= box.right + 16 &&
      event.clientY >= box.top - 16 &&
      event.clientY <= box.bottom + 16,
  );
}
function cancel() {
  finish(false);
}
function remove(id: HomeTool) {
  if (props.editing && id !== "settings")
    emit(
      "update",
      props.items.filter((tool) => tool !== id),
    );
}
function keyboard(event: KeyboardEvent, id?: HomeTool) {
  if (event.key === "Escape" && pointer) {
    event.preventDefault();
    event.stopPropagation();
    cancel();
    return;
  }
  if (!props.editing || !id) return;
  if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    remove(id);
    return;
  }
  if (
    !event.altKey ||
    !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)
  )
    return;
  event.preventDefault();
  const items = [...props.items],
    index = items.indexOf(id);
  const target =
    index + (["ArrowLeft", "ArrowUp"].includes(event.key) ? -1 : 1);
  if (target < 0 || target >= items.length) return;
  [items[index], items[target]] = [items[target], items[index]];
  emit("update", items);
  void nextTick(() =>
    root.value
      ?.querySelector<HTMLButtonElement>(`[data-home-tool="${id}"] > button`)
      ?.focus(),
  );
}
watch(() => props.editing, cancel);
watch(() => props.orientation, cancel);
onBeforeUnmount(() => {
  cancel();
  emit("settingsAnchor", undefined);
});
</script>

<template>
  <TransitionGroup
    tag="div"
    name="home-tool"
    class="global-tools home-toolbar"
    :class="{ 'editing-tools': editing }"
    :ref="(element) => (root = (element as any)?.$el || element)"
    @pointermove="move"
    @pointerup="release"
    @pointercancel="cancel"
    @lostpointercapture="cancel"
    @contextmenu.prevent="!editing && emit('edit')"
    @dragstart.prevent
    @keydown="keyboard($event)"
  >
    <div
      v-for="(id, index) in order"
      :key="id"
      class="home-tool-item"
      :data-home-tool="id"
      :class="{ 'tool-dragging': dragging === id }"
    >
      <button
        :ref="(element) => buttonRef(id, element)"
        class="icon-button ui-reveal-item"
        :style="listRevealStyle(index + 1)"
        :title="t(homeToolLabels[id])"
        :aria-label="t(homeToolLabels[id])"
        :aria-expanded="id === 'settings' ? settingsOpen : undefined"
        :aria-haspopup="id === 'settings' ? 'dialog' : undefined"
        @pointerdown="start($event, id)"
        @keydown="keyboard($event, id)"
        @click="!editing && emit('activate', id)"
      >
        <component :is="icons[id]" :size="20" />
      </button>
      <button
        v-if="editing && id !== 'settings'"
        class="home-tool-remove"
        :title="t('移除 {name}', { name: t(homeToolLabels[id]) })"
        :aria-label="t('移除 {name}', { name: t(homeToolLabels[id]) })"
        @pointerdown.stop
        @click.stop="remove(id)"
      >
        <X :size="11" />
      </button>
    </div>
    <button
      v-if="!items.length && !editing"
      key="empty"
      class="icon-button"
      :aria-label="t('编辑工具栏')"
      :title="t('编辑工具栏')"
      @click="emit('edit')"
    >
      <Plus :size="20" />
    </button>
  </TransitionGroup>
  <Teleport v-if="dragging" to=".shell">
    <div
      class="home-tool-ghost"
      :style="{ left: `${point.x - 22}px`, top: `${point.y - 22}px` }"
    >
      <component :is="icons[dragging]" :size="20" />
    </div>
  </Teleport>
</template>

<style>
.home-toolbar {
  position: relative;
}
.home-tool-item {
  position: relative;
  width: 44px;
  height: 44px;
  flex-shrink: 0;
}
.editing-tools,
.editing-tools * {
  user-select: none;
  -webkit-user-select: none;
}
.editing-tools .home-tool-item > .icon-button {
  cursor: grab;
  touch-action: none;
}
.home-tool-item.tool-dragging {
  opacity: 0;
}
.home-tool-remove {
  position: absolute;
  top: -2px;
  right: -2px;
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: var(--night);
  color: var(--muted);
  opacity: 0;
  pointer-events: none;
  transition:
    opacity 120ms,
    color 120ms;
}
.home-tool-item:hover .home-tool-remove,
.home-tool-item:focus-within .home-tool-remove {
  opacity: 1;
  pointer-events: auto;
}
.home-tool-remove:hover {
  color: var(--danger);
}
.home-tool-move {
  transition: transform 280ms var(--motion-ease);
}
.layout-dragging .home-tool-move {
  transition: none;
}
.home-tool-enter-active,
.home-tool-leave-active {
  transition:
    opacity 160ms,
    transform 220ms var(--motion-ease);
}
.home-tool-enter-from,
.home-tool-leave-to {
  opacity: 0;
  transform: scale(0.85);
}
.home-tool-leave-active {
  position: absolute;
  pointer-events: none;
}
.home-tool-ghost {
  position: fixed;
  z-index: 160;
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  color: var(--blue);
  filter: drop-shadow(0 0 6px var(--glow));
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .home-tool-move,
  .home-tool-enter-active,
  .home-tool-leave-active {
    transition: none;
  }
}
</style>
