<script setup lang="ts">
import {
  defineAsyncComponent,
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { locale, t } from "../i18n";
import { HITOKOTO_URL } from "../homeWidgets";
import {
  homeWidgetLabels,
  mergeNews,
  parseFeed,
  parseQuote,
  presetFeeds,
  type FeedSource,
  type HomeWidget,
  type NewsItem,
  type Quote,
  type QuoteStyle,
} from "../homeWidgets";
import { cachedWidget, fetchWidget, type WidgetRpc } from "../widgetClient";
const MinecraftSplash = defineAsyncComponent(
  () => import("./MinecraftSplash.vue"),
);
const props = defineProps<{
  kind: HomeWidget;
  quoteStyle?: QuoteStyle;
  quoteAngle?: number;
  quoteScale?: number;
  newsSources?: FeedSource[];
  newsAlign?: "left" | "right";
  editing: boolean;
  rpc: WidgetRpc;
}>();
const emit = defineEmits<{ link: [url: string] }>();
const sources = computed(() => props.newsSources ?? [presetFeeds[0]]);
const quote = ref<Quote>();
const feedItems = ref<Record<string, NewsItem[]>>({});
const problems = ref<Record<string, string>>({});
const quoteProblem = ref(false);
const loading = ref(false);
let disposed = false,
  generation = 0,
  quoteRequested = false;
const items = computed(() =>
  mergeNews(sources.value.map((source) => feedItems.value[source.url] || [])),
);
const displayedQuote = computed(
  () =>
    quote.value ||
    (props.editing
      ? { text: "祸不妄至，福不徒来。", source: "——司马迁《史记·龟策列传》" }
      : undefined),
);
const previewNews: NewsItem[] = [
  {
    title: "Minecraft Java 新版本动态",
    url: "https://www.minecraft.net/",
    date: "2026-11-03",
    source: "Minecraft 官方动态",
  },
  {
    title: "猫咪盒子启动器 · 开发版更新",
    url: "https://github.com/NekoOS-Group/NekoCrate-Launcher/releases",
    date: "2026-11-02",
    source: "猫咪盒子更新",
  },
  {
    title: "社区地图、创作与新玩法",
    url: "https://www.minecraft.net/community",
    date: "2026-11-01",
    source: "Minecraft 官方动态",
  },
];
const displayedNews = computed(() =>
  props.editing && !items.value.length
    ? previewNews.filter((item) =>
        sources.value.some((source) => source.name === item.source),
      )
    : items.value,
);
const failedSources = computed(() =>
  sources.value.filter((source) => problems.value[source.url]),
);
function readCache() {
  if (props.kind === "quote") {
    const cached = cachedWidget(HITOKOTO_URL);
    if (cached)
      try {
        quote.value = parseQuote(cached.text);
      } catch {
        /* Retry invalid cache. */
      }
  } else
    for (const source of sources.value) {
      const cached = cachedWidget(source.url);
      if (cached)
        try {
          feedItems.value[source.url] = parseFeed(
            cached.text,
            source,
            cached.url,
          );
        } catch {
          /* Retry invalid cache. */
        }
    }
}
readCache();
async function refresh(force = false) {
  if (props.editing || disposed || document.hidden || loading.value) return;
  const run = generation;
  loading.value = true;
  if (props.kind === "quote") {
    // Disk cache paints immediately and survives offline launches, but must
    // not pin the random quote across restarts for its full six-hour TTL.
    // Defer this first request until the widget is visible and editing ends.
    const refreshOnStart = !quoteRequested;
    quoteRequested = true;
    try {
      const response = await fetchWidget(
        props.rpc,
        HITOKOTO_URL,
        6 * 3600_000,
        (data) => parseQuote(data.text),
        force || refreshOnStart,
      );
      if (!disposed && generation === run) {
        quote.value = parseQuote(response.text);
        quoteProblem.value = false;
      }
    } catch {
      if (!disposed && generation === run) quoteProblem.value = true;
    }
  } else
    await Promise.all(
      sources.value.map(async (source) => {
        try {
          const response = await fetchWidget(
            props.rpc,
            source.url,
            30 * 60_000,
            (data) => parseFeed(data.text, source, data.url),
            force,
          );
          if (
            !disposed &&
            generation === run &&
            sources.value.some((item) => item.url === source.url)
          ) {
            feedItems.value[source.url] = parseFeed(
              response.text,
              source,
              response.url,
            );
            delete problems.value[source.url];
          }
        } catch {
          if (!disposed && generation === run)
            problems.value[source.url] = source.name;
        }
      }),
    );
  if (!disposed && generation === run) loading.value = false;
}
function date(value: string) {
  return value
    ? new Intl.DateTimeFormat(locale.value, {
        month: "short",
        day: "numeric",
      }).format(new Date(value))
    : "";
}
function backgroundRefresh() {
  if (!document.hidden) void refresh();
}
watch(
  () => props.editing,
  (value) => {
    if (!value) void refresh();
  },
);
watch(
  () => props.newsSources,
  () => {
    if (props.kind !== "news") return;
    generation++;
    loading.value = false;
    readCache();
    void refresh();
  },
);
const quoteBody = ref<HTMLElement>();
const newsList = ref<HTMLElement>();
const newsScrolled = ref(false);
function updateNewsScroll() {
  newsScrolled.value = (newsList.value?.scrollTop ?? 0) > 1;
}
watch([newsList, displayedNews], updateNewsScroll, { flush: "post" });
const quoteText = ref<HTMLElement>();
const textScale = ref(1);
let textObserver: ResizeObserver | undefined;
function fitText() {
  const body = quoteBody.value,
    text = quoteText.value;
  if (!body || !text) return;
  const angle = ((props.quoteAngle || 0) * Math.PI) / 180,
    c = Math.abs(Math.cos(angle)),
    s = Math.abs(Math.sin(angle));
  const width = text.offsetWidth * c + text.offsetHeight * s,
    height = text.offsetWidth * s + text.offsetHeight * c;
  textScale.value = Math.min(
    1,
    body.clientWidth / Math.max(1, width),
    body.clientHeight / Math.max(1, height),
  );
}
function observeText() {
  textObserver?.disconnect();
  if (quoteBody.value) textObserver?.observe(quoteBody.value);
  if (quoteText.value) textObserver?.observe(quoteText.value);
  fitText();
}
watch([quoteBody, quoteText, () => props.quoteAngle], observeText, {
  flush: "post",
});
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  textObserver = new ResizeObserver(fitText);
  observeText();
  void refresh();
  timer = setInterval(backgroundRefresh, 30 * 60_000);
  document.addEventListener("visibilitychange", backgroundRefresh);
});
onBeforeUnmount(() => {
  textObserver?.disconnect();
  disposed = true;
  generation++;
  clearInterval(timer);
  document.removeEventListener("visibilitychange", backgroundRefresh);
});
</script>

