<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { Plus } from "lucide-vue-next";
import {
  createInstanceWheelStepper,
  instanceWheelFrame,
  type InstanceWheelBounds,
} from "../instanceWheel";
import { t } from "../i18n";
import type { Instance } from "../types";

const props = defineProps<{
  instances: Instance[];
  selected?: string;
  editing: boolean;
  dragging: boolean;
  busy: boolean;
  bounds?: InstanceWheelBounds;
}>();
const emit = defineEmits<{
  select: [id: string];
  add: [];
  surface: [element: HTMLElement | undefined];
}>();
const root = ref<HTMLElement>();
const viewport = ref<HTMLElement>();
const hovering = ref(false);
const keyboardOpen = ref(false);
const dropOpen = ref(false);
const open = computed(
  () =>
    props.instances.length > 0 &&
    (props.editing ||
      hovering.value ||
      keyboardOpen.value ||
      dropOpen.value ||
      props.dragging),
);
const dragCenter = ref<number>();
const center = computed(() =>
  Math.min(
    Math.max(0, props.instances.length - 1),
    dragCenter.value ??
      Math.max(
        0,
        props.instances.findIndex((item) => item.id === props.selected),
      ),
  ),
);
const radius = computed(
  () =>
    Math.ceil(
      Math.max(
        placement.value.iconY,
        placement.value.height - placement.value.iconY,
      ) / 50,
    ) + 2,
);
const rows = computed(() =>
  props.instances
    .slice(
      Math.max(0, center.value - radius.value),
      center.value + radius.value + 1,
    )
    .map((instance, i) => ({
      instance,
      offset: Math.max(0, center.value - radius.value) + i - center.value,
    })),
);
const placement = ref({
  left: 0,
  top: 0,
  width: 260,
  height: 320,
  iconX: 228,
  iconY: 160,
  leftLabels: true,
});
const stepper = createInstanceWheelStepper();
let hideTimer: ReturnType<typeof setTimeout> | undefined;
let observer: ResizeObserver | undefined;
function place() {
  if (root.value)
    placement.value = instanceWheelFrame(
      root.value.getBoundingClientRect(),
      {
        width: window.innerWidth,
        height: window.innerHeight,
      },
      props.bounds,
    );
}
function enter() {
  clearTimeout(hideTimer);
  place();
  hovering.value = true;
}
function leave() {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    hovering.value = false;
    keyboardOpen.value = false;
    stepper.reset();
  }, 180);
}
function close() {
  clearTimeout(hideTimer);
  hovering.value = keyboardOpen.value = dropOpen.value = false;
  stepper.reset();
}
function revealForDrop() {
  place();
  dropOpen.value = true;
}
function endDrop() {
  dropOpen.value = false;
  if (!root.value?.matches(":hover")) close();
}
function focusItem(id: string | undefined) {
  const item = Array.from(
    viewport.value?.querySelectorAll<HTMLElement>("[data-instance-id]") ?? [],
  ).find(
    (element) =>
      element.dataset.instanceId === id &&
      !element.closest(".wheel-row-leave-active"),
  );
  item
    ?.querySelector<HTMLButtonElement>(".instance-tile")
    ?.focus({ preventScroll: true });
}
function choose(index: number, focus = false) {
  const item =
    props.instances[Math.max(0, Math.min(props.instances.length - 1, index))];
  if (!item) return;
  if (props.dragging) dragCenter.value = props.instances.indexOf(item);
  if (item.id !== props.selected) emit("select", item.id);
  if (focus) void nextTick(() => focusItem(item.id));
}
function wheel(event: WheelEvent) {
  if (
    props.editing ||
    event.ctrlKey ||
    event.metaKey ||
    props.instances.length < 2
  )
    return;
  event.preventDefault();
  event.stopPropagation();
  enter();
  const delta =
    Math.abs(event.deltaY) >= Math.abs(event.deltaX)
      ? event.deltaY
      : event.deltaX;
  const step = stepper.step(delta, event.deltaMode, performance.now());
  if (step) choose(center.value + step);
}
function key(event: KeyboardEvent) {
  if (props.editing || event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    focusItem(props.selected);
    close();
    return;
  }
  if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  event.stopPropagation();
  place();
  keyboardOpen.value = true;
  choose(
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? props.instances.length - 1
        : center.value + (event.key === "ArrowUp" ? -1 : 1),
    true,
  );
}
function focusIn(event: FocusEvent) {
  clearTimeout(hideTimer);
  if ((event.target as HTMLElement).matches(":focus-visible")) {
    place();
    keyboardOpen.value = true;
  }
}
function blur(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !root.value?.contains(event.relatedTarget)
  )
    leave();
}
const frameStyle = computed(() => ({
  left: `${placement.value.left}px`,
  top: `${placement.value.top}px`,
  width: `${placement.value.width}px`,
  height: `${placement.value.height}px`,
  "--wheel-fade-top": `${Math.min(36, Math.max(0, placement.value.iconY - 40))}px`,
  "--wheel-fade-bottom": `${Math.min(36, Math.max(0, placement.value.height - placement.value.iconY - 40))}px`,
}));
const addStyle = computed(() => ({
  left: `${placement.value.iconX - 16}px`,
  top: `${placement.value.iconY - 16}px`,
}));
function rowVisible(offset: number) {
  if (!offset) return true;
  const y = placement.value.iconY + offset * 50;
  return open.value && y >= 20 && y <= placement.value.height - 20;
}
function rowPosition(offset: number) {
  return {
    x: placement.value.iconX - 28,
    y: placement.value.iconY - 28 + (open.value ? offset * 50 : 0),
  };
}
function rowStyle(offset: number) {
  const { x, y } = rowPosition(offset),
    distance = Math.abs(offset);
  return {
    // The frame may be remeasured after layout/entrance motion. Apply its
    // inverse offset as layout, so only the candidate spacing is animated.
    left: `${x}px`,
    top: `${placement.value.iconY - 28}px`,
    transform: `translateY(${y - (placement.value.iconY - 28)}px)`,
    opacity: distance
      ? open.value
        ? [1, 0.64, 0.38, 0.22, 0.12][Math.min(4, distance)]
        : 0
      : 1,
    "--wheel-scale":
      offset === 0 ? (open.value ? 1.18 : 1) : distance === 1 ? 0.82 : 0.66,
    "--wheel-font-size": `${distance ? 11 : 14}px`,
  };
}
watch(open, () => {
  if (!open.value) stepper.reset();
});
watch(() => props.editing, close);
watch(() => props.bounds, place, { deep: true });
watch(
  () => props.dragging,
  (dragging) => {
    dragCenter.value = dragging ? center.value : undefined;
  },
  { flush: "sync" },
);
watch(
  () => props.instances.length,
  () => {
    place();
    if (!props.instances.length) close();
  },
);
onMounted(() => {
  emit("surface", viewport.value);
  place();
  observer = new ResizeObserver(place);
  if (root.value) observer.observe(root.value);
  window.addEventListener("resize", place);
  window.addEventListener("nekocrate-home-layout", place);
  window.addEventListener("blur", close);
});
onBeforeUnmount(() => {
  close();
  observer?.disconnect();
  emit("surface", undefined);
  window.removeEventListener("resize", place);
  window.removeEventListener("nekocrate-home-layout", place);
  window.removeEventListener("blur", close);
});
defineExpose({ revealForDrop, endDrop });
</script>

