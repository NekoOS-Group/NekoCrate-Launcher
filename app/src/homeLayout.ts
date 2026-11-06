import {
  homeWidgets,
  defaultNewsHeight,
  minNewsHeight,
  type HomeWidget,
  type NewsDock,
} from "./homeWidgets.ts";
export const homeGroups = [
  "account",
  "tools",
  "instances",
  "themes",
  "launch",
] as const;
export type HomeGroup = (typeof homeGroups)[number];
export const removableHomeGroups = ["themes", "instances"] as const;
export type RemovableHomeGroup = (typeof removableHomeGroups)[number];
export function readHiddenHomeGroups(
  value: string | null,
): RemovableHomeGroup[] {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1 && Array.isArray(data.hiddenGroups))
      return removableHomeGroups.filter((id) => data.hiddenGroups.includes(id));
  } catch {
    /* Older layouts keep the original controls visible. */
  }
  return [];
}
export type HomeItem = HomeGroup | HomeWidget;
export type Point = { x: number; y: number };
export type Rect = Point & { width: number; height: number };
export type Area = { width: number; height: number };
export type HomePositions = Record<HomeGroup, Point> &
  Partial<Record<HomeWidget, Point>>;
// Layouts saved before tools became movable keep their original account anchor.
export type SavedHomePositions = Omit<HomePositions, "tools"> & {
  tools?: Point;
};
export type HomeRects = Record<HomeGroup, Rect> &
  Partial<Record<HomeWidget, Rect>>;
export type HomeOrientation = "horizontal" | "vertical";
export type HomeOrientations = Record<"tools" | "instances", HomeOrientation>;
type Attachment = {
  side: "left" | "right" | "top" | "bottom";
  gap: number;
  offset: number;
};
export type HomeAttachments = Partial<
  Record<"tools" | "instances", Attachment>
>;
const attachmentParents = { tools: "account", instances: "launch" } as const;
export const defaultHomeOrientations: HomeOrientations = {
  tools: "horizontal",
  instances: "horizontal",
};
export const HOME_LAYOUT_KEY = "nekocrate.home-layout.v1";
const margin = 12,
  gap = 12;

