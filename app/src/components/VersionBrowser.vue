<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
  watch,
} from "vue";
import { Box, LoaderCircle, RefreshCw, Search, X } from "lucide-vue-next";
import VersionCube from "./VersionCube.vue";
import {
  filterVersions,
  versionCategories,
  type VersionCategory,
} from "../versionCatalog";
import { t } from "../i18n";
import type { VersionManifest } from "../types";

const props = defineProps<{
  modelValue: string;
  manifest: VersionManifest | null;
  loading: boolean;
  disabled: boolean;
}>();
const emit = defineEmits<{ "update:modelValue": [id: string]; refresh: [] }>();
const category = ref<VersionCategory>("release");
const query = ref("");
const filtered = computed(() =>
  filterVersions(props.manifest?.versions || [], category.value, query.value),
);
const viewport = ref<HTMLElement>();
const scrollTop = ref(0);
const height = ref(300);
const active = ref(0);
const uid = useId();
const rowHeight = 54;
const columns = 2;
const start = computed(
  () => Math.max(0, Math.floor(scrollTop.value / rowHeight) - 2) * columns,
);
const end = computed(() =>
  Math.min(
    filtered.value.length,
    (Math.ceil((scrollTop.value + height.value) / rowHeight) + 2) * columns,
  ),
);
const visible = computed(() => filtered.value.slice(start.value, end.value));
const totalHeight = computed(
  () => Math.ceil(filtered.value.length / columns) * rowHeight,
);
const activeId = computed(() =>
  active.value >= start.value && active.value < end.value
    ? `${uid}-version-${active.value}`
    : undefined,
);
let observer: ResizeObserver | undefined;

function reveal(index: number) {
  const element = viewport.value;
  if (!element) return;
  const top = Math.floor(index / columns) * rowHeight;
  if (top < element.scrollTop) element.scrollTop = top;
  else if (top + rowHeight > element.scrollTop + element.clientHeight)
    element.scrollTop = top + rowHeight - element.clientHeight;
  scrollTop.value = element.scrollTop;
}
watch(
  filtered,
  async (entries) => {
    active.value = Math.max(
      0,
      entries.findIndex((entry) => entry.id === props.modelValue),
    );
    scrollTop.value = 0;
    if (viewport.value) viewport.value.scrollTop = 0;
    await nextTick();
    reveal(active.value);
  },
  { immediate: true },
);

function choose(index: number) {
  if (props.disabled || !filtered.value[index]) return;
  active.value = index;
  emit("update:modelValue", filtered.value[index].id);
}
function navigate(event: KeyboardEvent) {
  if (!filtered.value.length || props.disabled) return;
  let index = active.value;
  switch (event.key) {
    case "ArrowDown":
      index += columns;
      break;
    case "ArrowUp":
      index -= columns;
      break;
    case "ArrowRight":
      index++;
      break;
    case "ArrowLeft":
      index--;
      break;
    case "Home":
      index = 0;
      break;
    case "End":
      index = filtered.value.length - 1;
      break;
    case "PageDown":
      index += Math.max(1, Math.floor(height.value / rowHeight)) * columns;
      break;
    case "PageUp":
      index -= Math.max(1, Math.floor(height.value / rowHeight)) * columns;
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      choose(index);
      return;
    default:
      return;
  }
  event.preventDefault();
  active.value = Math.max(0, Math.min(filtered.value.length - 1, index));
  reveal(active.value);
}
onMounted(() => {
  if (!viewport.value) return;
  observer = new ResizeObserver(() => {
    height.value = viewport.value?.clientHeight || 300;
  });
  observer.observe(viewport.value);
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <div class="version-browser">
    <div class="catalog-filter" role="group" :aria-label="t('版本分类')">
      <button
        v-for="item in versionCategories"
        :key="item.id"
        type="button"
        :class="{ active: category === item.id }"
        :style="{ '--category-accent': item.color }"
        :aria-label="t(item.label)"
        :aria-pressed="category === item.id"
        @click="category = item.id"
      >
        <VersionCube :variant="item.id" />
        <span>{{ t(item.label) }}</span>
      </button>
    </div>
    <div class="catalog-toolbar">
      <div class="catalog-search">
        <Search :size="15" aria-hidden="true" />
        <input
          v-model="query"
          type="search"
          :aria-label="t('搜索当前分类的版本')"
          :placeholder="t('搜索版本…')"
          spellcheck="false"
          autocomplete="off"
          @keydown.enter.prevent
        />
        <button
          v-if="query"
          type="button"
          class="icon-button"
          :aria-label="t('清除搜索')"
          @click="query = ''"
        >
          <X :size="13" />
        </button>
      </div>
      <button
        type="button"
        class="icon-button catalog-refresh"
        :disabled="loading || disabled"
        :aria-label="t('更新版本列表')"
        @click="$emit('refresh')"
      >
        <RefreshCw :size="15" :class="{ spin: loading }" />
      </button>
    </div>
    <div class="catalog-results">
      <div
        ref="viewport"
        class="catalog-viewport"
        role="listbox"
        :aria-label="t('Minecraft 版本')"
        :aria-busy="loading"
        :aria-disabled="disabled"
        :aria-activedescendant="activeId"
        tabindex="0"
        @scroll="scrollTop = ($event.target as HTMLElement).scrollTop"
        @keydown="navigate"
      >
        <div class="catalog-spacer" :style="{ height: `${totalHeight}px` }">
          <div
            class="catalog-grid"
            :style="{
              transform: `translateY(${Math.floor(start / columns) * rowHeight}px)`,
            }"
          >
            <div
              v-for="(version, index) in visible"
              :id="`${uid}-version-${start + index}`"
              :key="version.id"
              class="catalog-version"
              :class="{
                selected: modelValue === version.id,
                focused: active === start + index,
              }"
              role="option"
              :aria-selected="modelValue === version.id"
              :aria-posinset="start + index + 1"
              :aria-setsize="filtered.length"
              :title="version.id"
              @click="choose(start + index)"
            >
              <Box :size="18" :stroke-width="1.5" aria-hidden="true" />
              <span>{{ version.id }}</span>
              <small
                v-if="
                  version.id === manifest?.latest.release ||
                  version.id === manifest?.latest.snapshot
                "
                >{{ t("最新") }}</small
              >
            </div>
          </div>
        </div>
      </div>
      <div v-if="!filtered.length" class="catalog-empty" role="status">
        <LoaderCircle v-if="loading" :size="22" class="spin" />
        <Search v-else-if="query" :size="24" :stroke-width="1" />
        <Box v-else :size="24" :stroke-width="1" />
        <span>{{
          loading
            ? t("正在获取版本列表…")
            : !manifest
              ? t("暂未获取版本列表")
              : t("没有匹配的版本")
        }}</span>
        <button
          v-if="!loading && !manifest"
          type="button"
          class="text-button"
          :disabled="disabled"
          @click="$emit('refresh')"
        >
          {{ t("重试") }}
        </button>
      </div>
    </div>
    <span class="catalog-count">{{
      t("{count} 个版本", { count: filtered.length })
    }}</span>
  </div>
