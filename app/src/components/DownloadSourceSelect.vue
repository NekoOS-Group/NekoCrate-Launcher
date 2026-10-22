<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { Check, ChevronDown } from "lucide-vue-next";

const props = defineProps<{ modelValue: string; disabled: boolean }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const sources = [
  { value: "official", label: "Minecraft 官方", detail: "仅使用官方来源" },
  { value: "auto", label: "官方优先", detail: "失败时切换至 BMCLAPI" },
  { value: "bmclapi", label: "BMCLAPI 优先", detail: "失败时切换至官方来源" },
];
const selected = computed(
  () =>
    sources.find((source) => source.value === props.modelValue) || sources[0],
);
const root = ref<HTMLElement>();
const trigger = ref<HTMLButtonElement>();
const list = ref<HTMLElement>();
const expanded = ref(false);

function options() {
  return Array.from(
    list.value?.querySelectorAll<HTMLButtonElement>("[role='option']") || [],
  );
}
function focusOption(index: number) {
  const items = options();
  items[(index + items.length) % items.length]?.focus({ preventScroll: true });
}
async function open(position: "selected" | "first" | "last" = "selected") {
  if (props.disabled) return;
  expanded.value = true;
  await nextTick();
  if (expanded.value)
    focusOption(
      position === "first"
        ? 0
        : position === "last"
          ? sources.length - 1
          : sources.indexOf(selected.value),
    );
}
function close(restoreFocus = false) {
  expanded.value = false;
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}
function choose(value: string) {
  emit("update:modelValue", value);
  close(true);
}
function navigate(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (expanded.value) {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    }
    return;
  }
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  if (!expanded.value) {
    void open(
      event.key === "Home"
        ? "first"
        : event.key === "End"
          ? "last"
          : "selected",
    );
    return;
  }
  const index = options().indexOf(document.activeElement as HTMLButtonElement);
  focusOption(
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? sources.length - 1
        : index + (event.key === "ArrowDown" ? 1 : -1),
  );
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
  <div ref="root" class="source-select" @focusout="leave" @keydown="navigate">
    <button
      id="download-source-trigger"
      ref="trigger"
      type="button"
      class="source-select-trigger"
      :disabled="disabled"
      aria-labelledby="download-source-label download-source-value"
      aria-haspopup="listbox"
      aria-controls="download-source-options"
      :aria-expanded="expanded"
      @click="expanded ? close() : open()"
    >
      <span id="download-source-value">{{ selected.label }}</span>
      <ChevronDown :size="16" />
    </button>
    <Transition name="source-menu">
      <div
        v-if="expanded"
        id="download-source-options"
        ref="list"
        class="source-select-menu glass-surface"
        role="listbox"
        aria-labelledby="download-source-label"
      >
        <button
          v-for="source in sources"
          :key="source.value"
          type="button"
          class="source-select-option"
          role="option"
          :aria-selected="source.value === modelValue"
          tabindex="-1"
          :disabled="disabled"
          @click="choose(source.value)"
        >
          <span class="source-select-copy"
            ><span>{{ source.label }}</span
            ><small>{{ source.detail }}</small></span
          >
          <Check v-if="source.value === modelValue" :size="16" />
        </button>
      </div>
    </Transition>
  </div>
</template>
