<script setup>
import { ref, computed, onBeforeUnmount } from "vue";
import {
  Package,
  Image as ImageIcon,
  Database,
  Trash2,
  Check,
  X,
  RotateCcw,
} from "lucide-vue-next";
import grass from "../../../app/src/assets/blocks/grass_block_side.png";
import moss from "../../../app/src/assets/blocks/mossy_cobblestone.png";

const props = defineProps({ copy: { type: Object, required: true } });
const itemIds = ["mod", "pack", "datapack"];
const itemIcons = { mod: Package, pack: ImageIcon, datapack: Database };
const targetIds = ["version", "world", "trash"];

const placed = ref({}); // item -> target
const held = ref(""); // item being dragged or picked
const over = ref(""); // target under the pointer
const flash = ref(""); // last completed drop, for the arrival pulse
const ghost = ref(null); // {x, y} while dragging
const targetEls = {};
let start = null;

function verdict(item, target) {
  if (!item || !target) return null;
  if (target === "trash") return { ok: true, text: props.copy.results["any:trash"] };
  const text = props.copy.results[`${item}:${target}`];
  const ok =
    (target === "version" && item !== "datapack") ||
    (target === "world" && item === "datapack");
  return { ok, text };
}
const preview = computed(() => verdict(held.value, over.value));
const status = computed(() => {
  if (preview.value) return preview.value;
  if (held.value) return { text: props.copy.pickHint };
  return { text: props.copy.idle };
});
const contents = (target) =>
  itemIds.filter((item) => placed.value[item] === target);

function drop(target) {
  const result = verdict(held.value, target);
  if (result?.ok) {
    placed.value = { ...placed.value, [held.value]: target };
    flash.value = target;
    setTimeout(() => flash.value === target && (flash.value = ""), 700);
  }
  held.value = "";
  over.value = "";
}

function targetAt(x, y) {
  return (
    targetIds.find((id) => {
      const rect = targetEls[id]?.getBoundingClientRect();
      return rect && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
    }) || ""
  );
}
function down(event, item) {
  if (event.button !== 0 || placed.value[item]) return;
  start = { x: event.clientX, y: event.clientY, item, moved: false, id: event.pointerId };
  try {
    event.currentTarget.setPointerCapture(event.pointerId);
  } catch {
    /* Synthetic or already-released pointers can't be captured. */
  }
}
function move(event) {
  if (!start || event.pointerId !== start.id) return;
  const dx = event.clientX - start.x;
  const dy = event.clientY - start.y;
  if (!start.moved && Math.hypot(dx, dy) < 5) return;
  start.moved = true;
  held.value = start.item;
  ghost.value = { x: event.clientX, y: event.clientY };
  over.value = targetAt(event.clientX, event.clientY);
}
function up(event, item) {
  if (!start || event.pointerId !== start.id) return;
  const wasDrag = start.moved;
  start = null;
  ghost.value = null;
  if (wasDrag) drop(targetAt(event.clientX, event.clientY));
  else held.value = held.value === item ? "" : item; // click to pick
}
function cancel() {
  start = null;
  ghost.value = null;
  held.value = "";
  over.value = "";
}
function reset() {
  placed.value = {};
  cancel();
}
function keyup(event) {
  if (event.key === "Escape") cancel();
}
window.addEventListener("keyup", keyup);
onBeforeUnmount(() => window.removeEventListener("keyup", keyup));
</script>

<template>
  <div class="drag-demo crate" :class="{ dragging: held }">
    <div class="drag-files" role="group" :aria-label="copy.hint">
      <button
        v-for="item in itemIds"
        :key="item"
        class="drag-file"
        :class="{ gone: placed[item], lifted: held === item }"
        :disabled="Boolean(placed[item])"
        :aria-pressed="held === item"
        @pointerdown="down($event, item)"
        @pointermove="move"
        @pointerup="up($event, item)"
        @pointercancel="cancel"
        @keydown.enter.prevent="held = held === item ? '' : item"
        @keydown.space.prevent="held = held === item ? '' : item"
      >
        <component :is="itemIcons[item]" :size="18" />
        <span
          >{{ copy.items[item] }}<small>{{ copy.kinds[item] }}</small></span
        >
      </button>
      <button v-if="Object.keys(placed).length" class="drag-reset" @click="reset">
        <RotateCcw :size="14" />{{ copy.reset }}
      </button>
    </div>

    <div class="drag-targets">
      <button
        v-for="target in targetIds"
        :key="target"
        :ref="(el) => (targetEls[target] = el)"
        class="drag-target"
        :class="[
          `is-${target}`,
          {
            hover: over === target,
            good: over === target && preview?.ok,
            bad: over === target && preview && !preview.ok,
            ready: held && !over,
            arrived: flash === target,
          },
        ]"
        :disabled="!held"
        @pointerenter="held && !ghost && (over = target)"
        @pointerleave="!ghost && over === target && (over = '')"
        @focus="held && (over = target)"
        @blur="over === target && (over = '')"
        @click="held && drop(target)"
      >
        <span class="target-head">
          <img v-if="target === 'version'" :src="grass" alt="" />
          <img v-else-if="target === 'world'" :src="moss" alt="" />
          <Trash2 v-else :size="20" />
          <span
            >{{ copy.targets[target] }}<small>{{ copy.targetKinds[target] }}</small></span
          >
        </span>
        <TransitionGroup
          v-if="target !== 'trash'"
          tag="span"
          name="chip"
          class="target-contents"
        >
          <span v-for="item in contents(target)" :key="item" class="chip">
            <Check :size="12" />{{ copy.items[item] }}
          </span>
        </TransitionGroup>
      </button>
    </div>

    <p class="drag-status" role="status" aria-live="polite">
      <template v-if="preview">
        <Check v-if="preview.ok" :size="15" /><X v-else :size="15" />
      </template>
      {{ status.text }}
    </p>

    <div
      v-if="ghost"
      class="drag-ghost"
      :class="{ good: preview?.ok, bad: preview && !preview.ok }"
      :style="{ translate: `${ghost.x}px ${ghost.y}px` }"
      aria-hidden="true"
    >
      <component :is="itemIcons[held]" :size="16" />{{ copy.items[held] }}
    </div>
  </div>
</template>
