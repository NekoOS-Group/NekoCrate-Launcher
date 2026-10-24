<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onBeforeUnmount,
  reactive,
  ref,
  shallowRef,
  watch,
} from "vue";
import { Terminal, Copy, Trash2, X, Search, ArrowDown } from "lucide-vue-next";
import { t, translateMessage, locale } from "../i18n";
import GlassSelect from "./GlassSelect.vue";
import {
  createLogPresenter,
  filterLogs,
  copyLogRows,
} from "../logPresentation";
import type { LogStore } from "../logStore";
import { LogViewport } from "../logViewport";

const props = defineProps<{
  store: LogStore;
  instances: readonly { id: string; name: string }[];
  initialScope: string;
  session: number;
}>();
const emit = defineEmits<{ close: []; copy: [text: string] }>();
const filter = reactive({ scope: props.initialScope, level: "all", query: "" });
const raw = shallowRef(props.store.snapshot());
const presenter = createLogPresenter();
const rows = computed(() =>
  presenter.update(raw.value, translateMessage, locale.value),
);
const filtered = computed(() => filterLogs(rows.value, filter));
const count = computed(() =>
  filtered.value.reduce((sum, row) => sum + row.count, 0),
);
const scopedCount = computed(
  () =>
    raw.value.filter(
      (line) =>
        filter.scope === "all" ||
        (line.instanceId || "launcher") === filter.scope,
    ).length,
);
const scopes = computed(() => {
  const names = new Map(
    props.instances.map((instance) => [instance.id, instance.name]),
  );
  for (const line of raw.value)
    if (line.instanceId && !names.has(line.instanceId))
      names.set(
        line.instanceId,
        `${t("已移除实例")} · ${line.instanceId.slice(0, 8)}`,
      );
  return [
    { value: "all", label: t("全部实例") },
    { value: "launcher", label: t("启动器") },
    ...Array.from(names, ([value, label]) => ({ value, label })),
  ];
});
const levels = [
  { value: "all", label: "所有级别" },
  { value: "warning", label: "警告和错误" },
  { value: "error", label: "仅错误" },
  { value: "info", label: "INFO" },
  { value: "debug", label: "DEBUG / TRACE" },
];
const viewport = new LogViewport();
const content = ref<HTMLElement>();
const visible = shallowRef(viewport.window());
let scrollRevision = 0;
function publish() {
  visible.value = viewport.window();
  const revision = ++scrollRevision;
  void nextTick(() => {
    if (revision === scrollRevision && content.value)
      content.value.scrollTop = viewport.top;
  });
}
function scroll() {
  if (!content.value || Math.abs(content.value.scrollTop - viewport.top) < 1)
    return;
  scrollRevision++;
  viewport.scroll(content.value.scrollTop);
  visible.value = viewport.window();
}
function latest() {
  viewport.latest();
  publish();
}
function refresh() {
  raw.value = props.store.snapshot();
}
function clear() {
  props.store.clear(filter.scope);
  refresh();
}
watch(
  [filtered, () => filter.scope, () => filter.level, () => filter.query],
  ([next], previous) => {
    const reset =
      !previous.length ||
      filter.scope !== previous[1] ||
      filter.level !== previous[2] ||
      filter.query !== previous[3];
    viewport.setRows(next, reset);
    publish();
  },
  { immediate: true, flush: "sync" },
);
watch(
  () => props.session,
  () => {
    Object.assign(filter, {
      scope: props.initialScope,
      level: "all",
      query: "",
    });
    refresh();
    latest();
  },
);
let unsubscribe: (() => void) | undefined;
let observer: ResizeObserver | undefined;
const vLogRow = {
  mounted(element: HTMLElement) {
    observer?.observe(element);
  },
  unmounted(element: HTMLElement) {
    observer?.unobserve(element);
  },
};
onMounted(() => {
  unsubscribe = props.store.subscribe(refresh);
  observer = new ResizeObserver((entries) => {
    let changed = false;
    const measured = [];
    for (const entry of entries) {
      if (entry.target === content.value)
        changed =
          viewport.resize(entry.contentRect.width, entry.contentRect.height) ||
          changed;
      else
        measured.push({
          id: Number((entry.target as HTMLElement).dataset.logId),
          height:
            entry.borderBoxSize?.[0]?.blockSize ||
            (entry.target as HTMLElement).offsetHeight,
        });
    }
    changed = viewport.measure(measured) || changed;
    if (changed) publish();
  });
  if (content.value) {
    observer.observe(content.value);
    for (const row of content.value.querySelectorAll("[data-log-id]"))
      observer.observe(row);
  }
});
onBeforeUnmount(() => {
  unsubscribe?.();
  observer?.disconnect();
  scrollRevision++;
});
</script>

