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
  GalleryVertical,
  Settings2,
  Terminal,
  Server,
  Map,
  ListTodo,
  Clock,
  CalendarDays,
  Palette,
  X,
} from "lucide-vue-next";
import LayoutSlider from "./LayoutSlider.vue";
import NewsSources from "./NewsSources.vue";
import { useEdgeWidget } from "../useEdgeWidget";
import {
  defaultInstanceWheelBounds,
  readInstanceWheelBounds,
  instanceWheelFrame,
  resizeInstanceWheelBounds,
  type InstanceWheelBounds,
} from "../instanceWheel";
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
  readListWidgetLayout,
  defaultNewsHeight,
  readClockLayout,
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
  resizeWidgetRect,
  inferHomeAttachments,
  readHomeAttachments,
  type WidgetResizeEdge,
  type HomeItem,
  type SavedHomePositions,
  type HomeRects,
  type Point,
  type InstanceLayout,
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
const editing = ref(false);
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
const savedWheelBounds = ref(readInstanceWheelBounds(stored));
const draftWheelBounds = ref({ ...savedWheelBounds.value });
const previewWheelBounds = ref<InstanceWheelBounds>();
const wheelBounds = computed(
  () =>
    previewWheelBounds.value ??
    (editing.value ? draftWheelBounds.value : savedWheelBounds.value),
);
const savedTools = ref(readHomeTools(stored));
const draftTools = ref([...savedTools.value]);
const toolsMenu = ref(false);
const widgetsMenu = ref(false);
const edgePanels = {
  news: useEdgeWidget(readNewsLayout(stored), editing),
  servers: useEdgeWidget(readListWidgetLayout(stored, "servers"), editing),
  worlds: useEdgeWidget(readListWidgetLayout(stored, "worlds"), editing),
};
const listDocks = computed(() => ({
  servers: edgePanels.servers.dock.value,
  worlds: edgePanels.worlds.dock.value,
}));
const listHeights = computed(() => ({
  servers: edgePanels.servers.layout.value.height,
  worlds: edgePanels.worlds.layout.value.height,
}));
const {
  saved: savedNewsLayout,
  draft: draftNewsLayout,
  layout: newsLayout,
  dock: newsDock,
} = edgePanels.news;
function edgePanel(id: HomeItem) {
  return id === "news" || id === "servers" || id === "worlds"
    ? edgePanels[id]
    : undefined;
}
function dockCandidate(id: HomeItem) {
  return edgePanel(id)?.candidate(rects.value[id], area.value);
}
function closeEdgePanels() {
  Object.values(edgePanels).forEach((panel) => panel.close());
}
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
const dragging = ref<HomeItem>();
const resizingPanel = ref(false);
const resizingWheel = ref(false);
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
    listHeights.value,
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
      [],
      newsDock.value,
      hiddenGroups.value,
      listDocks.value,
      listHeights.value,
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
const wheelFrame = computed(() => {
  const box = rects.value.instances;
  return instanceWheelFrame(
    { left: box.x, top: box.y, width: box.width, height: box.height },
    area.value,
    wheelBounds.value,
  );
});
const wheelBoundaryStyle = computed(() => ({
  left: `${wheelFrame.value.left}px`,
  top: `${wheelFrame.value.top}px`,
  width: `${wheelFrame.value.width}px`,
  height: `${wheelFrame.value.height}px`,
}));
let pointer:
  | {
      id: number;
      group: HomeItem;
      origin: Point;
      base: HomeRects;
      target: HTMLElement;
      resizeEdge?: WidgetResizeEdge;
      wheelBounds: InstanceWheelBounds;
    }
  | undefined;