<template>
  <section
    class="home-widget"
    :class="[
      `home-${kind}`,
      kind === 'quote' ? `quote-${quoteStyle || 'simple'}` : null,
      kind === 'news' ? `news-align-${newsAlign || 'left'}` : null,
    ]"
    :aria-label="t(homeWidgetLabels[kind])"
    :style="
      kind === 'quote'
        ? { '--quote-scale': (quoteScale ?? 100) / 100 }
        : undefined
    "
  >
    <template v-if="kind === 'quote'">
      <Transition name="widget-text" mode="out-in">
        <div
          :key="`${quoteStyle}:${displayedQuote?.text || 'empty'}`"
          class="home-quote-copy"
        >
          <div ref="quoteBody" class="quote-text-stage">
            <MinecraftSplash
              v-if="quoteStyle === 'splash' && displayedQuote"
              :text="displayedQuote.text"
              :source="displayedQuote.source"
              :angle="quoteAngle ?? -20"
              :scale="(quoteScale ?? 100) / 100"
              :rpc="rpc"
            />
            <div
              v-else
              ref="quoteText"
              class="simple-quote-lines"
              :style="{
                transform: `rotate(${quoteAngle ?? 0}deg) scale(${textScale})`,
              }"
            >
              <p>
                {{
                  displayedQuote?.text ||
                  t(quoteProblem ? "暂时无法获取一言" : "…")
                }}
              </p>
              <small
                v-if="displayedQuote?.source"
                :title="displayedQuote.source"
                >{{ displayedQuote.source }}</small
              >
            </div>
          </div>
        </div>
      </Transition>
      <span v-if="quoteProblem && quote" class="widget-hint">{{
        t("暂时无法刷新，保留上次内容")
      }}</span>
    </template>
    <template v-else>
      <div
        ref="newsList"
        class="home-news-list"
        :class="{ scrolled: newsScrolled }"
        :aria-busy="loading"
        @scroll.passive="updateNewsScroll"
      >
        <TransitionGroup name="widget-news" tag="ol" class="news-timeline">
          <li v-for="item in displayedNews" :key="item.url">
            <a
              :href="item.url"
              :title="item.title"
              @click.prevent="emit('link', item.url)"
            >
              <small class="news-timeline-meta">
                <time v-if="item.date" :datetime="item.date">{{
                  date(item.date)
                }}</time>
                <span>{{ t(item.source) }}</span>
              </small>
              <span class="news-timeline-title">{{ item.title }}</span>
            </a>
          </li>
        </TransitionGroup>
        <p v-if="!displayedNews.length" class="widget-empty">
          {{
            t(
              !sources.length
                ? "选择想关注的订阅源"
                : loading
                  ? "…"
                  : failedSources.length
                    ? "暂时无法获取动态"
                    : "暂无动态",
            )
          }}
        </p>
      </div>
      <span
        v-if="failedSources.length"
        class="widget-hint"
        :title="failedSources.map((source) => source.name).join(' · ')"
        >{{ t("部分订阅暂不可用，稍后自动重试") }}</span
      >
    </template>
  </section>
</template>