<template>
  <nav
    ref="root"
    class="instance-dock instance-wheel"
    :class="{ 'wheel-open': open, sorting: dragging }"
    :aria-label="t('滚动切换实例，方向键选择')"
    @pointerenter="enter"
    @pointerleave="leave"
    @focusin="focusIn"
    @focusout="blur"
    @keydown="key"
    @wheel="wheel"
    @dragstart.prevent
  >
    <div
      ref="viewport"
      class="instance-wheel-viewport"
      data-orientation="wheel"
      :data-label-side="placement.leftLabels ? 'left' : 'right'"
      :style="frameStyle"
    >
      <TransitionGroup
        tag="div"
        class="instance-wheel-list"
        name="wheel-row"
        move-class="wheel-row-static"
      >
        <div
          v-for="row in rows"
          :key="row.instance.id"
          class="instance-wheel-row"
          :class="{ current: row.offset === 0 }"
          :data-resource-instance="row.instance.id"
          :data-resource-version="row.instance.selected_version || undefined"
          data-drop-kind="instance"
          :data-wheel-visible="rowVisible(row.offset)"
          :inert="!rowVisible(row.offset)"
          :aria-hidden="!rowVisible(row.offset)"
          :data-wheel-x="rowPosition(row.offset).x"
          :data-wheel-y="rowPosition(row.offset).y"
          :style="rowStyle(row.offset)"
        >
          <slot
            :instance="row.instance"
            :tab-index="row.offset === 0 ? 0 : -1"
          />
          <div class="instance-wheel-label">
            <button
              v-if="row.offset === 0"
              class="icon-button instance-wheel-add"
              :disabled="busy"
              :aria-label="t('添加游戏目录')"
              :title="t('添加游戏目录')"
              :inert="!open"
              @click="emit('add')"
            >
              <Plus :size="20" />
            </button>
            <button
              class="instance-wheel-name"
              :tabindex="-1"
              @click="choose(center + row.offset)"
            >
              {{ row.instance.name }}
            </button>
          </div>
        </div>
      </TransitionGroup>
      <button
        v-if="!instances.length"
        class="icon-button instance-wheel-add wheel-empty"
        :style="addStyle"
        :disabled="busy"
        :aria-label="t('添加游戏目录')"
        :title="t('添加游戏目录')"
        @click="emit('add')"
      >
        <Plus :size="20" />
      </button>
    </div>
  </nav>
