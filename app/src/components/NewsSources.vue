<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { Check, Plus, Rss, X } from "lucide-vue-next";
import { t } from "../i18n";
import { presetFeeds, safeFeedUrl, type FeedSource } from "../homeWidgets";

const props = defineProps<{ sources: FeedSource[] }>();
const emit = defineEmits<{ change: [sources: FeedSource[]] }>();
const host = ref<HTMLElement>();
const open = ref(false);
const sourceUrl = ref("");
const error = ref("");
const customSources = computed(() =>
  props.sources.filter(
    (source) => !presetFeeds.some((preset) => preset.url === source.url),
  ),
);
function toggle(source: FeedSource) {
  error.value = "";
  if (props.sources.some((item) => item.url === source.url))
    emit(
      "change",
      props.sources.filter((item) => item.url !== source.url),
    );
  else if (props.sources.length >= 8) error.value = "最多添加 8 个订阅源";
  else emit("change", [...props.sources, source]);
}
function add() {
  const url = safeFeedUrl(sourceUrl.value);
  if (!url) {
    error.value = "请输入有效的 HTTP 或 HTTPS 订阅地址";
    return;
  }
  if (props.sources.some((source) => source.url === url)) {
    error.value = "已订阅此地址";
    return;
  }
  if (props.sources.length >= 8) {
    error.value = "最多添加 8 个订阅源";
    return;
  }
  error.value = "";
  emit("change", [...props.sources, { url, name: new URL(url).hostname }]);
  sourceUrl.value = "";
}
function outside(event: PointerEvent) {
  if (event.target instanceof Node && !host.value?.contains(event.target))
    open.value = false;
}
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", outside, true),
);
</script>

<template>
  <div
    ref="host"
    class="layout-news-sources"
    @pointerdown.stop
    @keydown.stop
    @keydown.esc="open = false"
  >
    <button
      type="button"
      class="layout-orientation"
      :aria-label="t('订阅来源')"
      :title="t('订阅来源')"
      :aria-expanded="open"
      @click="open = !open"
    >
      <Rss :size="14" />
    </button>
    <Transition name="news-sources">
      <div v-if="open" class="layout-news-source-menu glass-surface">
        <button
          v-for="source in presetFeeds"
          :key="source.url"
          type="button"
          class="news-source-row"
          :class="{ selected: sources.some((item) => item.url === source.url) }"
          :aria-pressed="sources.some((item) => item.url === source.url)"
          @click="toggle(source)"
        >
          <Rss :size="14" /><span>{{ t(source.name) }}</span
          ><Check
            v-if="sources.some((item) => item.url === source.url)"
            :size="14"
          />
        </button>
        <div
          v-for="source in customSources"
          :key="source.url"
          class="news-source-row selected"
          :title="source.url"
        >
          <Rss :size="14" /><span>{{ source.name }}</span
          ><button
            type="button"
            class="icon-button"
            :aria-label="t('移除 {name}', { name: source.name })"
            @click="toggle(source)"
          >
            <X :size="13" />
          </button>
        </div>
        <form class="news-source-form" @submit.prevent="add">
          <input
            v-model="sourceUrl"
            type="url"
            required
            :placeholder="t('RSS / Atom 订阅地址')"
            :aria-label="t('RSS / Atom 订阅地址')"
            maxlength="2048"
          />
          <button class="icon-button" type="submit" :aria-label="t('添加订阅')">
            <Plus :size="15" />
          </button>
        </form>
        <small v-if="error" class="news-source-error">{{ t(error) }}</small>
      </div>
    </Transition>
  </div>
</template>

<style>
.layout-news-sources {
  display: flex;
}
.layout-news-source-menu.glass-surface {
  position: absolute;
  z-index: 4;
  top: calc(100% + 4px);
  left: 0;
  width: 280px;
  max-height: min(340px, 60vh);
  overflow-y: auto;
  padding: 6px;
  cursor: default;
  --glass-fill: color-mix(in srgb, var(--surface) 95%, transparent);
}
.home-layout-item[data-horizontal="right"] .layout-news-source-menu {
  left: auto;
  right: 0;
}
.home-layout-item[data-vertical="bottom"] .layout-news-source-menu {
  top: auto;
  bottom: calc(100% + 4px);
}
.news-source-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 5px 6px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 11px;
  text-align: left;
}
.news-source-row > span {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}
.news-source-row.selected,
button.news-source-row:is(:hover, :focus-visible) {
  color: var(--blue);
  outline: none;
}
.layout-news-source-menu .icon-button {
  width: 24px;
  height: 24px;
  flex-shrink: 0;
}
.news-source-form {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 2px 0;
}
.news-source-form input {
  width: 0;
  flex: 1;
  font-size: 11px;
  height: 28px;
}
.news-source-error {
  display: block;
  white-space: normal;
  margin: 6px;
  color: var(--muted);
  font-size: 10px;
}
.news-sources-enter-active,
.news-sources-leave-active {
  transition:
    opacity 120ms,
    translate 160ms var(--motion-ease);
}
.news-sources-enter-from,
.news-sources-leave-to {
  opacity: 0;
  translate: 0 -4px;
}
@media (prefers-reduced-motion: reduce) {
  .news-sources-enter-active,
  .news-sources-leave-active {
    transition: none;
  }
}
</style>
