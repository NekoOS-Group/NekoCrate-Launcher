<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import {
  Boxes,
  ALargeSmall,
  AlignLeft,
  AlignRight,
  PanelLeftClose,
  PanelRightClose,
  Check,
  Columns3,
  Download,
  Grip,
  Quote,
  Rss,
  Plus,
  RotateCcw,
  RotateCw,
  Sparkles,
  Type,
  Rows3,
  Settings2,
  Terminal,
  Server,
  Bookmark,
  Map,
  ListTodo,
  Clock,
  CalendarDays,
  Palette,
  X,
} from "lucide-vue-next";
import LayoutSlider from "./LayoutSlider.vue";
import NewsSources from "./NewsSources.vue";
import {
  defaultHomeTools,
  homeToolLabels,
  homeTools,
  readHomeTools,
  sanitizeHomeTools,
  type HomeTool,
} from "../homeToolbar";
import { t } from "../i18n";
import {
  homeWidgetLabels,
  homeWidgets,
  readHomeWidgets,
  readQuoteStyle,
  readQuoteAngles,
  defaultQuoteAngles,
  defaultQuoteScales,
  readQuoteScales,
  quoteStyleLabels,
  readFeedSources,
  NEWS_SOURCES_KEY,
  readNewsLayout,
  defaultNewsHeight,
  readClockLayout,
  type NewsDock,
  type FeedSource,
  type QuoteStyle,
  type HomeWidget,
} from "../homeWidgets";
import {
  HOME_LAYOUT_KEY,
  homeGroups,
  removableHomeGroups,
  readHiddenHomeGroups,
  homeSizes,
  arrangeHomeLayout,
  expandHomeLayout,
  normalizeHomeLayout,
  readHomeLayout,
  readHomeOrientations,
  defaultHomeOrientations,
  snapHomeRect,
  newsDockTarget,
  resizeNewsRect,
  inferHomeAttachments,
  readHomeAttachments,
  type NewsResizeEdge,
  type HomeItem,
  type SavedHomePositions,
  type HomeRects,
  type Point,
  type HomeOrientation,
  type HomeOrientations,
} from "../homeLayout";

const props = defineProps<{ instanceSlots: number; portable?: boolean }>();
const emit = defineEmits<{ editing: [value: boolean] }>();
const area = ref({ width: window.innerWidth, height: window.innerHeight });
function storedLayout(key = HOME_LAYOUT_KEY) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
const stored = storedLayout();
const saved = ref<SavedHomePositions | null>(readHomeLayout(stored));
const savedHiddenGroups = ref(readHiddenHomeGroups(stored));
const draftHiddenGroups = ref([...savedHiddenGroups.value]);
const hiddenGroups = computed(() =>
  (editing.value ? draftHiddenGroups.value : savedHiddenGroups.value).filter(
    (id) => id === "themes" || props.portable,
  ),
);
const savedClock = ref(readClockLayout(stored));
const draftClock = ref({ ...savedClock.value });
const clockLayout = computed(() =>
  editing.value ? draftClock.value : savedClock.value,
);
const savedOrientations = ref(readHomeOrientations(stored));
const draftOrientations = ref({ ...savedOrientations.value });
const savedTools = ref(readHomeTools(stored));
const draftTools = ref([...savedTools.value]);
const toolsMenu = ref(false);
const widgetsMenu = ref(false);
const savedNewsLayout = ref(readNewsLayout(stored));
const draftNewsLayout = ref({ ...savedNewsLayout.value });
const previewNewsDock = ref<NewsDock>();
const newsLayout = computed(() =>
  editing.value ? draftNewsLayout.value : savedNewsLayout.value,
);
const newsDock = computed(() =>
  editing.value && previewNewsDock.value !== undefined
    ? previewNewsDock.value
    : newsLayout.value.dock,
);
const newsOpen = ref(false);
const newsHost = ref<HTMLElement>();
let newsHideTimer: ReturnType<typeof setTimeout> | undefined;
const savedNewsSources = ref(readFeedSources(storedLayout(NEWS_SOURCES_KEY)));
const draftNewsSources = ref([...savedNewsSources.value]);
const newsSources = computed(() =>
  editing.value ? draftNewsSources.value : savedNewsSources.value,
);
const savedQuoteStyle = ref(readQuoteStyle(stored));
const savedQuoteAngles = ref(readQuoteAngles(stored));
const draftQuoteAngles = ref({ ...savedQuoteAngles.value });
const savedQuoteScales = ref(readQuoteScales(stored));
const draftQuoteScales = ref({ ...savedQuoteScales.value });
const quoteScale = computed(
  () =>
    (editing.value ? draftQuoteScales.value : savedQuoteScales.value)[
      quoteStyle.value
    ],
);
const quoteAngle = computed(
  () =>
    (editing.value ? draftQuoteAngles.value : savedQuoteAngles.value)[
      quoteStyle.value
    ],
);
const draftQuoteStyle = ref(savedQuoteStyle.value);
const quoteStyle = computed(() =>
  editing.value ? draftQuoteStyle.value : savedQuoteStyle.value,
);
const savedWidgets = ref(readHomeWidgets(stored));
const draftWidgets = ref([...savedWidgets.value]);
const widgets = computed(() =>
  editing.value ? draftWidgets.value : savedWidgets.value,
);
const availableWidgets = computed(() => [
  ...homeWidgets.filter((id) => !widgets.value.includes(id)),
  ...hiddenGroups.value,
]);
const groups = computed(() => [
  ...homeGroups.filter(
    (id) => !hiddenGroups.value.some((hidden) => hidden === id),
  ),
  ...widgets.value,
]);
const widgetIcons = {
  quote: Quote,
  news: Rss,
  servers: Server,
  favorites: Bookmark,
  worlds: Map,
  todos: ListTodo,
  clock: Clock,
  themes: Palette,
  instances: Boxes,
};
const tools = computed(() =>
  editing.value ? draftTools.value : savedTools.value,
);
const availableTools = computed(() =>
  homeTools.filter((id) => !tools.value.includes(id)),
);
const toolIcons = {
  resources: Boxes,
  settings: Settings2,
  logs: Terminal,
  store: Download,
};
const draft = ref<SavedHomePositions | null>(null);
const editing = ref(false);
const dragging = ref<HomeItem>();
const resizingNews = ref(false);
const preview = ref<HomeRects>();
const guides = ref<{ x?: number; y?: number }>({});
const handles = ref<HTMLButtonElement[]>([]);
const orientations = computed(() =>
  editing.value ? draftOrientations.value : savedOrientations.value,
);
const sizes = computed(() =>
  homeSizes(
    area.value,
    props.instanceSlots,
    orientations.value,
    tools.value.length,
    quoteScale.value / 100,
    newsLayout.value.height,
    clockLayout.value.scale / 100,
  ),
);
const savedAttachments = ref(
  readHomeAttachments(stored) ??
    inferHomeAttachments(
      expandHomeLayout(saved.value, area.value, sizes.value, widgets.value),
      hiddenGroups.value,
    ),
);
const draftAttachments = ref({ ...savedAttachments.value });
function orientation(id: HomeItem) {
  return id === "tools" || id === "instances"
    ? orientations.value[id]
    : "horizontal";
}
const rects = computed(
  () =>
    preview.value ||
    arrangeHomeLayout(
      expandHomeLayout(
        editing.value ? draft.value : saved.value,
        area.value,
        sizes.value,
        widgets.value,
        editing.value ? draftAttachments.value : savedAttachments.value,
        hiddenGroups.value,
      ),
      area.value,
      undefined,
      editing.value ? [toolbarRect()] : [],
      newsDock.value,
      hiddenGroups.value,
    ),
);
const labels: Record<HomeItem, string> = {
  account: "账户",
  tools: "工具栏",
  instances: "实例栏",
  themes: "主题切换",
  launch: "启动与版本",
  ...homeWidgetLabels,
};
let pointer:
  | {
      id: number;
      group: HomeItem;
      origin: Point;
      base: HomeRects;
      target: HTMLElement;
      resizeEdge?: NewsResizeEdge;
    }
  | undefined;
