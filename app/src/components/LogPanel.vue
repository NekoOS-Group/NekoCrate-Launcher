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
import {
  Terminal,
  Copy,
  Trash2,
  X,
  Search,
  ChevronRight,
  Coffee,
  MemoryStick,
  Package,
  Wrench,
} from "lucide-vue-next";
import { t, translateMessage, locale } from "../i18n";
import GlassSelect from "./GlassSelect.vue";
import {
  createLogPresenter,
  filterLogs,
  copyLogRows,
  unattributedLogSource,
} from "../logPresentation";
import {
  createLogOutline,
  visibleLogNodes,
  type LogNode,
} from "../logStructure";
import type { LogStore } from "../logStore";
import { LogViewport } from "../logViewport";
import { createViewportMotion } from "../viewportMotion";
import { listRevealStyle } from "../listMotion";
import {
  recoveryLabels,
  type LaunchRecovery,
  type RecoveryAction,
} from "../launchRecovery";

const props = defineProps<{
  store: LogStore;
  initialScope: string;
  session: number;
  recoveries?: readonly LaunchRecovery[];
  busy?: boolean;
}>();
const emit = defineEmits<{
  close: [];
  copy: [text: string];
  recover: [recovery: LaunchRecovery, action: RecoveryAction];
  clear: [scope: string];
}>();
const filter = reactive({
  scope: props.initialScope,
  source: "",
  level: "all",
  query: "",
});
const recovery = computed(() =>
  props.recoveries?.find(
    (item) => filter.scope === "all" || item.instanceId === filter.scope,
  ),
);
const recoveryIcons = {
  java: Coffee,
  memory: MemoryStick,
  mods: Package,
  dependencies: Wrench,
  repair: Wrench,
};
const repairActions = computed(
  () =>
    recovery.value?.actions.filter(
      (action) => action !== "settings" && action !== "report",
    ) || [],
);
const raw = shallowRef(props.store.snapshot());
const presenter = createLogPresenter();
const rows = computed(() =>
  presenter.update(raw.value, translateMessage, locale.value),
);
const filtered = computed(() => filterLogs(rows.value, filter));
const outline = createLogOutline();
const expanded = shallowRef(new Map<number, boolean>());
const outlineKey = computed(() => [
  filter.scope,
  filter.source,
  filter.level,
  filter.query,
]);
const tree = computed(() => outline.build(filtered.value, outlineKey.value));
const displayed = computed(() =>
  visibleLogNodes(tree.value, expanded.value, !!filter.query.trim()),
);
function isExpanded(node: LogNode) {
  return (
    expanded.value.get(node.id) ?? (!!filter.query.trim() || !node.collapsed)
  );
}
function toggle(node: LogNode) {
  viewport.following = false;
  expanded.value = new Map(expanded.value).set(node.id, !isExpanded(node));
  reveal(false);
}
const count = computed(() =>
  filtered.value.reduce((sum, row) => sum + row.count, 0),
);
const scopedCount = computed(() =>
  rows.value
    .filter((line) => filter.scope === "all" || line.scope === filter.scope)
    .reduce((sum, line) => sum + line.count, 0),
);
const senders = computed(() => {
  const names = new Set<string>();
  for (const line of rows.value)
    if (filter.scope === "all" || line.scope === filter.scope)
      names.add(line.filterSource || unattributedLogSource);
  return [
    { value: "", label: t("所有发送者") },
    ...Array.from(names)
      .sort((a, b) => a.localeCompare(b))
      .map((value) => ({
        value,
        label: value === unattributedLogSource ? t("未标注发送者") : value,
      })),
  ];
});
const levels = [
  { value: "all", label: "所有级别" },
  { value: "warning", label: "警告和错误" },
  { value: "error", label: "仅错误" },
  { value: "info", label: "INFO" },
  { value: "debug", label: "DEBUG / TRACE" },
];
const viewport = new LogViewport<LogNode>();
const content = ref<HTMLElement>();
const visible = shallowRef(viewport.window());
const motion = createViewportMotion(".log-row, .log-empty", "log-content");
let revealRevision = 0;
let revealFrame: number | undefined;
function reveal(leave = true) {
  const revision = ++revealRevision;
  if (revealFrame !== undefined) cancelAnimationFrame(revealFrame);
  if (leave && content.value) motion.leave(content.value);
  void nextTick(() => {
    if (
      revision !== revealRevision ||
      typeof requestAnimationFrame === "undefined"
    )
      return;
    revealFrame = requestAnimationFrame(() => {
      revealFrame = undefined;
      if (revision === revealRevision && content.value)
        motion.enter(content.value);
    });
  });
}
let programmaticTop: number | undefined;
function publish() {
  visible.value = viewport.window();
}
watch(
  visible,
  () => {
    if (content.value) {
      content.value.scrollTop = viewport.top;
      // The browser can clamp this before the first ResizeObserver callback.
      // That is a layout adjustment, not the reader leaving follow mode.
      programmaticTop = content.value.scrollTop;
    }
  },
  // A sync filter/stream watcher can run before Vue schedules its DOM update.
  // Apply scrolling after the spacer has its new height, never to the old one.
  { flush: "post" },
);
function scroll() {
  if (!content.value) return;
  const top = content.value.scrollTop;
  const applied = programmaticTop;
  programmaticTop = undefined;
  if (Math.abs(top - (applied ?? viewport.top)) < 1) return;
  motion.stop();
  revealRevision++;
  viewport.scroll(top);
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
  reveal();
  expanded.value = new Map();
  props.store.clear(filter.scope);
  emit("clear", filter.scope);
  refresh();
}
watch(
  [
    displayed,
    () => filter.scope,
    () => filter.source,
    () => filter.level,
    () => filter.query,
    expanded,
  ],
  ([next], previous) => {
    const reset =
      !previous.length ||
      filter.scope !== previous[1] ||
      filter.source !== previous[2] ||
      filter.level !== previous[3] ||
      filter.query !== previous[4];
    viewport.setRows(next, reset, expanded.value !== previous[5]);
    publish();
  },
  { immediate: true, flush: "sync" },
);
watch(
  () => filter.query,
  () => {
    expanded.value = new Map();
  },
  { flush: "sync" },
);
watch(
  [
    () => filter.scope,
    () => filter.source,
    () => filter.level,
    () => filter.query,
    () => props.session,
  ],
  () => reveal(),
  // Capture the old viewport before publish() applies the new scroll offset.
  { flush: "sync" },
);
watch(
  () => props.session,
  () => {
    Object.assign(filter, {
      scope: props.initialScope,
      source: "",
      level: "all",
      query: "",
    });
    expanded.value = new Map();
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
  void nextTick(() => {
    // GlassDialog opens after its children mount. Measure once it is visible.
    if (!content.value) return;
    if (content.value.clientWidth && content.value.clientHeight)
      viewport.resize(content.value.clientWidth, content.value.clientHeight);
    publish();
    reveal(false);
  });
});
onBeforeUnmount(() => {
  revealRevision++;
  if (revealFrame !== undefined) cancelAnimationFrame(revealFrame);
  motion.dispose();
  unsubscribe?.();
  observer?.disconnect();
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
          v-for="action in repairActions"
          :key="action"
          class="icon-button"
          :disabled="busy"
          :aria-label="t(recoveryLabels[action])"
          :title="t(recoveryLabels[action])"
          @click="recovery && emit('recover', recovery, action)"
        >
          <component :is="recoveryIcons[action]" :size="16" />
        </button>
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
      <div class="list-reveal-item" :style="listRevealStyle(0)">
        <label id="log-sender-label" class="sr-only">{{
          t("日志发送者")
        }}</label
        ><GlassSelect
          id="log-sender"
          v-model="filter.source"
          :options="senders"
          :aria-label="t('日志发送者')"
          :search-placeholder="t('搜索发送者')"
          :menu-min-width="480"
          searchable
          floating
          compact
          literal-labels
        />
      </div>
      <div class="list-reveal-item" :style="listRevealStyle(1)">
        <label id="log-level-label" class="sr-only">{{ t("日志级别") }}</label
        ><GlassSelect
          id="log-level"
          v-model="filter.level"
          :options="levels"
          floating
          compact
        />
      </div>
      <label class="log-search list-reveal-item" :style="listRevealStyle(2)"
        ><Search :size="14" aria-hidden="true" /><input
          v-model="filter.query"
          type="search"
          :aria-label="t('搜索日志')"
          :placeholder="t('搜索正文或来源')"
          spellcheck="false"
      /></label>
    </div>
    <div class="log-stage">
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
        <div
          v-else
          class="log-spacer"
          :style="{ height: `${visible.height}px` }"
        >
          <div
            class="log-window"
            :style="{ transform: `translateY(${visible.offset}px)` }"
          >
            <div
              v-for="(line, index) in visible.items"
              :key="line.id"
              v-log-row
              :data-log-id="line.id"
              role="listitem"
              :aria-posinset="visible.start + index + 1"
              :aria-setsize="displayed.length"
              class="log-row"
              :class="[
                line.row.level && `log-${line.row.level.toLowerCase()}`,
                line.row.detail && `log-${line.row.detail}`,
                { 'log-child': line.depth > 0 },
              ]"
              :style="{ '--log-depth': Math.min(line.depth, 24) }"
            >
              <time
                :title="
                  line.count > 1
                    ? `${line.time} – ${line.row.lastTime}`
                    : undefined
                "
                >{{ line.continuation ? "" : line.time }}</time
              >
              <span class="log-level">{{
                line.continuation ? "" : line.row.level
              }}</span>
              <div class="log-message">
                <button
                  v-if="line.children.length"
                  class="log-fold"
                  :aria-expanded="isExpanded(line)"
                  :aria-label="
                    t(isExpanded(line) ? '收起日志详情' : '展开日志详情')
                  "
                  :title="t(isExpanded(line) ? '收起日志详情' : '展开日志详情')"
                  @click="toggle(line)"
                >
                  <ChevronRight :size="13" />
                </button>
                <span
                  v-if="line.row.source && !line.continuation && !line.depth"
                  class="log-source"
                  >{{ line.row.source }} ·
                </span>
                <span class="log-body">{{ line.text }}</span
                ><span
                  v-if="line.count > 1"
                  class="log-repeat"
                  :title="
                    t(
                      line.row.variants
                        ? '相似日志 {count} 条'
                        : '重复 {count} 次',
                      { count: line.count },
                    )
                  "
                  >×{{ line.count }}</span
                ><span
                  v-if="
                    line.descendants && !line.row.variants && !isExpanded(line)
                  "
                  class="log-fold-count"
                  >{{ t("{count} 条", { count: line.descendants }) }}</span
                >
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