<template>
  <section class="log-panel glass-surface" :aria-label="t('运行日志')">
    <header>
      <h2 class="dialog-heading" tabindex="-1" autofocus>
        <Terminal :size="16" />{{ t("运行日志") }}
        <small>{{ count }} / {{ scopedCount }}</small>
      </h2>
      <div class="inline-actions">
        <button
          class="icon-button"
          :disabled="!count"
          :aria-label="t('复制筛选后的日志')"
          :title="t('复制筛选后的日志')"
          @click="emit('copy', copyLogRows(filtered))"
        >
          <Copy :size="16" />
        </button>
        <button
          class="icon-button"
          :disabled="!scopedCount"
          :aria-label="
            t(filter.scope === 'all' ? '清空日志' : '清空此实例日志')
          "
          @click="clear"
        >
          <Trash2 :size="16" />
        </button>
        <button
          class="icon-button"
          :aria-label="t('关闭日志')"
          @click="emit('close')"
        >
          <X :size="16" />
        </button>
      </div>
    </header>
    <div class="log-filters">
      <div>
        <label id="log-scope-label" class="sr-only">{{ t("日志来源") }}</label
        ><GlassSelect
          id="log-scope"
          v-model="filter.scope"
          :options="scopes"
          floating
          compact
          literal-labels
        />
      </div>
      <div>
        <label id="log-level-label" class="sr-only">{{ t("日志级别") }}</label
        ><GlassSelect
          id="log-level"
          v-model="filter.level"
          :options="levels"
          floating
          compact
        />
      </div>
      <label class="log-search"
        ><Search :size="14" aria-hidden="true" /><input
          v-model="filter.query"
          type="search"
          :aria-label="t('搜索日志')"
          :placeholder="t('搜索正文或来源')"
          spellcheck="false"
      /></label>
    </div>
    <div
      ref="content"
      class="log-content"
      tabindex="0"
      role="list"
      :aria-label="t('运行日志')"
      @scroll.passive="scroll"
    >
      <p v-if="!filtered.length" class="log-empty muted">
        {{ t(raw.length ? "没有符合筛选条件的日志。" : "暂无运行日志。") }}
      </p>
      <div v-else class="log-spacer" :style="{ height: `${visible.height}px` }">
        <div
          class="log-window"
          :style="{ transform: `translateY(${visible.offset}px)` }"
        >
          <p
            v-for="(line, index) in visible.items"
            :key="line.id"
            v-log-row
            :data-log-id="line.id"
            role="listitem"
            :aria-posinset="visible.start + index + 1"
            :aria-setsize="filtered.length"
            class="log-row"
            :class="[
              line.level && `log-${line.level.toLowerCase()}`,
              line.detail && `log-${line.detail}`,
            ]"
          >
            <time
              :title="
                line.count > 1 ? `${line.time} – ${line.lastTime}` : undefined
              "
              >{{ line.time }}</time
            >
            <span class="log-level">{{ line.level }}</span>
            <span class="log-message"
              ><span v-if="line.source" class="log-source">{{
                line.source
              }}</span
              ><span class="log-body">{{ line.message }}</span
              ><span
                v-if="line.count > 1"
                class="log-repeat"
                :title="t('重复 {count} 次', { count: line.count })"
                >×{{ line.count }}</span
              ></span
            >
          </p>
        </div>
      </div>
    </div>
    <button
      v-if="!visible.following"
      class="log-latest text-button"
      @click="latest"
    >
      <ArrowDown :size="14" />{{ t("回到最新日志") }}
    </button>
  </section>
</template>
