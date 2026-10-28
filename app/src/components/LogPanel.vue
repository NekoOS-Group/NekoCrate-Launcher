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
import type { CrashReport } from "../types";
import type { LogStore } from "../logStore";
import { LogViewport } from "../logViewport";
import { createViewportMotion } from "../viewportMotion";
import { listRevealStyle } from "../listMotion";

const props = defineProps<{
  store: LogStore;
  loadDiagnosis?: (id: string) => Promise<CrashReport | null>;
  diagnosisRevision?: number;
  instances: readonly { id: string; name: string }[];
  initialScope: string;
  session: number;
}>();
const emit = defineEmits<{ close: []; copy: [text: string] }>();
const filter = reactive({ scope: props.initialScope, level: "all", query: "" });
const diagnosis = ref<CrashReport | null>(null);
let diagnosisRequest = 0;
watch(
  [() => filter.scope, () => props.session, () => props.diagnosisRevision],
  async () => {
    const ticket = ++diagnosisRequest;
    diagnosis.value = null;
    if (!props.loadDiagnosis || ["all", "launcher"].includes(filter.scope))
      return;
    try {
      const report = await props.loadDiagnosis(filter.scope);
      if (ticket === diagnosisRequest) diagnosis.value = report;
    } catch {
      /* Diagnostic I/O must never hide the underlying game log. */
    }
  },
  { immediate: true },
);
const raw = shallowRef(props.store.snapshot());
const presenter = createLogPresenter();
const rows = computed(() =>
  presenter.update(raw.value, translateMessage, locale.value),
);
const filtered = computed(() => filterLogs(rows.value, filter));
const count = computed(() =>
  filtered.value.reduce((sum, row) => sum + row.count, 0),
);
const scopedCount = computed(() =>
  rows.value
    .filter((line) => filter.scope === "all" || line.scope === filter.scope)
    .reduce((sum, line) => sum + line.count, 0),
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
  [
    () => filter.scope,
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
  diagnosisRequest++;
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
    <details v-if="diagnosis" class="crash-diagnosis" open>
      <summary>
        {{ t("崩溃诊断") }}<small>{{ diagnosis.version }}</small>
      </summary>
      <div class="crash-findings">
        <p v-if="!diagnosis.findings.length">
          {{ t("未找到明确的已知原因，请结合完整日志继续排查。") }}
        </p>
        <article v-for="finding in diagnosis.findings" :key="finding.code">
          <strong>{{ t(finding.title) }}</strong>
          <p>{{ t(finding.advice) }}</p>
          <details>
            <summary>{{ t("日志依据") }}</summary>
            <pre>{{ finding.evidence.join("\n") }}</pre>
          </details>
        </article>
        <small>{{ t("诊断来自本次运行日志，不代表已确定唯一原因。") }}</small>
        <button
          type="button"
          class="text-button"
          @click="emit('copy', JSON.stringify(diagnosis, null, 2))"
        >
          <Copy :size="13" />{{ t("复制诊断") }}
        </button>
      </div>
    </details>
    <div class="log-filters">
      <div class="list-reveal-item" :style="listRevealStyle(0)">
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

<style scoped>
.crash-diagnosis {
  flex: 0 0 auto;
  min-height: 0;
  border-bottom: 1px solid color-mix(in srgb, currentColor 12%, transparent);
  padding: 8px var(--panel-inset);
}
.crash-diagnosis > summary {
  cursor: pointer;
  font-size: 12px;
}
.crash-diagnosis small {
  margin-left: 8px;
  opacity: 0.6;
  font-size: 10px;
}
.crash-findings {
  max-height: 210px;
  overflow-y: auto;
  padding: 8px 4px;
  font-size: 12px;
}
.crash-findings article + article {
  margin-top: 12px;
}
.crash-findings p {
  margin: 5px 0;
  opacity: 0.8;
  line-height: 1.6;
}
.crash-findings pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 10px;
}
.crash-findings details summary {
  cursor: pointer;
  opacity: 0.7;
}
</style>

<style scoped>
.crash-findings > small {
  display: block;
  margin: 8px 0;
  line-height: 1.5;
}
</style>