<style>
.home-widget {
  position: relative;
  width: 100%;
  height: 100%;
  color: var(--ink);
  font-size: 13px;
}
.home-quote {
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 4px 20px 4px 0;
  user-select: none;
  -webkit-user-select: none;
}
.home-quote-copy p {
  margin: 0;
  font-size: calc(17px * var(--quote-scale, 1));
  line-height: 1.65;
  font-weight: 450;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-wrap: pretty;
}
.home-quote-copy small {
  display: block;
  margin-top: 8px;
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.home-news {
  display: flex;
  flex-direction: column;
}
.home-news-list {
  min-height: 0;
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  scroll-padding-top: 24px;
  scroll-padding-bottom: 36px;
  -webkit-mask-image: linear-gradient(
    to bottom,
    #000 calc(100% - 36px),
    transparent
  );
  mask-image: linear-gradient(to bottom, #000 calc(100% - 36px), transparent);
}
.home-news-list::-webkit-scrollbar {
  display: none;
  width: 0;
  height: 0;
}
.home-news-list.scrolled {
  -webkit-mask-image: linear-gradient(
    to bottom,
    transparent,
    #000 24px,
    #000 calc(100% - 36px),
    transparent
  );
  mask-image: linear-gradient(
    to bottom,
    transparent,
    #000 24px,
    #000 calc(100% - 36px),
    transparent
  );
}
.news-timeline {
  list-style: none;
  padding: 4px 4px 36px 2px;
  margin: 0;
}
.news-timeline > li {
  position: relative;
  padding: 0 4px 18px 20px;
}
.news-timeline > li::before {
  content: "";
  position: absolute;
  left: 4px;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--glass-stroke);
}
.news-timeline > li:first-child::before {
  top: 8px;
}
.news-timeline > li:last-child::before {
  height: 8px;
  bottom: auto;
}
.news-timeline > li:only-child::before {
  display: none;
}
.news-timeline > li::after {
  content: "";
  position: absolute;
  left: 1px;
  top: 5px;
  width: 7px;
  height: 7px;
  border: 1px solid var(--blue);
  background: var(--night);
  border-radius: 50%;
  pointer-events: none;
  transition:
    background 160ms,
    box-shadow 160ms;
}
.news-timeline > li:first-child::after,
.news-timeline > li:is(:hover, :focus-within)::after {
  background: var(--blue);
  box-shadow: 0 0 8px var(--glow);
}
.news-timeline a {
  display: flex;
  flex-direction: column;
  gap: 5px;
  color: var(--ink);
  text-decoration: none;
  transition: color 160ms;
}
.news-timeline-title {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  line-height: 1.5;
  text-wrap: pretty;
}
.news-timeline a:is(:hover, :focus-visible) {
  color: var(--blue);
  outline: none;
  text-shadow: 0 0 10px var(--glow);
}
.news-timeline-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 10px;
  line-height: 16px;
}
.news-timeline-meta time {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
.news-timeline-meta > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.news-align-right .news-timeline {
  padding-left: 4px;
  padding-right: 2px;
}
.news-align-right .news-timeline > li {
  padding-left: 4px;
  padding-right: 20px;
}
.news-align-right .news-timeline > li::before {
  left: auto;
  right: 4px;
}
.news-align-right .news-timeline > li::after {
  left: auto;
  right: 1px;
}
.news-align-right .news-timeline a {
  text-align: right;
}
.news-align-right .news-timeline-meta {
  justify-content: flex-end;
}
.news-align-right .widget-empty {
  text-align: right;
}
.widget-empty {
  color: var(--muted);
  font-size: 12px;
  margin: 24px 0;
}
.widget-hint {
  display: block;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.5;
  margin-top: 5px;
}
.widget-text-enter-active,
.widget-text-leave-active,
.widget-news-enter-active,
.widget-news-leave-active {
  transition:
    opacity 160ms var(--motion-ease),
    transform 200ms var(--motion-ease);
}
.widget-text-enter-from,
.widget-news-enter-from {
  opacity: 0;
  transform: translateY(5px);
}
.widget-text-leave-to,
.widget-news-leave-to {
  opacity: 0;
  transform: translateY(-5px);
}
@media (prefers-reduced-motion: reduce) {
  .widget-text-enter-active,
  .widget-text-leave-active,
  .widget-news-enter-active,
  .widget-news-leave-active {
    transition: none;
  }
}
</style>

<style>
/* Rotate the quote and its right-aligned attribution as one fitted group. */
.home-quote {
  padding: 0 calc(10px * var(--quote-scale, 1));
}
.home-quote-copy {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
}
.quote-text-stage {
  height: 100%;
  width: 100%;
  display: grid;
  place-items: center;
  min-height: 0;
}
.simple-quote-lines {
  width: 100%;
  transform-origin: center;
  transition: transform 100ms linear;
}
.home-quote-copy small {
  width: 100%;
  text-align: right;
  margin-top: calc(5px * var(--quote-scale, 1));
  font-size: calc(10px * var(--quote-scale, 1));
}
</style>
