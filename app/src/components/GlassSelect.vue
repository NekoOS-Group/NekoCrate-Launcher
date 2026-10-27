<script setup lang="ts">
import { t } from "../i18n";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { Check, ChevronDown, FolderOpen } from "lucide-vue-next";
import { floatingMenuPosition } from "../floatingMenu";

const props = defineProps<{
  id: string;
  modelValue: string;
  disabled?: boolean;
  options: readonly { value: string; label: string; detail?: string }[];
  actionLabel?: string;
  floating?: boolean;
  compact?: boolean;
  literalLabels?: boolean;
  editable?: boolean;
  editableValue?: number;
  inputLabel?: string;
  inputMin?: number;
  inputMax?: number;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  action: [];
  "update:editableValue": [value: number];
}>();
const selected = computed(
  () =>
    props.options.find((source) => source.value === props.modelValue) ||
    props.options[0],
);
const label = (text: string) => (props.literalLabels ? text : t(text));
const root = ref<HTMLElement>();
const trigger = ref<HTMLButtonElement>();
const input = ref<HTMLInputElement>();
const list = ref<HTMLElement>();
const expanded = ref(false);
const availableHeight = ref<number>();
const portalTarget = ref<HTMLElement>();
const floatingStyle = ref<ReturnType<typeof floatingMenuPosition>>();
let trackingFrame: number | undefined;
function fitMenu() {
  if (!expanded.value || !trigger.value) return;
  if (props.floating) {
    const position = floatingMenuPosition(
      trigger.value.getBoundingClientRect(),
      {
        width: window.innerWidth,
        height: window.innerHeight,
      },
    );
    if (JSON.stringify(position) !== JSON.stringify(floatingStyle.value))
      floatingStyle.value = position;
    return;
  }
  availableHeight.value = Math.max(
    80,
    window.innerHeight - trigger.value.getBoundingClientRect().bottom - 24,
  );
}
function trackMenu() {
  trackingFrame = undefined;
  if (!expanded.value || !props.floating) return;
  fitMenu();
  // Follow the animated panel and scrolling while open; no idle render loop.
  trackingFrame = requestAnimationFrame(trackMenu);
}

function optionButtons() {
  return Array.from(
    list.value?.querySelectorAll<HTMLButtonElement>(
      "[role='option'], [data-select-action]",
    ) || [],
  );
}
function focusOption(index: number) {
  const items = optionButtons();
  const item = items[(index + items.length) % items.length];
  item?.focus({ preventScroll: true });
  item?.scrollIntoView({ block: "nearest", inline: "nearest" });
}
async function open(position: "selected" | "first" | "last" = "selected") {
  if (props.disabled) return;
  if (props.floating)
    portalTarget.value = root.value?.closest("dialog") || document.body;
  expanded.value = true;
  fitMenu();
  if (props.floating) trackMenu();
  await nextTick();
  if (expanded.value)
    focusOption(
      position === "first"
        ? 0
        : position === "last"
          ? optionButtons().length - 1
          : props.options.indexOf(selected.value),
    );
}
function close(restoreFocus = false) {
  expanded.value = false;
  if (trackingFrame !== undefined) cancelAnimationFrame(trackingFrame);
  trackingFrame = undefined;
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}
function choose(value: string) {
  emit("update:modelValue", value);
  close(true);
}
function runAction() {
  close(true);
  emit("action");
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
  const index = optionButtons().indexOf(
    document.activeElement as HTMLButtonElement,
  );
  focusOption(
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? optionButtons().length - 1
        : index + (event.key === "ArrowDown" ? 1 : -1),
  );
}
function outside(event: PointerEvent) {
  if (
    event.target instanceof Node &&
    !root.value?.contains(event.target) &&
    !list.value?.contains(event.target)
  )
    close();
}
function leave(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    (!root.value?.contains(event.relatedTarget) &&
      !list.value?.contains(event.relatedTarget))
  )
    close();
}
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) close();
  },
);
watch(
  () => props.editable,
  async (editable) => {
    if (editable) {
      await nextTick();
      input.value?.focus({ preventScroll: true });
    }
  },
);
onMounted(() => {
  document.addEventListener("pointerdown", outside, true);
  window.addEventListener("resize", fitMenu);
});
onBeforeUnmount(() => {
  close();
  document.removeEventListener("pointerdown", outside, true);
  window.removeEventListener("resize", fitMenu);
});
</script>

<template>
  <div
    ref="root"
    class="source-select"
    :class="{ 'source-select-editable': editable }"
    @focusout="leave"
    @keydown="navigate"
  >
    <input
      v-if="editable"
      ref="input"
      class="source-select-input"
      type="number"
      :value="Number.isFinite(editableValue) ? editableValue : ''"
      :min="inputMin"
      :max="inputMax"
      step="1"
      required
      :disabled="disabled"
      :aria-label="inputLabel"
      @keydown.stop
      @input="
        emit(
          'update:editableValue',
          ($event.target as HTMLInputElement).value === ''
            ? NaN
            : Number(($event.target as HTMLInputElement).value),
        )
      "
    />
    <button
      :id="`${id}-trigger`"
      ref="trigger"
      type="button"
      class="source-select-trigger"
      :disabled="disabled"
      :aria-labelledby="`${id}-label ${id}-value`"
      aria-haspopup="listbox"
      :aria-controls="`${id}-options`"
      :aria-expanded="expanded"
      :title="selected.detail ? t(selected.detail) : label(selected.label)"
      @click="expanded ? close() : open()"
    >
      <span :id="`${id}-value`" class="selection-label">
        <Transition
          name="selection-text"
          @before-leave="
            (element) => element.setAttribute('aria-hidden', 'true')
          "
          @leave-cancelled="(element) => element.removeAttribute('aria-hidden')"
        >
          <span :key="selected.value">{{ label(selected.label) }}</span>
        </Transition>
      </span>
      <ChevronDown :size="16" />
    </button>
    <Teleport
      :to="portalTarget || 'body'"
      :disabled="!floating || !portalTarget"
    >
      <Transition name="source-menu">
        <div
          v-if="expanded"
          ref="list"
          class="source-select-menu glass-surface"
          :class="{
            'source-select-floating': floating,
            'source-select-compact': compact,
          }"
          @keydown.stop="navigate"
          @focusout.stop="leave"
          :style="
            floating
              ? floatingStyle
              : availableHeight === undefined
                ? undefined
                : { maxHeight: `${availableHeight}px` }
          "
        >
          <div
            :id="`${id}-options`"
            class="source-select-options"
            role="listbox"
            :aria-labelledby="`${id}-label`"
          >
            <button
              v-for="source in options"
              :key="source.value"
              type="button"
              class="source-select-option"
              role="option"
              :aria-selected="source.value === modelValue"
              :title="source.detail ? t(source.detail) : label(source.label)"
              tabindex="-1"
              :disabled="disabled"
              @pointerdown.prevent
              @click="choose(source.value)"
            >
              <span class="source-select-copy"
                ><span>{{ label(source.label) }}</span
                ><small v-if="source.detail">{{
                  t(source.detail)
                }}</small></span
              >
              <Check v-if="source.value === modelValue" :size="16" />
            </button>
          </div>
          <button
            v-if="actionLabel"
            type="button"
            class="source-select-action"
            data-select-action
            :disabled="disabled"
            tabindex="-1"
            @pointerdown.prevent
            @click="runAction"
          >
            <FolderOpen :size="15" />{{ actionLabel }}
          </button>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