export function homeSizes(
  area: Area,
  slots: number,
  orientations: HomeOrientations = defaultHomeOrientations,
  toolCount = 3,
  quoteScale = 1,
  newsHeight = defaultNewsHeight,
  clockScale = 1,
) {
  const edge = area.width <= 1000 || area.height <= 660 ? 24 : 32;
  const compactWidgets = area.height <= 600;
  const toolLength = Math.max(44, toolCount * 50 - 6);
  return {
    account: { width: 224, height: 64 },
    tools:
      orientations.tools === "vertical"
        ? { width: 64, height: toolLength }
        : { width: toolLength, height: 64 },
    instances:
      orientations.instances === "vertical"
        ? {
            width: 64,
            height: Math.min(
              slots ? slots * 60 + 56 : 64,
              Math.max(64, area.height - edge * 2 - 76),
            ),
          }
        : {
            width: Math.max(
              44,
              Math.min(
                slots ? slots * 60 + 56 : 44,
                area.width - edge * 2 - 36 - 16 - 206 - 12,
              ),
            ),
            height: 64,
          },
    themes: { width: 44, height: 64 },
    launch: { width: 206, height: 56 },
    quote: { width: 300 * quoteScale, height: 112 * quoteScale },
    news: {
      width: 300,
      height: Math.min(newsHeight, Math.max(1, area.height - margin * 2)),
    },
    servers: { width: 244, height: compactWidgets ? 164 : 214 },
    favorites: { width: 244, height: compactWidgets ? 104 : 124 },
    worlds: { width: 250, height: compactWidgets ? 132 : 190 },
    todos: { width: 244, height: compactWidgets ? 120 : 174 },
    clock: { width: 156 * clockScale, height: 66 * clockScale },
  } satisfies Record<HomeItem, Area>;
}
export function defaultHomeLayout(
  area: Area,
  sizes: Record<HomeItem, Area>,
  widgets: HomeWidget[] = [],
): HomeRects {
  const edge = area.width <= 1000 || area.height <= 660 ? 24 : 32;
  const widgetPosition: Record<HomeWidget, Point> = {
    quote: { x: edge, y: edge + 112 },
    news: { x: area.width - edge - sizes.news.width, y: edge + 64 },
    favorites: { x: edge, y: edge + 100 },
    servers: { x: edge, y: edge + sizes.favorites.height + 126 },
    worlds: { x: area.width - edge - sizes.worlds.width, y: edge + 110 },
    todos: {
      x: area.width - edge - sizes.todos.width,
      y: edge + sizes.worlds.height + 126,
    },
    clock: { x: area.width - edge - sizes.clock.width, y: edge },
  };
  return {
    ...Object.fromEntries(
      widgets.map((id) => [
        id,
        {
          ...sizes[id],
          ...widgetPosition[id],
        },
      ]),
    ),
    account: { ...sizes.account, x: edge, y: edge },
    tools: { ...sizes.tools, x: edge + sizes.account.width + gap, y: edge },
    instances: {
      ...sizes.instances,
      x: area.width - edge - sizes.launch.width - 12 - sizes.instances.width,
      y: area.height - edge - sizes.instances.height,
    },
    themes: { ...sizes.themes, x: edge - 6, y: area.height - edge - 46 },
    launch: {
      ...sizes.launch,
      x: area.width - edge - sizes.launch.width,
      y: area.height - edge - 60,
    },
  };
}
function clamp(value: number, max: number) {
  return Math.max(margin, Math.min(value, Math.max(margin, max)));
}
export function fitHomeRect(rect: Rect, area: Area): Rect {
  const width = Math.min(rect.width, Math.max(1, area.width - margin * 2));
  const height = Math.min(rect.height, Math.max(1, area.height - margin * 2));
  return {
    width,
    height,
    x: clamp(rect.x, area.width - width - margin),
    y: clamp(rect.y, area.height - height - margin),
  };
}
export function overlaps(a: Rect, b: Rect) {
  return (
    a.x < b.x + b.width + gap - 0.01 &&
    a.x + a.width + gap > b.x + 0.01 &&
    a.y < b.y + b.height + gap - 0.01 &&
    a.y + a.height + gap > b.y + 0.01
  );
}
function closestSpace(rect: Rect, occupied: Rect[], area: Area) {
  const origin = fitHomeRect(rect, area);
  if (occupied.every((other) => !overlaps(origin, other))) return origin;
  const xs = [origin.x, margin, area.width - origin.width - margin];
  const ys = [origin.y, margin, area.height - origin.height - margin];
  for (const other of occupied) {
    xs.push(other.x - origin.width - gap, other.x + other.width + gap);
    ys.push(other.y - origin.height - gap, other.y + other.height + gap);
  }
  let best: Rect | undefined,
    distance = Infinity;
  for (const x of xs)
    for (const y of ys) {
      const candidate = fitHomeRect({ ...origin, x, y }, area);
      if (occupied.some((other) => overlaps(candidate, other))) continue;
      const score =
        (candidate.x - origin.x) ** 2 + (candidate.y - origin.y) ** 2;
      if (score < distance) {
        best = candidate;
        distance = score;
      }
    }
  return best || origin;
}
/** Interactive groups avoid each other; text overlays only avoid editor chrome. */
export function arrangeHomeLayout(
  requested: HomeRects,
  area: Area,
  active?: HomeItem,
  obstacles: Rect[] = [],
  newsDock: NewsDock = null,
  hidden: readonly HomeGroup[] = [],
): HomeRects {
  const order: HomeItem[] = [
    "account",
    "tools",
    "launch",
    "themes",
    "instances",
    ...homeWidgets.filter((id) => requested[id]),
  ];
  if (active) order.unshift(...order.splice(order.indexOf(active), 1));
  const result = {} as HomeRects,
    occupied = [...obstacles];
  for (const id of order) {
    if (hidden.some((group) => group === id)) {
      result[id] = fitHomeRect(requested[id]!, area);
      continue;
    }
    if (id === "news" && newsDock) {
      // Edge panels do not push controls away when expanded or resized.
      const rect = fitHomeRect(requested[id]!, area);
      result[id] = {
        ...rect,
        x: newsDock === "left" ? margin : area.width - margin - rect.width,
      };
      continue;
    }
    // Text widgets may overlap controls without displacing them. Trends can
    // grow vertically; its resize edge must not cause a positional jump.
    const overlay = ["quote", "news", "clock"].includes(id);
    result[id] = closestSpace(
      requested[id]!,
      id === "news" ? [] : overlay ? obstacles : occupied,
      area,
    );
    if (!overlay) occupied.push(result[id]!);
  }
  return result;
}
export function newsDockTarget(
  rect: Rect,
  area: Area,
  current: NewsDock = null,
): NewsDock {
  // Match the visible 12px inset plus snapHomeRect's 10px capture distance.
  // Requiring overshoot outside the window made a visibly snapped panel stay open.
  if (rect.x <= margin + 10) return "left";
  if (rect.x + rect.width >= area.width - margin - 10) return "right";
  // A small release zone avoids flapping between docked and undocked states.
  if (current === "left" && rect.x < 36) return "left";
  if (current === "right" && rect.x + rect.width > area.width - 36)
    return "right";
  return null;
}
export type NewsResizeEdge = "top" | "bottom";
export function resizeNewsRect(
  rect: Rect,
  edge: NewsResizeEdge,
  delta: number,
  area: Area,
): Rect {
  const base = fitHomeRect(rect, area);
  const minimum = Math.min(minNewsHeight, base.height);
  if (edge === "top") {
    const bottom = base.y + base.height;
    const y = Math.max(margin, Math.min(bottom - minimum, base.y + delta));
    return { ...base, y, height: bottom - y };
  }
  return {
    ...base,
    height: Math.max(
      minimum,
      Math.min(area.height - margin - base.y, base.height + delta),
    ),
  };
}
/** Adjacent pairs keep their gap on resize; deliberately separated controls stay independent. */
export function inferHomeAttachments(
  rects: HomeRects,
  hidden: readonly HomeGroup[] = [],
): HomeAttachments {
  const result: HomeAttachments = {};
  for (const child of ["tools", "instances"] as const) {
    if (hidden.includes(child)) continue;
    const a = rects[attachmentParents[child]],
      b = rects[child];
    const dx = b.x + b.width / 2 - a.x - a.width / 2;
    const dy = b.y + b.height / 2 - a.y - a.height / 2;
    const candidates: Attachment[] = [
      { side: "left", gap: a.x - b.x - b.width, offset: dy },
      { side: "right", gap: b.x - a.x - a.width, offset: dy },
      { side: "top", gap: a.y - b.y - b.height, offset: dx },
      { side: "bottom", gap: b.y - a.y - a.height, offset: dx },
    ];
    const candidate = candidates.find(
      (item) =>
        item.gap >= gap - 0.01 && item.gap <= 32 && Math.abs(item.offset) <= 16,
    );
    if (candidate) result[child] = candidate;
  }
  return result;
}
export function readHomeAttachments(
  value: string | null,
): HomeAttachments | null {
  try {
    const data = JSON.parse(value || "null");
    if (
      data?.version !== 1 ||
      !data.attachments ||
      typeof data.attachments !== "object"
    )
      return null;
    const result: HomeAttachments = {};
    for (const child of ["tools", "instances"] as const) {
      const item = data.attachments[child];
      if (
        item &&
        ["left", "right", "top", "bottom"].includes(item.side) &&
        Number.isFinite(item.gap) &&
        item.gap >= gap - 0.01 &&
        item.gap <= 32 &&
        Number.isFinite(item.offset) &&
        Math.abs(item.offset) <= 16
      )
        result[child] = { side: item.side, gap: item.gap, offset: item.offset };
    }
    return result;
  } catch {
    return null;
  }
}
function attachHomePairs(
  rects: HomeRects,
  area: Area,
  attachments: HomeAttachments,
  hidden: readonly HomeGroup[],
): HomeRects {
  for (const child of ["tools", "instances"] as const) {
    if (hidden.includes(child)) continue;
    const link = attachments[child];
    if (!link) continue;
    const parent = attachmentParents[child],
      a = rects[parent],
      b = { ...rects[child] };
    const horizontal = link.side === "left" || link.side === "right";
    b.x = horizontal
      ? link.side === "left"
        ? a.x - b.width - link.gap
        : a.x + a.width + link.gap
      : a.x + (a.width - b.width) / 2 + link.offset;
    b.y = horizontal
      ? a.y + (a.height - b.height) / 2 + link.offset
      : link.side === "top"
        ? a.y - b.height - link.gap
        : a.y + a.height + link.gap;
    const left = Math.min(a.x, b.x),
      top = Math.min(a.y, b.y);
    const bounds = {
      x: left,
      y: top,
      width: Math.max(a.x + a.width, b.x + b.width) - left,
      height: Math.max(a.y + a.height, b.y + b.height) - top,
    };
    // When the pair no longer fits, normal collision handling takes over.
    if (
      bounds.width > area.width - margin * 2 ||
      bounds.height > area.height - margin * 2
    )
      continue;
    const fitted = fitHomeRect(bounds, area),
      dx = fitted.x - left,
      dy = fitted.y - top;
    rects[parent] = { ...a, x: a.x + dx, y: a.y + dy };
    rects[child] = { ...b, x: b.x + dx, y: b.y + dy };
  }
  return rects;
}
export function normalizeHomeLayout(
  rects: HomeRects,
  area: Area,
): HomePositions {
  return Object.fromEntries(
    (Object.keys(rects) as HomeItem[]).map((id) => [
      id,
      {
        x: Math.max(
          0,
          Math.min(
            1,
            (rects[id]!.x - margin) /
              Math.max(1, area.width - rects[id]!.width - margin * 2),
          ),
        ),
        y: Math.max(
          0,
          Math.min(
            1,
            (rects[id]!.y - margin) /
              Math.max(1, area.height - rects[id]!.height - margin * 2),
          ),
        ),
      },
    ]),
  ) as HomePositions;
}
export function expandHomeLayout(
  saved: SavedHomePositions | null,
  area: Area,
  sizes: Record<HomeItem, Area>,
  widgets: HomeWidget[] = [],
  attachments: HomeAttachments = {},
  hidden: readonly HomeGroup[] = [],
): HomeRects {
  if (!saved) return defaultHomeLayout(area, sizes, widgets);
  const defaults = defaultHomeLayout(area, sizes, widgets);
  const expand = (
    id: HomeItem,
    position: Point,
    width = sizes[id].width,
  ): Rect => ({
    ...sizes[id],
    x: margin + position.x * Math.max(0, area.width - width - margin * 2),
    y:
      margin +
      position.y * Math.max(0, area.height - sizes[id].height - margin * 2),
  });
  const account = expand(
    "account",
    saved.account,
    saved.tools
      ? sizes.account.width
      : sizes.account.width + gap + sizes.tools.width,
  );
  return attachHomePairs(
    {
      ...Object.fromEntries(
        widgets.map((id) => [
          id,
          saved[id] ? expand(id, saved[id]!) : defaults[id],
        ]),
      ),
      account,
      tools: saved.tools
        ? expand("tools", saved.tools)
        : { ...sizes.tools, x: account.x + account.width + gap, y: account.y },
      instances: expand("instances", saved.instances),
      themes: expand("themes", saved.themes),
      launch: expand("launch", saved.launch),
    },
    area,
    attachments,
    hidden,
  );
}
export function readHomeLayout(
  value: string | null,
): SavedHomePositions | null {
  try {
    const data = JSON.parse(value || "null");
    const groups = [
      ...homeGroups,
      ...homeWidgets.filter((id) => data?.positions?.[id] !== undefined),
    ].filter((id) => id !== "tools" || data?.positions?.tools !== undefined);
    if (
      data?.version !== 1 ||
      !groups.every((id) =>
        [data.positions?.[id]?.x, data.positions?.[id]?.y].every(
          (n) =>
            typeof n === "number" && Number.isFinite(n) && n >= 0 && n <= 1,
        ),
      )
    )
      return null;
    return Object.fromEntries(
      groups.map((id) => [
        id,
        { x: data.positions[id].x, y: data.positions[id].y },
      ]),
    ) as SavedHomePositions;
  } catch {
    return null;
  }
}
/** Missing directions belong to older layouts and retain the horizontal dock. */
export function readHomeOrientations(value: string | null): HomeOrientations {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1)
      return {
        tools:
          data.orientations?.tools === "vertical" ? "vertical" : "horizontal",
        instances:
          data.orientations?.instances === "vertical"
            ? "vertical"
            : "horizontal",
      };
  } catch {
    /* A damaged preference must not prevent the home screen from opening. */
  }
  return { ...defaultHomeOrientations };
}
export function snapHomeRect(rect: Rect, others: Rect[], area: Area) {
  const fitted = fitHomeRect(rect, area);
  const candidates = (axis: "x" | "y", size: "width" | "height") => {
    const values = [
      { value: margin, guide: margin },
      { value: area[size] - margin - rect[size], guide: area[size] - margin },
      { value: (area[size] - rect[size]) / 2, guide: area[size] / 2 },
    ];
    for (const other of others)
      values.push(
        { value: other[axis], guide: other[axis] },
        {
          value: other[axis] + other[size] - rect[size],
          guide: other[axis] + other[size],
        },
        {
          value: other[axis] + (other[size] - rect[size]) / 2,
          guide: other[axis] + other[size] / 2,
        },
        { value: other[axis] - rect[size] - gap, guide: other[axis] - gap },
        {
          value: other[axis] + other[size] + gap,
          guide: other[axis] + other[size] + gap,
        },
      );
    return values
      .filter((candidate) => Math.abs(candidate.value - fitted[axis]) <= 10)
      .sort(
        (a, b) =>
          Math.abs(a.value - fitted[axis]) - Math.abs(b.value - fitted[axis]),
      )[0];
  };
  const x = candidates("x", "width"),
    y = candidates("y", "height");
  return {
    rect: fitHomeRect(
      { ...fitted, x: x?.value ?? fitted.x, y: y?.value ?? fitted.y },
      area,
    ),
    guides: { x: x?.guide, y: y?.guide },
  };
}