</template>

<style scoped>
.version-browser {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  padding: 18px 24px 16px;
}
.catalog-filter {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 6px;
  flex-shrink: 0;
}
.catalog-filter button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 44px;
  min-width: 0;
  padding: 8px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
  transition:
    background-color 200ms var(--motion-ease),
    box-shadow 200ms var(--motion-ease),
    transform 180ms var(--motion-ease),
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.catalog-filter button:hover,
.catalog-filter button:focus-visible {
  color: var(--category-accent);
  background: color-mix(in srgb, var(--category-accent) 8%, transparent);
  outline: none;
}
.catalog-filter button.active {
  color: var(--category-accent);
  background: color-mix(in srgb, var(--category-accent) 18%, transparent);
  box-shadow:
    inset 0 0 0 1px color-mix(in srgb, var(--category-accent) 25%, transparent),
    0 0 16px color-mix(in srgb, var(--category-accent) 8%, transparent);
  text-shadow: 0 0 12px
    color-mix(in srgb, var(--category-accent) 24%, transparent);
}
.catalog-filter button:focus-visible {
  box-shadow: inset 0 0 0 1px var(--category-accent);
}
.catalog-filter button:active {
  transform: translateY(1px);
}
.catalog-filter .version-cube {
  color: var(--category-accent);
}
@media (max-width: 820px) {
  .catalog-filter button {
    flex-direction: column;
    gap: 4px;
    padding: 8px 4px;
  }
}
.catalog-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 12px 0 14px;
  flex-shrink: 0;
}
.catalog-search {
  display: flex;
  align-items: center;
  gap: 9px;
  flex: 1;
  min-width: 0;
  height: 36px;
  border-bottom: 1px solid var(--line);
  color: var(--muted);
}
.catalog-search:focus-within {
  border-color: var(--blue);
}
.catalog-search input {
  width: 100%;
  min-width: 0;
  min-height: 0;
  height: 34px;
  padding: 0;
  border: 0;
  background: transparent;
  box-shadow: none;
  outline: none;
  font-size: 12px;
}
.catalog-search input::-webkit-search-cancel-button {
  display: none;
}
.catalog-search .icon-button {
  width: 24px;
  height: 28px;
  flex-shrink: 0;
}
.catalog-refresh {
  width: 30px;
  height: 32px;
  flex-shrink: 0;
}
.catalog-results {
  position: relative;
  flex: 1;
  min-height: 0;
}
.catalog-viewport {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  outline: none;
  scrollbar-gutter: stable;
}
.catalog-spacer {
  position: relative;
}
.catalog-grid {
  position: absolute;
  inset: 0 0 auto;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  column-gap: 16px;
}
.catalog-version {
  display: flex;
  align-items: center;
  gap: 9px;
  height: 54px;
  min-width: 0;
  padding: 0 2px;
  color: var(--muted);
  cursor: pointer;
  transition:
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.catalog-version > svg {
  flex-shrink: 0;
  transition:
    fill 180ms var(--motion-ease),
    filter 180ms var(--motion-ease);
}
.catalog-version > span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font: 12px/20px var(--mono);
}
.catalog-version > small {
  margin-left: auto;
  font-size: 9px;
  white-space: nowrap;
  opacity: 0.75;
}
.catalog-version:hover,
.catalog-version.selected,
.catalog-viewport:focus-visible .catalog-version.focused {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.catalog-version.selected > svg {
  fill: var(--blue);
  filter: drop-shadow(0 0 5px var(--glow));
}
.catalog-viewport:focus-visible .catalog-version.focused > span {
  text-decoration: underline;
  text-underline-offset: 5px;
}
.catalog-viewport[aria-disabled="true"] .catalog-version {
  cursor: default;
}
.catalog-empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--muted);
  font-size: 12px;
  pointer-events: none;
}
.catalog-empty button {
  pointer-events: auto;
}
.catalog-count {
  padding-top: 12px;
  font: 10px/16px var(--mono);
  color: var(--muted);
  opacity: 0.8;
}
@media (prefers-reduced-motion: reduce) {
  .catalog-version,
  .catalog-version > svg,
  .catalog-filter button {
    transition: none;
  }
}
</style>