</template>

<style>
.instance-wheel.instance-dock {
  width: 64px;
  height: 64px;
  max-width: none;
  user-select: none;
  -webkit-user-select: none;
}
.instance-wheel-viewport {
  position: absolute;
  pointer-events: none;
}
.instance-wheel-list {
  position: absolute;
  inset: 0;
  overflow: hidden;
  mask-image: linear-gradient(
    to bottom,
    transparent,
    #000 var(--wheel-fade-top),
    #000 calc(100% - var(--wheel-fade-bottom)),
    transparent
  );
  -webkit-mask-image: linear-gradient(
    to bottom,
    transparent,
    #000 var(--wheel-fade-top),
    #000 calc(100% - var(--wheel-fade-bottom)),
    transparent
  );
}
.wheel-open .instance-wheel-viewport,
.instance-wheel-row.current {
  pointer-events: auto;
}
.instance-wheel-row {
  position: absolute;
  left: 0;
  top: 0;
  width: 56px;
  height: 56px;
  transition:
    transform 320ms cubic-bezier(0.16, 1, 0.3, 1),
    opacity 220ms;
}
.instance-wheel-row .instance-item {
  width: 56px;
  height: 56px;
}
.instance-wheel-row .instance-art {
  transform: scale(var(--wheel-scale));
  transition:
    transform 320ms cubic-bezier(0.16, 1, 0.3, 1),
    opacity 180ms,
    filter 180ms;
}
.instance-wheel-label {
  position: absolute;
  left: 64px;
  top: 12px;
  display: flex;
  align-items: center;
  gap: 4px;
  width: max-content;
  max-width: 188px;
  height: 32px;
  pointer-events: none;
}
[data-label-side="left"] .instance-wheel-label {
  left: auto;
  right: 64px;
}
.wheel-open .instance-wheel-label {
  pointer-events: auto;
}
.instance-wheel-name {
  min-width: 0;
  padding: 0;
  border: 0;
  background: none;
  color: var(--muted);
  font-size: var(--wheel-font-size);
  line-height: 22px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  opacity: 0;
  pointer-events: none;
  transition:
    opacity 180ms,
    font-size 280ms var(--motion-ease),
    color 180ms;
}
[data-label-side="left"] .instance-wheel-name {
  text-align: right;
}
.wheel-open .instance-wheel-name {
  opacity: 1;
  pointer-events: auto;
}
.instance-wheel-row.current .instance-wheel-name {
  color: var(--ink);
  text-shadow: 0 0 10px var(--glow);
}
.instance-wheel-name:hover {
  color: var(--blue);
}
.instance-wheel .instance-tile:focus-visible,
.instance-wheel-name:focus {
  outline: none;
}
.instance-wheel-name:focus-visible {
  color: var(--ink);
  text-shadow: 0 0 10px var(--glow);
}
.instance-wheel .instance-wheel-add {
  flex: 0 0 32px;
  width: 32px;
  height: 32px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 180ms;
}
.instance-wheel .instance-wheel-add.wheel-empty {
  position: absolute;
}
.wheel-open .instance-wheel-add,
.instance-wheel .instance-wheel-add.wheel-empty {
  opacity: 1;
  pointer-events: auto;
}
.wheel-row-enter-active,
.wheel-row-leave-active {
  transition: opacity 200ms !important;
}
.wheel-row-static {
  transition: none !important;
} /* Positions already animate around the selected row; never apply a second FLIP transform. */
.wheel-row-enter-from,
.wheel-row-leave-to {
  opacity: 0 !important;
}
.wheel-row-leave-active {
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .instance-wheel-viewport,
  .instance-wheel-row,
  .instance-wheel-row .instance-art,
  .instance-wheel-name,
  .wheel-row-enter-active,
  .wheel-row-leave-active {
    transition: none !important;
  }
}
</style>