let pending: Point | undefined;
let frame: number | undefined;
let disposed = false;
function toolbarRect() {
  return { x: area.value.width - 222, y: 12, width: 210, height: 36 };
}
function changed() {
  // Anchored settings may be open while the application window is resized.
  window.dispatchEvent(new Event("nekocrate-home-layout"));
}
watch(
  rects,
  () =>
    void nextTick(() => {
      if (!disposed) changed();
    }),
  { flush: "post" },
);
function begin() {
  if (editing.value) return;
  draft.value = saved.value;
  draftHiddenGroups.value = [...savedHiddenGroups.value];
  draftClock.value = { ...savedClock.value };
  draftAttachments.value = { ...savedAttachments.value };
  draftOrientations.value = { ...savedOrientations.value };
  draftTools.value = [...savedTools.value];
  draftWidgets.value = [...savedWidgets.value];
  draftQuoteStyle.value = savedQuoteStyle.value;
  draftQuoteAngles.value = { ...savedQuoteAngles.value };
  draftQuoteScales.value = { ...savedQuoteScales.value };
  draftNewsSources.value = [...savedNewsSources.value];
  draftNewsLayout.value = { ...savedNewsLayout.value };
  closeNews();
  toolsMenu.value = false;
  widgetsMenu.value = false;
  editing.value = true;
  emit("editing", true);
  void nextTick(() => handles.value[0]?.focus({ preventScroll: true }));
}
function stopPointer() {
  if (frame !== undefined) cancelAnimationFrame(frame);
  frame = undefined;
  pending = undefined;
  const previous = pointer;
  pointer = undefined;
  if (previous?.target.hasPointerCapture(previous.id))
    previous.target.releasePointerCapture(previous.id);
  dragging.value = undefined;
  resizingNews.value = false;
  guides.value = {};
  document.documentElement.classList.remove("home-layout-dragging");
  document.documentElement.classList.remove("home-layout-resizing");
  document.removeEventListener("pointermove", move);
  document.removeEventListener("pointerup", release);
  document.removeEventListener("pointercancel", cancelDrag);
  document.removeEventListener("selectstart", preventSelection, true);
  window.removeEventListener("blur", cancelDrag);
}
function cancelDrag() {
  stopPointer();
  preview.value = undefined;
  previewNewsDock.value = undefined;
}
function end(commit: boolean) {
  cancelDrag();
  toolsMenu.value = false;
  widgetsMenu.value = false;
  if (commit) {
    saved.value = draft.value;
    savedHiddenGroups.value = [...draftHiddenGroups.value];
    savedClock.value = { ...draftClock.value };
    savedAttachments.value = { ...draftAttachments.value };
    savedOrientations.value = { ...draftOrientations.value };
    savedTools.value = [...draftTools.value];
    savedWidgets.value = [...draftWidgets.value];
    savedQuoteStyle.value = draftQuoteStyle.value;
    savedQuoteAngles.value = { ...draftQuoteAngles.value };
    savedQuoteScales.value = { ...draftQuoteScales.value };
    savedNewsSources.value = [...draftNewsSources.value];
    savedNewsLayout.value = { ...draftNewsLayout.value };
    try {
      const sources = JSON.stringify(savedNewsSources.value);
      if (sources === JSON.stringify(readFeedSources(null)))
        localStorage.removeItem(NEWS_SOURCES_KEY);
      else localStorage.setItem(NEWS_SOURCES_KEY, sources);
      if (
        saved.value ||
        savedHiddenGroups.value.length ||
        savedWidgets.value.length ||
        savedQuoteStyle.value !== "simple" ||
        savedQuoteAngles.value.simple !== 0 ||
        savedQuoteAngles.value.splash !== -20 ||
        savedQuoteScales.value.simple !== 100 ||
        savedQuoteScales.value.splash !== 100 ||
        savedNewsLayout.value.align !== "left" ||
        savedNewsLayout.value.dock !== null ||
        savedNewsLayout.value.height !== defaultNewsHeight ||
        savedClock.value.scale !== 100 ||
        savedClock.value.angle !== 0 ||
        !savedClock.value.date ||
        Object.values(savedOrientations.value).includes("vertical") ||
        savedTools.value.join() !== defaultHomeTools.join()
      )
        localStorage.setItem(
          HOME_LAYOUT_KEY,
          JSON.stringify({
            version: 1,
            positions: saved.value,
            hiddenGroups: savedHiddenGroups.value,
            attachments: savedAttachments.value,
            orientations: savedOrientations.value,
            tools: savedTools.value,
            widgets: savedWidgets.value,
            quoteStyle: savedQuoteStyle.value,
            quoteAngles: savedQuoteAngles.value,
            quoteScales: savedQuoteScales.value,
            news: savedNewsLayout.value,
            clock: savedClock.value,
          }),
        );
      else localStorage.removeItem(HOME_LAYOUT_KEY);
    } catch {
      /* Storage may be disabled; retain the layout for this session. */
    }
  }
  editing.value = false;
  closeNews();
  emit("editing", false);
}
function reset() {
  cancelDrag();
  draft.value = null;
  draftHiddenGroups.value = [];
  draftClock.value = readClockLayout(null);
  draftAttachments.value = {};
  draftOrientations.value = { ...defaultHomeOrientations };
  draftTools.value = [...defaultHomeTools];
  draftWidgets.value = [];
  draftQuoteStyle.value = "simple";
  draftQuoteAngles.value = { ...defaultQuoteAngles };
  draftQuoteScales.value = { ...defaultQuoteScales };
  draftNewsSources.value = readFeedSources(null);
  draftNewsLayout.value = readNewsLayout(null);
  toolsMenu.value = false;
  widgetsMenu.value = false;
}
function setQuoteStyle(value: QuoteStyle) {
  if (!editing.value || !Object.hasOwn(quoteStyleLabels, value)) return;
  draftQuoteStyle.value = value;
}
function setQuoteAngle(value: number) {
  if (!editing.value || !Number.isFinite(value)) return;
  draftQuoteAngles.value = {
    ...draftQuoteAngles.value,
    [quoteStyle.value]: Math.round(Math.max(-45, Math.min(45, value))),
  };
}
function setQuoteScale(value: number) {
  if (!editing.value || !Number.isFinite(value)) return;
  draftQuoteScales.value = {
    ...draftQuoteScales.value,
    [quoteStyle.value]: Math.round(Math.max(60, Math.min(160, value))),
  };
}
function setNewsSources(value: FeedSource[]) {
  if (editing.value)
    draftNewsSources.value = readFeedSources(JSON.stringify(value));
}
function toggleNewsAlignment() {
  if (editing.value)
    draftNewsLayout.value = {
      ...draftNewsLayout.value,
      align: draftNewsLayout.value.align === "left" ? "right" : "left",
    };
}
function undockNews() {
  if (editing.value)
    draftNewsLayout.value = { ...draftNewsLayout.value, dock: null };
}
function setNewsHost(element: unknown) {
  newsHost.value = element instanceof HTMLElement ? element : undefined;
}
function showNews() {
  clearTimeout(newsHideTimer);
  if (!editing.value && newsDock.value) newsOpen.value = true;
}
function closeNews() {
  clearTimeout(newsHideTimer);
  newsOpen.value = false;
}
function leaveNews() {
  clearTimeout(newsHideTimer);
  newsHideTimer = setTimeout(() => {
    // Pointer movement does not clear :focus-visible in WebKit. A previously
    // focused link must not pin the panel open after the pointer leaves.
    // Focus moving between links stays open through showNews / blurNews.
    newsOpen.value = false;
  }, 180);
}
function blurNews(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !newsHost.value?.contains(event.relatedTarget)
  )
    leaveNews();
}
function escapeNews(event: KeyboardEvent) {
  if (editing.value || !newsDock.value) return;
  event.stopPropagation();
  event.preventDefault();
  newsHost.value
    ?.querySelector<HTMLButtonElement>(".news-edge-trigger")
    ?.focus({ preventScroll: true });
  closeNews();
}
function removable(id: HomeItem) {
  return (
    homeWidgets.some((widget) => widget === id) ||
    id === "themes" ||
    (id === "instances" && !!props.portable)
  );
}
function layoutAttachments(layout: HomeRects) {
  const links = inferHomeAttachments(layout, hiddenGroups.value);
  if (
    hiddenGroups.value.includes("instances") &&
    draftAttachments.value.instances
  )
    links.instances = draftAttachments.value.instances;
  return links;
}
function toggleWidget(id: HomeItem) {
  if (!editing.value || !removable(id)) return;
  cancelDrag();
  const before = normalizeHomeLayout(rects.value, area.value);
  if (id === "themes" || id === "instances") {
    draftHiddenGroups.value = draftHiddenGroups.value.includes(id)
      ? draftHiddenGroups.value.filter((group) => group !== id)
      : removableHomeGroups.filter(
          (group) => group === id || draftHiddenGroups.value.includes(group),
        );
  } else {
    const widget = id as HomeWidget;
    const present = draftWidgets.value.includes(widget);
    draftWidgets.value = present
      ? draftWidgets.value.filter((item) => item !== widget)
      : [...draftWidgets.value, widget];
    delete before[widget];
  }
  draft.value = before;
  draftAttachments.value = layoutAttachments(rects.value);
  widgetsMenu.value = false;
}
function rememberLayout(layout: HomeRects) {
  draft.value = normalizeHomeLayout(layout, area.value);
  draftAttachments.value = layoutAttachments(layout);
}
function setTools(value: HomeTool[]) {
  if (!editing.value) return;
  const next = sanitizeHomeTools(value);
  if (next.join() === tools.value.join()) return;
  cancelDrag();
  const before = rects.value;
  draftTools.value = next;
  rememberLayout(
    arrangeHomeLayout(
      {
        ...before,
        tools: { ...before.tools, ...sizes.value.tools },
      },
      area.value,
      "tools",
      [toolbarRect()],
      newsDock.value,
      hiddenGroups.value,
    ),
  );
  toolsMenu.value = false;
  widgetsMenu.value = false;
}
function setOrientation(group: keyof HomeOrientations, value: HomeOrientation) {
  if (!editing.value || orientations.value[group] === value) return;
  cancelDrag();
  toolsMenu.value = false;
  widgetsMenu.value = false;
  const before = rects.value;
  draftOrientations.value = { ...draftOrientations.value, [group]: value };
  const size = sizes.value[group];
  // Rotate around the existing centre, then fit the frame and its neighbours.
  const arranged = arrangeHomeLayout(
    {
      ...before,
      [group]: {
        ...size,
        x: before[group].x + (before[group].width - size.width) / 2,
        y: before[group].y + (before[group].height - size.height) / 2,
      },
    },
    area.value,
    group,
    [toolbarRect()],
    newsDock.value,
    hiddenGroups.value,
  );
  rememberLayout(arranged);
}
function preventSelection(event: Event) {
  event.preventDefault();
}
function start(
  event: PointerEvent,
  group: HomeItem,
  resizeEdge?: NewsResizeEdge,
) {
  if (!editing.value || event.button !== 0 || !event.isPrimary || pointer)
    return;
  event.preventDefault();
  toolsMenu.value = false;
  widgetsMenu.value = false;
  event.stopPropagation();
  const target = event.currentTarget as HTMLElement;
  target.focus({ preventScroll: true });
  pointer = {
    id: event.pointerId,
    group,
    origin: { x: event.clientX, y: event.clientY },
    base: rects.value,
    target,
    resizeEdge,
  };
  dragging.value = group;
  resizingNews.value = !!resizeEdge;
  target.setPointerCapture(event.pointerId);
  document.documentElement.classList.add("home-layout-dragging");
  if (resizeEdge)
    document.documentElement.classList.add("home-layout-resizing");
  document.addEventListener("selectstart", preventSelection, true);
  document.addEventListener("pointermove", move, { passive: false });
  document.addEventListener("pointerup", release);
  document.addEventListener("pointercancel", cancelDrag);
  window.addEventListener("blur", cancelDrag);
}
function place(group: HomeItem, point: Point, base: HomeRects, snap = true) {
  const proposed = { ...base[group]!, ...point };
  if (group === "news")
    previewNewsDock.value = newsDockTarget(
      proposed,
      area.value,
      newsDock.value,
    );
  const result = snap
    ? snapHomeRect(
        proposed,
        groups.value.filter((id) => id !== group).map((id) => base[id]!),
        area.value,
      )
    : { rect: proposed, guides: {} as { x?: number; y?: number } };
  const arranged = arrangeHomeLayout(
    { ...base, [group]: result.rect },
    area.value,
    group,
    [toolbarRect()],
    newsDock.value,
    hiddenGroups.value,
  );
  preview.value = arranged;
  guides.value = {
    x: arranged[group]!.x === result.rect.x ? result.guides.x : undefined,
    y: arranged[group]!.y === result.rect.y ? result.guides.y : undefined,
  };
}
function draw() {
  frame = undefined;
  if (!pointer || !pending) return;
  const { base, origin, group } = pointer;
  if (pointer.resizeEdge) {
    preview.value = {
      ...base,
      news: resizeNewsRect(
        base.news!,
        pointer.resizeEdge,
        pending.y - origin.y,
        area.value,
      ),
    };
    return;
  }
  place(
    group,
    {
      x: base[group]!.x + pending.x - origin.x,
      y: base[group]!.y + pending.y - origin.y,
    },
    base,
  );
}
function move(event: PointerEvent) {
  if (!pointer || event.pointerId !== pointer.id) return;
  event.preventDefault();
  pending = { x: event.clientX, y: event.clientY };
  if (frame === undefined) frame = requestAnimationFrame(draw);
}
function release(event: PointerEvent) {
  if (!pointer || event.pointerId !== pointer.id) return;
  if (frame !== undefined) cancelAnimationFrame(frame);
  // A press without movement must not persist a new layout.
  if (pending) {
    pending = { x: event.clientX, y: event.clientY };
    draw();
    if (preview.value) rememberLayout(preview.value);
    if (pointer.group === "news")
      draftNewsLayout.value = {
        ...draftNewsLayout.value,
        ...(pointer.resizeEdge
          ? { height: preview.value!.news!.height }
          : { dock: previewNewsDock.value ?? null }),
      };
  }
  stopPointer();
  preview.value = undefined;
  previewNewsDock.value = undefined;
}
function nudge(event: KeyboardEvent, group: HomeItem) {
  if (
    !editing.value ||
    !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)
  )
    return;
  event.preventDefault();
  const step = event.shiftKey ? 1 : 8,
    origin = rects.value[group]!;
  // A docked panel is pinned at the inset; let keyboard movement detach it
  // without repeatedly losing its progress inside the pointer hysteresis zone.
  if (
    group === "news" &&
    ((newsDock.value === "left" && event.key === "ArrowRight") ||
      (newsDock.value === "right" && event.key === "ArrowLeft"))
  ) {
    const x =
      newsDock.value === "left" ? 36 : area.value.width - origin.width - 36;
    draftNewsLayout.value = { ...draftNewsLayout.value, dock: null };
    rememberLayout({ ...rects.value, news: { ...origin, x } });
    return;
  }
  place(
    group,
    {
      x:
        origin.x +
        (event.key === "ArrowLeft"
          ? -step
          : event.key === "ArrowRight"
            ? step
            : 0),
      y:
        origin.y +
        (event.key === "ArrowUp"
          ? -step
          : event.key === "ArrowDown"
            ? step
            : 0),
    },
    rects.value,
    false,
  );
  rememberLayout(preview.value!);
  if (group === "news")
    draftNewsLayout.value = {
      ...draftNewsLayout.value,
      dock: previewNewsDock.value ?? null,
    };
  preview.value = undefined;
  previewNewsDock.value = undefined;
  guides.value = {};
}
function nudgeNewsSize(event: KeyboardEvent, edge: NewsResizeEdge) {
  if (!editing.value || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
  event.preventDefault();
  const delta = (event.key === "ArrowUp" ? -1 : 1) * (event.shiftKey ? 1 : 8);
  const before = rects.value;
  const news = resizeNewsRect(before.news!, edge, delta, area.value);
  draftNewsLayout.value = { ...draftNewsLayout.value, height: news.height };
  rememberLayout({ ...before, news });
}
function keyboard(event: KeyboardEvent) {
  if (!editing.value || event.key !== "Escape") return;
  event.preventDefault();
  if (widgetsMenu.value) widgetsMenu.value = false;
  else if (toolsMenu.value) toolsMenu.value = false;
  else if (pointer) cancelDrag();
  else end(false);
}
function resize() {
  cancelDrag();
  area.value = { width: window.innerWidth, height: window.innerHeight };
}
function style(id: HomeItem) {
  const box = rects.value[id]!;
  return {
    left: `${box.x}px`,
    top: `${box.y}px`,
    width: `${box.width}px`,
    height: `${box.height}px`,
    "--layout-x": `${box.x}px`,
    "--layout-y": `${box.y}px`,
    "--layout-width": `${box.width}px`,
    "--layout-height": `${box.height}px`,
  };
}
onMounted(() => {
  window.addEventListener("resize", resize);
  document.addEventListener("keydown", keyboard);
  window.addEventListener("blur", closeNews);
});
onBeforeUnmount(() => {
  disposed = true;
  cancelDrag();
  window.removeEventListener("resize", resize);
  document.removeEventListener("keydown", keyboard);
  window.removeEventListener("blur", closeNews);
  closeNews();
});
defineExpose({ begin });
</script>

<template>
  <slot />
  <Transition name="layout-editor">
    <div
      v-if="editing"
      class="home-layout-editor"
      data-drop-kind="blocked"
      @contextmenu.prevent
    >
      <div class="layout-safe-area" aria-hidden="true" />
      <i
        v-if="guides.x !== undefined"
        class="layout-guide vertical"
        :style="{ left: `${guides.x}px` }"
      />
      <i
        v-if="guides.y !== undefined"
        class="layout-guide horizontal"
        :style="{ top: `${guides.y}px` }"
      />
    </div>
  </Transition>
  <Transition name="layout-editor">
    <div
      v-if="editing"
      class="layout-toolbar glass-surface"
      role="toolbar"
      :aria-label="t('编辑布局')"
    >
      <span class="layout-toolbar-title">{{ t("编辑布局") }}</span>
      <button
        v-if="availableWidgets.length"
        class="icon-button"
        :title="t('添加小组件')"
        :aria-label="t('添加小组件')"
        :aria-expanded="widgetsMenu"
        @click="
          widgetsMenu = !widgetsMenu;
          toolsMenu = false;
        "
      >
        <Plus :size="16" />
      </button>
      <Transition name="layout-editor">
        <div
          v-if="widgetsMenu && availableWidgets.length"
          class="layout-tools-menu layout-widgets-menu glass-surface"
        >
          <button
            v-for="widget in availableWidgets"
            :key="widget"
            @click="toggleWidget(widget)"
          >
            <component :is="widgetIcons[widget]" :size="16" /><span>{{
              t(labels[widget])
            }}</span>
          </button>
        </div>
      </Transition>
      <button
        class="icon-button"
        :title="t('恢复默认布局')"
        :aria-label="t('恢复默认布局')"
        @click="reset"
      >
        <RotateCcw :size="16" />
      </button>
      <button
        class="icon-button"
        :title="t('取消')"
        :aria-label="t('取消')"
        @click="end(false)"
      >
        <X :size="16" />
      </button>
      <button
        class="icon-button layout-done"
        :title="t('完成')"
        :aria-label="t('完成')"
        @click="end(true)"
      >
        <Check :size="16" />
      </button>
    </div>
  </Transition>
  <TransitionGroup name="home-widget">
    <div
      v-for="id in groups"
      :key="id"
      :ref="id === 'news' ? setNewsHost : undefined"
      class="home-layout-item"
      :data-layout-group="id"
      :data-home-widget="
        homeWidgets.some((widget) => widget === id) ? 'true' : undefined
      "
      :data-orientation="orientation(id)"
      :class="{
        'layout-editing': editing,
        'layout-dragging': dragging === id,
        'layout-resizing': id === 'news' && resizingNews,
        'news-docked': id === 'news' && newsDock && !editing,
        'news-revealed': id === 'news' && newsOpen,
      }"
      :data-news-dock="id === 'news' ? newsDock : undefined"
      :data-vertical="
        rects[id]!.y + rects[id]!.height / 2 < area.height / 2
          ? 'top'
          : 'bottom'
      "
      :data-horizontal="
        rects[id]!.x + rects[id]!.width / 2 < area.width / 2 ? 'left' : 'right'
      "
      :style="style(id)"
      @transitionend.self="changed"
      @pointerenter="id === 'news' && showNews()"
      @pointerleave="id === 'news' && leaveNews()"
      @focusin="id === 'news' && showNews()"
      @focusout="id === 'news' && blurNews($event)"
      @keydown.esc="id === 'news' && escapeNews($event)"
    >
      <button
        v-if="id === 'news' && newsDock && !editing"
        class="news-edge-trigger"
        :aria-label="t('展开 Trends')"
        :aria-expanded="newsOpen"
        @click="showNews"
      />
      <div
        class="home-layout-content"
        :inert="
          (editing && id !== 'tools') ||
          (id === 'news' && !!newsDock && !editing && !newsOpen)
        "
      >
        <slot
          :name="id"
          :orientation="orientation(id)"
          :editing="editing"
          :quote-style="quoteStyle"
          :quote-angle="quoteAngle"
          :quote-scale="quoteScale"
          :news-sources="newsSources"
          :news-align="newsLayout.align"
          :clock-scale="clockLayout.scale"
          :clock-angle="clockLayout.angle"
          :clock-date="clockLayout.date"
          :tools="tools"
          :update-tools="setTools"
        />
      </div>
      <button
        v-if="editing"
        ref="handles"
        class="layout-handle"
        :aria-label="
          t('移动{name}，方向键调整，Shift 微调', { name: t(labels[id]) })
        "
        @pointerdown="start($event, id)"
        @keydown="nudge($event, id)"
        @contextmenu.prevent
        @dragstart.prevent
      />
      <div
        v-if="editing"
        class="layout-label"
        tabindex="-1"
        @pointerdown="start($event, id)"
      >
        <span><Grip :size="13" />{{ t(labels[id]) }}</span>
        <button
          v-if="id === 'instances' || id === 'tools'"
          type="button"
          class="layout-orientation"
          :title="
            orientation(id) === 'horizontal' ? t('切换为竖排') : t('切换为横排')
          "
          :aria-label="
            orientation(id) === 'horizontal' ? t('切换为竖排') : t('切换为横排')
          "
          @pointerdown.stop
          @click.stop="
            setOrientation(
              id,
              orientation(id) === 'horizontal' ? 'vertical' : 'horizontal',
            )
          "
        >
          <Columns3 v-if="orientation(id) === 'horizontal'" :size="14" /><Rows3
            v-else
            :size="14"
          />
        </button>
        <button
          v-if="id === 'quote'"
          class="layout-orientation layout-quote-style"
          :title="t('点击切换一言样式')"
          :aria-label="t('切换一言样式，当前为 {style}', { style: quoteStyle })"
          @pointerdown.stop
          @click.stop="
            setQuoteStyle(quoteStyle === 'simple' ? 'splash' : 'simple')
          "
        >
          <Type v-if="quoteStyle === 'simple'" :size="14" />
          <Sparkles v-else :size="14" />
        </button>
        <LayoutSlider
          v-if="id === 'quote'"
          :label="t('文字角度')"
          :min="-45"
          :max="45"
          :value="quoteAngle"
          unit="°"
          @change="setQuoteAngle"
          ><RotateCw :size="14"
        /></LayoutSlider>
        <LayoutSlider
          v-if="id === 'quote'"
          :label="t('字体大小')"
          :min="60"
          :max="160"
          :step="5"
          :value="quoteScale"
          unit="%"
          @change="setQuoteScale"
          ><ALargeSmall :size="15"
        /></LayoutSlider>
        <NewsSources
          v-if="id === 'news'"
          :sources="newsSources"
          @change="setNewsSources"
        />
        <LayoutSlider
          v-if="id === 'clock'"
          :label="t('文字角度')"
          :min="-45"
          :max="45"
          :value="clockLayout.angle"
          unit="°"
          @change="draftClock.angle = $event"
          ><RotateCw :size="14"
        /></LayoutSlider>
        <LayoutSlider
          v-if="id === 'clock'"
          :label="t('字体大小')"
          :min="60"
          :max="160"
          :step="5"
          :value="clockLayout.scale"
          unit="%"
          @change="draftClock.scale = $event"
          ><ALargeSmall :size="15"
        /></LayoutSlider>
        <button
          v-if="id === 'clock'"
          class="layout-orientation"
          :aria-label="t('显示日期')"
          :title="t('显示日期')"
          :aria-pressed="clockLayout.date"
          @pointerdown.stop
          @click.stop="draftClock.date = !draftClock.date"
        >
          <CalendarDays :size="14" />
        </button>
        <button
          v-if="id === 'news'"
          class="layout-orientation"
          :title="t(newsLayout.align === 'left' ? '文字靠右' : '文字靠左')"
          :aria-label="t(newsLayout.align === 'left' ? '文字靠右' : '文字靠左')"
          @pointerdown.stop
          @click.stop="toggleNewsAlignment"
        >
          <AlignLeft v-if="newsLayout.align === 'left'" :size="14" /><AlignRight
            v-else
            :size="14"
          />
        </button>
        <button
          v-if="id === 'news' && newsDock"
          class="layout-orientation"
          :title="t('取消边缘隐藏')"
          :aria-label="t('取消边缘隐藏')"
          @pointerdown.stop
          @click.stop="undockNews"
        >
          <PanelLeftClose
            v-if="newsDock === 'left'"
            :size="14"
          /><PanelRightClose v-else :size="14" />
        </button>
        <button
          v-if="removable(id)"
          class="layout-orientation"
          :title="t('移除 {name}', { name: t(labels[id]) })"
          :aria-label="t('移除 {name}', { name: t(labels[id]) })"
          @pointerdown.stop
          @click.stop="toggleWidget(id)"
        >
          <X :size="14" />
        </button>
        <button
          v-if="id === 'tools' && availableTools.length"
          class="layout-orientation"
          :aria-label="t('添加工具栏按钮')"
          :title="t('添加工具栏按钮')"
          :aria-expanded="toolsMenu"
          @pointerdown.stop
          @click.stop="toolsMenu = !toolsMenu"
        >
          <Plus :size="14" />
        </button>
        <div
          v-if="id === 'tools' && toolsMenu && availableTools.length"
          class="layout-tools-menu glass-surface"
          @pointerdown.stop
        >
          <button
            v-for="tool in availableTools"
            :key="tool"
            @click="setTools([...tools, tool])"
          >
            <component :is="toolIcons[tool]" :size="16" /><span>{{
              t(homeToolLabels[tool])
            }}</span>
          </button>
        </div>
      </div>
      <template v-if="editing && id === 'news'">
        <button
          v-for="edge in ['top', 'bottom'] as const"
          :key="edge"
          class="layout-height-handle"
          :data-edge="edge"
          :title="
            t(
              edge === 'top'
                ? '拖动上边缘调整 Trends 高度'
                : '拖动下边缘调整 Trends 高度',
            )
          "
          :aria-label="
            t(
              edge === 'top'
                ? '拖动上边缘调整 Trends 高度'
                : '拖动下边缘调整 Trends 高度',
            )
          "
          @pointerdown.stop="start($event, 'news', edge)"
          @keydown="nudgeNewsSize($event, edge)"
          @dragstart.prevent
        />
      </template>
    </div>
  </TransitionGroup>
