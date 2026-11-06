type Rect = { left: number; top: number; width: number; height: number };

/** Keep a settings bubble attached to its trigger without leaving the viewport. */
export function bubblePosition(
  anchor: Rect | undefined,
  panel: { width: number; height: number },
  viewport: { width: number; height: number },
  preferred: "above" | "below",
) {
  const margin = 12,
    gap = 12;
  const width = Math.min(panel.width, viewport.width - margin * 2);
  const height = Math.min(panel.height, viewport.height - margin * 2);
  const clamp = (value: number, size: number, limit: number) =>
    Math.max(margin, Math.min(value, limit - size - margin));
  if (!anchor)
    return {
      left: (viewport.width - width) / 2,
      top: (viewport.height - height) / 2,
      origin: "50% 50%",
      side: undefined,
      tip: 0,
    };
  const above = anchor.top - gap - margin;
  const below = viewport.height - (anchor.top + anchor.height) - gap - margin;
  const side =
    preferred === "above"
      ? above >= height || above >= below
        ? "above"
        : "below"
      : below >= height || below >= above
        ? "below"
        : "above";
  const left = clamp(anchor.left - 16, width, viewport.width);
  const top = clamp(
    side === "above"
      ? anchor.top - gap - height
      : anchor.top + anchor.height + gap,
    height,
    viewport.height,
  );
  const tip = Math.max(
    20,
    Math.min(anchor.left + anchor.width / 2 - left, width - 20),
  );
  // When a tall panel overlaps its trigger, omit a misleading pointer.
  const separated =
    side === "above"
      ? top + height <= anchor.top - gap
      : top >= anchor.top + anchor.height + gap;
  return {
    left,
    top,
    tip,
    side: separated ? side : undefined,
    origin: `${tip}px ${side === "above" ? "100%" : "0%"}`,
  };
}