let pending: Point | undefined;
let frame: number | undefined;
let disposed = false;
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
  draftWheelBounds.value = { ...savedWheelBounds.value };
  draftTools.value = [...savedTools.value];
  draftWidgets.value = [...savedWidgets.value];
  draftQuoteStyle.value = savedQuoteStyle.value;
  draftQuoteAngles.value = { ...savedQuoteAngles.value };
  draftQuoteScales.value = { ...savedQuoteScales.value };
  draftNewsSources.value = [...savedNewsSources.value];
  for (const panel of Object.values(edgePanels))
    panel.draft.value = { ...panel.saved.value };
  closeEdgePanels();
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
  resizingPanel.value = false;
  resizingWheel.value = false;
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
  previewWheelBounds.value = undefined;
  Object.values(edgePanels).forEach((panel) => {
    panel.previewDock.value = undefined;
  });
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
    savedWheelBounds.value = { ...draftWheelBounds.value };
    savedTools.value = [...draftTools.value];
    savedWidgets.value = [...draftWidgets.value];
    savedQuoteStyle.value = draftQuoteStyle.value;
    savedQuoteAngles.value = { ...draftQuoteAngles.value };
    savedQuoteScales.value = { ...draftQuoteScales.value };
    savedNewsSources.value = [...draftNewsSources.value];
    for (const panel of Object.values(edgePanels))
      panel.saved.value = { ...panel.draft.value };
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
        edgePanels.servers.saved.value.align !== "left" ||
        edgePanels.servers.saved.value.dock !== null ||
        edgePanels.servers.saved.value.height !== null ||
        edgePanels.worlds.saved.value.align !== "left" ||
        edgePanels.worlds.saved.value.dock !== null ||
        edgePanels.worlds.saved.value.height !== null ||
        savedClock.value.scale !== 100 ||
        savedClock.value.angle !== 0 ||
        savedWheelBounds.value.above !== defaultInstanceWheelBounds.above ||
        savedWheelBounds.value.below !== defaultInstanceWheelBounds.below ||
        !savedClock.value.date ||
        Object.values(savedOrientations.value).some(
          (value) => value !== "horizontal",
        ) ||
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
            instanceWheel: savedWheelBounds.value,
            tools: savedTools.value,
            widgets: savedWidgets.value,
            quoteStyle: savedQuoteStyle.value,
            quoteAngles: savedQuoteAngles.value,
            quoteScales: savedQuoteScales.value,
            news: savedNewsLayout.value,
            servers: edgePanels.servers.saved.value,
            worlds: edgePanels.worlds.saved.value,
            clock: savedClock.value,
          }),
        );
      else localStorage.removeItem(HOME_LAYOUT_KEY);
    } catch {
      /* Storage may be disabled; retain the layout for this session. */
    }
  }
  editing.value = false;
  closeEdgePanels();
  emit("editing", false);
}
function reset() {
  cancelDrag();
  draft.value = null;
  draftHiddenGroups.value = [];
  draftClock.value = readClockLayout(null);
  draftWheelBounds.value = { ...defaultInstanceWheelBounds };
  draftAttachments.value = {};
  draftOrientations.value = { ...defaultHomeOrientations };
  draftTools.value = [...defaultHomeTools];
  draftWidgets.value = [];
  draftQuoteStyle.value = "simple";
  draftQuoteAngles.value = { ...defaultQuoteAngles };
  draftQuoteScales.value = { ...defaultQuoteScales };
  draftNewsSources.value = readFeedSources(null);
  draftNewsLayout.value = readNewsLayout(null);
  edgePanels.servers.draft.value = readListWidgetLayout(null, "servers");
  edgePanels.worlds.draft.value = readListWidgetLayout(null, "worlds");
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
      [],
      newsDock.value,
      hiddenGroups.value,
      listDocks.value,
      listHeights.value,
    ),
  );
  toolsMenu.value = false;
  widgetsMenu.value = false;
}
function nextOrientation(group: keyof HomeOrientations): InstanceLayout {
  const current = orientations.value[group];
  return current === "horizontal"
    ? "vertical"
    : group === "instances" && current === "vertical"
      ? "wheel"
      : "horizontal";
}
function orientationHint(group: keyof HomeOrientations) {
  const next = nextOrientation(group);
  return next === "wheel"
    ? t("切换为滚轮选择器")
    : next === "vertical"
      ? t("切换为竖排")
      : t("切换为横排");
}
function setOrientation(group: keyof HomeOrientations, value: InstanceLayout) {
  if (!editing.value || orientations.value[group] === value) return;
  if (group === "tools" && value === "wheel") return;
  cancelDrag();
  toolsMenu.value = false;
  widgetsMenu.value = false;
  const before = rects.value;
  draftOrientations.value = { ...draftOrientations.value, [group]: value };
  const size = sizes.value[group];
  // Keep the instance dock's nearest edge anchored while its footprint changes;
  // cycling through vertical must not pull the compact wheel away from Launch.
  // Tools retain their existing centre when rotating.
  const xShift =
    group === "instances"
      ? before[group].x + before[group].width / 2 > area.value.width / 2
        ? before[group].width - size.width
        : 0
      : (before[group].width - size.width) / 2;
  const yShift =
    group === "instances"
      ? before[group].y + before[group].height / 2 > area.value.height / 2
        ? before[group].height - size.height
        : 0
      : (before[group].height - size.height) / 2;
  const arranged = arrangeHomeLayout(
    {
      ...before,
      [group]: {
        ...size,
        x: before[group].x + xShift,
        y: before[group].y + yShift,
      },
    },
    area.value,
    group,
    [],
    newsDock.value,
    hiddenGroups.value,
    listDocks.value,
    listHeights.value,
  );
  rememberLayout(arranged);
}
function preventSelection(event: Event) {
  event.preventDefault();
}
function start(
  event: PointerEvent,
  group: HomeItem,
  resizeEdge?: WidgetResizeEdge,
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
    wheelBounds: { ...wheelBounds.value },
  };
  dragging.value = group;
  resizingPanel.value = !!resizeEdge && !!edgePanel(group);
  resizingWheel.value = !!resizeEdge && group === "instances";
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
  edgePanel(group)?.move(proposed, area.value);
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
    [],
    newsDock.value,
    hiddenGroups.value,
    listDocks.value,
    listHeights.value,
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
    if (group === "instances") {
      const box = base.instances;
      previewWheelBounds.value = resizeInstanceWheelBounds(
        pointer.wheelBounds,
        pointer.resizeEdge,
        pending.y - origin.y,
        { left: box.x, top: box.y, width: box.width, height: box.height },
        area.value,
      );
      return;
    }
    preview.value = {
      ...base,
      [group]: resizeWidgetRect(
        base[group]!,
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
    if (pointer.resizeEdge && pointer.group === "instances")
      draftWheelBounds.value = { ...previewWheelBounds.value! };
    else if (pointer.resizeEdge) {
      const panel = edgePanel(pointer.group)!;
      panel.draft.value = {
        ...panel.draft.value,
        height: preview.value![pointer.group]!.height,
      };
    } else edgePanel(pointer.group)?.commitMove();
  }
  stopPointer();
  preview.value = undefined;
  previewWheelBounds.value = undefined;
  Object.values(edgePanels).forEach((panel) => {
    panel.previewDock.value = undefined;
  });
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
  const panel = edgePanel(group);
  // A docked panel is pinned at the inset; let keyboard movement detach it
  // without repeatedly losing its progress inside the pointer hysteresis zone.
  if (
    panel &&
    ((panel.dock.value === "left" && event.key === "ArrowRight") ||
      (panel.dock.value === "right" && event.key === "ArrowLeft"))
  ) {
    const x =
      panel.dock.value === "left" ? 36 : area.value.width - origin.width - 36;
    panel.draft.value = { ...panel.draft.value, dock: null };
    rememberLayout({ ...rects.value, [group]: { ...origin, x } });
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
  panel?.commitMove();
  preview.value = undefined;
  Object.values(edgePanels).forEach((panel) => {
    panel.previewDock.value = undefined;
  });
  guides.value = {};
}
function nudgeWidgetSize(
  event: KeyboardEvent,
  id: HomeItem,
  edge: WidgetResizeEdge,
) {
  const panel = edgePanel(id);
  if (!editing.value || !panel || !["ArrowUp", "ArrowDown"].includes(event.key))
    return;
  event.preventDefault();
  const delta = (event.key === "ArrowUp" ? -1 : 1) * (event.shiftKey ? 1 : 8);
  const before = rects.value;
  const box = resizeWidgetRect(before[id]!, edge, delta, area.value);
  panel.draft.value = { ...panel.draft.value, height: box.height };
  rememberLayout({ ...before, [id]: box });
}
function nudgeWheelSize(event: KeyboardEvent, edge: WidgetResizeEdge) {
  if (!editing.value || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
  event.preventDefault();
  const box = rects.value.instances;
  draftWheelBounds.value = resizeInstanceWheelBounds(
    wheelBounds.value,
    edge,
    (event.key === "ArrowUp" ? -1 : 1) * (event.shiftKey ? 1 : 8),
    { left: box.x, top: box.y, width: box.width, height: box.height },
    area.value,
  );
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
  window.addEventListener("blur", closeEdgePanels);
});
onBeforeUnmount(() => {
  disposed = true;
  cancelDrag();
  window.removeEventListener("resize", resize);
  document.removeEventListener("keydown", keyboard);
  window.removeEventListener("blur", closeEdgePanels);
  closeEdgePanels();
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
      :ref="edgePanel(id)?.setHost"
      class="home-layout-item"
      :data-layout-group="id"
      :data-home-widget="
        homeWidgets.some((widget) => widget === id) ? 'true' : undefined
      "
      :data-orientation="orientation(id)"
      :class="{
        'layout-editing': editing,
        'layout-dragging': dragging === id,
        'layout-resizing':
          (dragging === id && resizingPanel) ||
          (id === 'instances' && resizingWheel),
        'widget-docked': edgePanel(id)?.dock.value && !editing,
        'widget-revealed': edgePanel(id)?.open.value,
      }"
      :data-edge-dock="edgePanel(id)?.dock.value"
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
      @pointerenter="edgePanel(id)?.show()"
      @pointerleave="edgePanel(id)?.leave()"
      @focusin="edgePanel(id)?.show()"
      @focusout="edgePanel(id)?.blur($event)"
      @keydown.esc="edgePanel(id)?.escape($event)"
    >
      <button
        v-if="edgePanel(id)?.dock.value && !editing"
        class="widget-edge-trigger"
        :aria-label="t('展开 {name}', { name: t(labels[id]) })"
        :aria-expanded="edgePanel(id)?.open.value"
        @click="edgePanel(id)?.show()"
      />
      <div
        class="home-layout-content"
        :inert="
          (editing && id !== 'tools') ||
          (!!edgePanel(id)?.dock.value &&
            !editing &&
            !edgePanel(id)?.open.value)
        "
      >
        <slot
          :name="id"
          :orientation="orientation(id)"
          :instance-wheel-bounds="wheelBounds"
          :editing="editing"
          :quote-style="quoteStyle"
          :quote-angle="quoteAngle"
          :quote-scale="quoteScale"
          :news-sources="newsSources"
          :news-align="newsLayout.align"
          :widget-align="edgePanel(id)?.layout.value.align"
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
          :title="orientationHint(id)"
          :aria-label="orientationHint(id)"
          @pointerdown.stop
          @click.stop="setOrientation(id, nextOrientation(id))"
        >
          <GalleryVertical v-if="orientation(id) === 'wheel'" :size="14" />
          <Columns3
            v-else-if="orientation(id) === 'horizontal'"
            :size="14"
          /><Rows3 v-else :size="14" />
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
          v-if="edgePanel(id)"
          class="layout-orientation"
          :title="
            t(
              edgePanel(id)?.layout.value.align === 'left'
                ? '文字靠右'
                : '文字靠左',
            )
          "
          :aria-label="
            t(
              edgePanel(id)?.layout.value.align === 'left'
                ? '文字靠右'
                : '文字靠左',
            )
          "
          @pointerdown.stop
          @click.stop="edgePanel(id)?.toggleAlignment()"
        >
          <AlignLeft
            v-if="edgePanel(id)?.layout.value.align === 'left'"
            :size="14"
          /><AlignRight v-else :size="14" />
        </button>
        <button
          v-if="dockCandidate(id)"
          class="layout-orientation"
          :title="
            t(edgePanel(id)?.dock.value ? '取消边缘隐藏' : '启用边缘隐藏')
          "
          :aria-label="
            t(edgePanel(id)?.dock.value ? '取消边缘隐藏' : '启用边缘隐藏')
          "
          :aria-pressed="!!edgePanel(id)?.dock.value"
          @pointerdown.stop
          @click.stop="edgePanel(id)?.toggleDock(rects[id], area)"
        >
          <PanelLeftClose
            v-if="dockCandidate(id) === 'left'"
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
      <div
        v-if="editing && id === 'instances' && orientation(id) === 'wheel'"
        class="layout-wheel-boundaries"
        :style="wheelBoundaryStyle"
      >
        <button
          v-for="edge in ['top', 'bottom'] as const"
          :key="edge"
          class="layout-wheel-boundary"
          :data-edge="edge"
          :title="
            t(
              edge === 'top'
                ? '拖动调整实例列表上边界'
                : '拖动调整实例列表下边界',
            )
          "
          :aria-label="
            t(
              edge === 'top'
                ? '拖动调整实例列表上边界'
                : '拖动调整实例列表下边界',
            )
          "
          @pointerdown="start($event, 'instances', edge)"
          @keydown="nudgeWheelSize($event, edge)"
          @dragstart.prevent
        />
      </div>
      <template v-if="editing && edgePanel(id)">
        <button
          v-for="edge in ['top', 'bottom'] as const"
          :key="edge"
          class="layout-height-handle"
          :data-edge="edge"
          :title="
            t(
              edge === 'top'
                ? '拖动上边缘调整 {name} 高度'
                : '拖动下边缘调整 {name} 高度',
              { name: t(labels[id]) },
            )
          "
          :aria-label="
            t(
              edge === 'top'
                ? '拖动上边缘调整 {name} 高度'
                : '拖动下边缘调整 {name} 高度',
              { name: t(labels[id]) },
            )
          "
          @pointerdown.stop="start($event, id, edge)"
          @keydown="nudgeWidgetSize($event, id, edge)"
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
.home-layout-item.widget-docked {
  pointer-events: none;
}
.home-layout-item.widget-docked.widget-revealed {
  pointer-events: auto;
}
.widget-docked > .home-layout-content {
  opacity: 0;
  pointer-events: none;
  transform: translateX(calc(-100% - 24px));
  transition:
    transform 360ms var(--motion-ease),
    opacity 220ms var(--motion-ease);
}
.widget-docked[data-edge-dock="right"] > .home-layout-content {
  transform: translateX(calc(100% + 24px));
}
.widget-docked.widget-revealed > .home-layout-content {
  opacity: 1;
  pointer-events: auto;
  transform: translateX(0);
}
.widget-edge-trigger {
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
[data-edge-dock="right"] > .widget-edge-trigger {
  left: auto;
  right: -12px;
}
.widget-edge-trigger::after {
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
.widget-edge-trigger:is(:hover, :focus-visible)::after {
  opacity: 1;
}
.widget-edge-trigger:focus-visible {
  outline: none;
}
@media (prefers-reduced-motion: reduce) {
  .widget-docked > .home-layout-content {
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
.home-layout-item.layout-editing:where(
    :focus-within,
    :has(.layout-orientation[aria-expanded="true"])
  ) {
  z-index: 152;
}
.home-layout-item.layout-editing:hover {
  z-index: 153;
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
.home-layout-item.layout-editing.layout-dragging {
  z-index: 153;
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
  /* Keep glyphs on one raster layer when a slider's glass layer is created. */
  transform: translateZ(0);
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
.layout-label .layout-orientation[aria-pressed="true"],
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
.layout-wheel-boundaries {
  position: absolute;
  z-index: 2;
  pointer-events: none;
}
.layout-wheel-boundary {
  position: absolute;
  left: 0;
  top: -8px;
  width: 100%;
  height: 16px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--muted);
  pointer-events: auto;
  cursor: ns-resize;
  touch-action: none;
}
.layout-wheel-boundary[data-edge="bottom"] {
  top: auto;
  bottom: -8px;
}
.layout-wheel-boundary::before {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: 7px;
  border-top: 1px dashed currentColor;
  opacity: 0.45;
}
.layout-wheel-boundary::after {
  content: "";
  position: absolute;
  left: calc(50% - 14px);
  top: 6px;
  width: 28px;
  height: 3px;
  background: currentColor;
}
.layout-wheel-boundary:is(:hover, :focus-visible),
.layout-resizing .layout-wheel-boundary {
  outline: none;
  transform: none;
  color: var(--blue);
  filter: drop-shadow(0 0 4px var(--glow));
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