</template>

<style>
.home-layout-item {
  position: fixed;
  z-index: 3;
  transition:
    left 280ms var(--motion-ease),
    top 280ms var(--motion-ease),
    width 280ms var(--motion-ease),
    height 280ms var(--motion-ease);
}
.home-layout-item:is(
    [data-layout-group="quote"],
    [data-layout-group="clock"]
  ):not(.layout-editing) {
  pointer-events: none;
}
/* Editing handles must stay above the mask even while hovered or focused. */
.home-layout-item:not(.layout-editing):is(:hover, :focus-within) {
  z-index: 40;
}
/* Content widgets stay below controls and their menus, even when revealed. */
.home-layout-item[data-home-widget="true"]:not(.layout-editing),
.home-layout-item[data-home-widget="true"]:not(.layout-editing):is(
    :hover,
    :focus-within
  ) {
  z-index: 2;
}
.home-layout-content {
  width: 100%;
  height: 100%;
}
.home-layout-item.news-docked {
  pointer-events: none;
}
.home-layout-item.news-docked.news-revealed {
  pointer-events: auto;
}
.news-docked > .home-layout-content {
  opacity: 0;
  pointer-events: none;
  transform: translateX(calc(-100% - 24px));
  transition:
    transform 360ms var(--motion-ease),
    opacity 220ms var(--motion-ease);
}
.news-docked[data-news-dock="right"] > .home-layout-content {
  transform: translateX(calc(100% + 24px));
}
.news-docked.news-revealed > .home-layout-content {
  opacity: 1;
  pointer-events: auto;
  transform: translateX(0);
}
.news-edge-trigger {
  position: absolute;
  left: -12px;
  top: 0;
  width: 12px;
  height: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--blue);
  pointer-events: auto;
  cursor: pointer;
}
[data-news-dock="right"] > .news-edge-trigger {
  left: auto;
  right: -12px;
}
.news-edge-trigger::after {
  content: "";
  position: absolute;
  left: 5px;
  top: calc(50% - 16px);
  width: 2px;
  height: 32px;
  border-radius: 2px;
  background: currentColor;
  opacity: 0.45;
  transition: opacity 160ms;
}
.news-edge-trigger:is(:hover, :focus-visible)::after {
  opacity: 1;
}
.news-edge-trigger:focus-visible {
  outline: none;
}
@media (prefers-reduced-motion: reduce) {
  .news-docked > .home-layout-content {
    transition: none;
  }
}
.home-layout-item .topbar {
  width: 100%;
  height: 64px;
}
.home-layout-item .global-tools {
  height: 100%;
}
.home-layout-item[data-orientation="vertical"] .global-tools {
  flex-direction: column;
  justify-content: center;
}
.home-layout-item .theme-picker {
  transform: none;
}
.home-layout-item .instance-dock {
  width: 100%;
  max-width: 100%;
  justify-content: flex-end;
}
.home-layout-item[data-layout-group="instances"] {
  z-index: 4;
}
.home-layout-item.layout-editing {
  z-index: 151;
}
/* Moving a whole dock is not a reorder: FLIP would make its tiles trail it. */
.home-layout-item.layout-editing .instance-sort-move {
  transition: none;
}
.home-layout-item[data-layout-group="tools"].layout-editing > .layout-handle {
  pointer-events: none;
}
.home-layout-item[data-layout-group="tools"].layout-editing
  > .home-layout-content {
  position: relative;
  z-index: 1;
}
.layout-label {
  z-index: 2;
}
.layout-tools-menu.glass-surface {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  min-width: 150px;
  padding: 6px;
  --glass-fill: color-mix(in srgb, var(--surface) 94%, transparent);
}
.home-layout-item[data-vertical="bottom"] .layout-tools-menu {
  top: auto;
  bottom: calc(100% + 8px);
}
.home-layout-item[data-horizontal="right"] .layout-tools-menu {
  left: auto;
  right: 0;
}
.layout-tools-menu button {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px;
  background: transparent;
  border: 0;
  color: var(--muted);
  text-align: left;
}
.layout-tools-menu button:hover,
.layout-tools-menu button:focus-visible {
  color: var(--blue);
  outline: none;
  text-shadow: 0 0 8px var(--glow);
}
.home-layout-item.layout-dragging {
  z-index: 152;
  transition: none;
}
.home-layout-content[inert] {
  pointer-events: none;
}
.home-layout-editor {
  position: fixed;
  inset: 0;
  z-index: 150;
  background: color-mix(in srgb, var(--night) 28%, transparent);
}
.layout-safe-area {
  position: absolute;
  inset: 12px;
  border: 1px dashed var(--glass-stroke);
  pointer-events: none;
}
.layout-handle {
  position: absolute;
  inset: -5px;
  border: 1px dashed color-mix(in srgb, var(--blue) 65%, transparent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--blue) 4%, transparent);
  color: var(--blue);
  cursor: grab;
  touch-action: none;
  user-select: none;
  padding: 0;
}
/* Keep the theme button and saved layout anchors in place; frame its centre. */
.home-layout-item[data-layout-group="themes"] .layout-handle {
  inset: 5px 1px 5px -11px;
}
.layout-label {
  position: absolute;
  left: -1px;
  top: -30px;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px 4px;
  background: var(--night);
  border-radius: 4px;
  font-size: 11px;
  white-space: nowrap;
  color: var(--blue);
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.layout-label > span {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-right: 2px;
}
.layout-label:focus {
  outline: none;
}
.home-layout-item[data-horizontal="right"] .layout-label {
  left: auto;
  right: -1px;
}
.home-layout-item[data-vertical="top"] .layout-label {
  top: auto;
  bottom: -30px;
}
.layout-label .layout-orientation {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  min-width: 0;
  flex: 0 0 20px;
  padding: 0;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--muted);
  transform: none;
  translate: none;
  scale: none;
  filter: none;
  transition: color 160ms;
}
.layout-label .layout-orientation:is(:hover, :focus-visible, :active) {
  outline: none;
  color: var(--blue);
  filter: none;
  transform: none;
  translate: none;
  scale: none;
}
.layout-handle:focus-visible,
.layout-handle:hover {
  outline: none;
  border-style: solid;
  box-shadow: 0 0 18px var(--glow);
}
.layout-dragging .layout-handle {
  cursor: grabbing;
  border-style: solid;
  box-shadow: 0 0 24px var(--glow);
}
.home-layout-dragging,
.home-layout-dragging * {
  user-select: none !important;
  -webkit-user-select: none !important;
  cursor: grabbing !important;
}
.home-layout-resizing,
.home-layout-resizing * {
  cursor: ns-resize !important;
}
.layout-height-handle {
  position: absolute;
  z-index: 2;
  left: 12px;
  right: 12px;
  top: -9px;
  height: 12px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--muted);
  cursor: ns-resize;
  touch-action: none;
}
.layout-height-handle[data-edge="bottom"] {
  top: auto;
  bottom: -9px;
}
.layout-height-handle::after {
  content: "";
  position: absolute;
  top: 5px;
  left: calc(50% - 16px);
  width: 32px;
  height: 2px;
  background: currentColor;
}
.layout-height-handle:is(:hover, :focus-visible),
.layout-resizing .layout-height-handle {
  color: var(--blue);
  outline: none;
  filter: none;
  transform: none;
}
.layout-guide {
  position: absolute;
  pointer-events: none;
  background: color-mix(in srgb, var(--blue) 40%, transparent);
}
.layout-guide.vertical {
  top: 12px;
  bottom: 12px;
  width: 1px;
}
.layout-guide.horizontal {
  left: 12px;
  right: 12px;
  height: 1px;
}
.layout-toolbar.glass-surface {
  position: fixed;
  z-index: 154;
  right: 12px;
  top: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 2px;
  width: 210px;
  height: 36px;
  padding: 3px 5px;
  --glass-fill: color-mix(in srgb, var(--surface) 35%, transparent);
  border: 1px dashed var(--glass-stroke);
  border-radius: 6px;
  box-shadow: var(--glass-rim);
  transition:
    background 160ms var(--motion-ease),
    opacity 160ms var(--motion-ease);
}
.layout-toolbar:hover {
  --glass-fill: color-mix(in srgb, var(--surface) 82%, transparent);
}
.layout-toolbar-title {
  flex: 1;
  min-width: 0;
  padding-inline: 5px;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
}
.layout-toolbar .icon-button {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
}
.layout-toolbar .layout-done {
  color: var(--blue);
}
.layout-editor-enter-active,
.layout-editor-leave-active {
  transition: opacity 160ms var(--motion-ease);
}
.layout-editor-enter-from,
.layout-editor-leave-to {
  opacity: 0;
}
.layout-editor-leave-active {
  pointer-events: none;
}
.shell.layout-editing .tool-window {
  visibility: hidden;
}
.shell > .workspace {
  grid-row: 2;
}
/* Menus stay attached to their group and unfold into the available space. */
.home-layout-item .account-dropdown {
  max-height: calc(100vh - var(--layout-y) - var(--layout-height) - 24px);
}
.home-layout-item[data-vertical="bottom"] .account-dropdown {
  top: auto;
  bottom: calc(100% + 10px);
  max-height: calc(var(--layout-y) - 24px);
  transform-origin: bottom left;
}
.home-layout-item .version-menu {
  right: auto;
  left: max(-232px, calc(12px - var(--layout-x) - var(--layout-width) + 40px));
  max-height: min(360px, calc(var(--layout-y) - 24px));
}
.home-layout-item[data-vertical="top"] .version-menu {
  bottom: auto;
  top: calc(100% + 12px);
  max-height: min(
    360px,
    calc(100vh - var(--layout-y) - var(--layout-height) - 24px)
  );
  transform-origin: top right;
}
.home-layout-item[data-vertical="top"] .version-trigger > svg {
  transform: rotate(180deg);
}
.home-layout-item .instance-overflow {
  max-height: min(360px, calc(var(--layout-y) - 24px));
}
.home-layout-item[data-vertical="top"] .instance-overflow {
  bottom: auto;
  top: calc(100% + 18px);
  max-height: min(
    360px,
    calc(100vh - var(--layout-y) - var(--layout-height) - 30px)
  );
}
.home-layout-item[data-vertical="top"]:not([data-orientation="vertical"])
  .instance-overflow-toggle:not(.expanded)
  > svg {
  transform: rotate(180deg);
}
.home-layout-item[data-vertical="top"]:not([data-orientation="vertical"])
  .instance-overflow-toggle.expanded
  > svg {
  transform: none;
}
.home-layout-item[data-horizontal="right"] .theme-options {
  left: auto;
  right: 100%;
  padding-left: 0;
  padding-right: 2px;
  flex-direction: row-reverse;
}
.home-layout-item[data-orientation="vertical"] .instance-dock {
  flex-direction: column;
  height: 100%;
}
.home-layout-item[data-orientation="vertical"]
  .instance-strip:not(.instance-overflow) {
  width: 64px;
  height: calc(var(--dock-columns) * 60px + 4px);
  max-height: 100%;
  min-height: 0;
  align-items: flex-start;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
}
.home-layout-item[data-orientation="vertical"]
  .instance-strip:not(.instance-overflow)
  .instance-track {
  grid-template-columns: 1fr;
  grid-auto-rows: 56px;
}
.home-layout-item[data-orientation="vertical"] .instance-dock .add-instance {
  width: 44px;
  height: 0;
  margin: 0;
  transition:
    height 160ms,
    margin-bottom 160ms,
    opacity 160ms;
}
.home-layout-item[data-orientation="vertical"]
  .instance-dock:is(:hover, :focus-within, .sorting, .empty)
  .add-instance {
  height: 44px;
  margin: 0 0 8px;
}
.home-layout-item[data-orientation="vertical"]
  .instance-dock.empty
  .add-instance {
  margin: 0;
}
.home-layout-item[data-orientation="vertical"] .instance-overflow-toggle {
  margin: 0 0 8px;
  --overflow-turn: 90deg;
}
.home-layout-item[data-orientation="vertical"][data-horizontal="right"]
  .instance-overflow-toggle {
  --overflow-turn: -90deg;
}
.home-layout-item[data-orientation="vertical"] .instance-overflow-toggle > svg {
  transform: rotate(var(--overflow-turn));
}
.home-layout-item[data-orientation="vertical"]
  .instance-overflow-toggle.expanded
  > svg {
  transform: rotate(calc(var(--overflow-turn) + 180deg));
}
.home-layout-item[data-orientation="vertical"] .instance-overflow {
  top: 0;
  bottom: auto;
  left: calc(100% + 12px);
  width: min(360px, calc(100vw - var(--layout-x) - var(--layout-width) - 24px));
  max-width: none;
  max-height: min(360px, calc(100vh - var(--layout-y) - 12px));
}
.home-layout-item[data-orientation="vertical"][data-horizontal="right"]
  .instance-overflow {
  left: auto;
  right: calc(100% + 12px);
  width: min(360px, calc(var(--layout-x) - 24px));
}
.home-layout-item[data-orientation="vertical"][data-vertical="bottom"]
  .instance-overflow {
  top: auto;
  bottom: 0;
  max-height: min(360px, calc(var(--layout-y) + var(--layout-height) - 12px));
}
.home-layout-item[data-orientation="vertical"]
  .instance-overflow
  .instance-track {
  grid-template-columns: repeat(auto-fit, minmax(48px, 1fr));
}
@media (hover: none) {
  .home-layout-item[data-orientation="vertical"] .instance-dock .add-instance {
    height: 44px;
    margin: 0 0 8px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .home-layout-item,
  .layout-toolbar.glass-surface,
  .layout-editor-enter-active,
  .layout-editor-leave-active {
    transition: none;
  }
}
</style>

<style>
.layout-widgets-menu.glass-surface {
  top: calc(100% + 8px);
  right: 0;
  left: auto;
  min-width: 180px;
}
.layout-widgets-menu button:disabled {
  opacity: 0.4;
  cursor: default;
}
.home-widget-enter-active,
.home-widget-leave-active {
  transition:
    opacity 180ms var(--motion-ease),
    translate 220ms var(--motion-ease);
}
.home-widget-enter-from,
.home-widget-leave-to {
  opacity: 0;
  translate: 0 8px;
}
.home-widget-leave-active {
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .home-widget-enter-active,
  .home-widget-leave-active {
    transition: none;
  }
}
</style>

<style>
/* Layout editing shows the full dock footprint without needing a hover. */
.home-layout-item.layout-editing[data-orientation="horizontal"]
  .instance-dock
  .add-instance {
  width: 44px;
  opacity: 1;
}
.home-layout-item.layout-editing[data-orientation="horizontal"]
  .instance-dock:not(.empty)
  .add-instance {
  margin-right: 8px;
}
.home-layout-item.layout-editing[data-orientation="vertical"]
  .instance-dock
  .add-instance {
  height: 44px;
  opacity: 1;
}
.home-layout-item.layout-editing[data-orientation="vertical"]
  .instance-dock:not(.empty)
  .add-instance {
  margin-bottom: 8px;
}
</style>

<style>
.layout-label .layout-quote-style {
  color: var(--blue);
}
</style>
