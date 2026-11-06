export type WindowRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};
export type WindowArea = { width: number; height: number };
export type WindowSnap = "left" | "right" | "maximized";
export type WindowLayout = { rect: WindowRect; snap?: WindowSnap };
export const WINDOW_MARGIN = 12;
export const WINDOW_MIN = { width: 340, height: 240 };

export function fitWindow(rect: WindowRect, area: WindowArea): WindowRect {
  const width = Math.min(
    Math.max(WINDOW_MIN.width, rect.width),
    Math.max(1, area.width - 24),
  );
  const height = Math.min(
    Math.max(WINDOW_MIN.height, rect.height),
    Math.max(1, area.height - 24),
  );
  return {
    x: Math.max(12, Math.min(rect.x, area.width - width - 12)),
    y: Math.max(12, Math.min(rect.y, area.height - height - 12)),
    width,
    height,
  };
}

export function snappedWindow(snap: WindowSnap, area: WindowArea): WindowRect {
  const width = Math.max(1, area.width - 24);
  const height = Math.max(1, area.height - 24);
  if (snap === "maximized") return { x: 12, y: 12, width, height };
  const half = (width - 12) / 2;
  // Small viewports cannot fit two useful panels; docking becomes maximize.
  if (half < WINDOW_MIN.width) return { x: 12, y: 12, width, height };
  return { x: snap === "left" ? 12 : 24 + half, y: 12, width: half, height };
}

export function snapAt(
  x: number,
  y: number,
  area: WindowArea,
): WindowSnap | undefined {
  if (y <= 18) return "maximized";
  if (x <= 18) return "left";
  if (x >= area.width - 18) return "right";
}

export function resizeWindow(
  start: WindowRect,
  edge: string,
  dx: number,
  dy: number,
  area: WindowArea,
): WindowRect {
  const minW = Math.min(WINDOW_MIN.width, area.width - 24);
  const minH = Math.min(WINDOW_MIN.height, area.height - 24);
  let { x, y, width, height } = start;
  if (edge.includes("e"))
    width = Math.max(minW, Math.min(width + dx, area.width - x - 12));
  if (edge.includes("s"))
    height = Math.max(minH, Math.min(height + dy, area.height - y - 12));
  if (edge.includes("w")) {
    x = Math.max(12, Math.min(x + dx, x + width - minW));
    width += start.x - x;
  }
  if (edge.includes("n")) {
    y = Math.max(12, Math.min(y + dy, y + height - minH));
    height += start.y - y;
  }
  return fitWindow({ x, y, width, height }, area);
}

export function validWindowLayout(value: unknown): value is WindowLayout {
  if (!value || typeof value !== "object" || !("rect" in value)) return false;
  const { rect, snap } = value as WindowLayout;
  return (
    !!rect &&
    [rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) &&
    rect.width > 0 &&
    rect.height > 0 &&
    (snap === undefined || ["left", "right", "maximized"].includes(snap))
  );
}
