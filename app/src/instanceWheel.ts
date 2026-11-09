/** Wheel input is quantized so trackpad tails cannot race through the list. */
export function createInstanceWheelStepper() {
  let accumulated = 0;
  let lastEvent = -Infinity;
  let lastStep = -Infinity;
  return {
    reset() {
      accumulated = 0;
      lastEvent = lastStep = -Infinity;
    },
    step(delta: number, mode: number, now: number) {
      if (!Number.isFinite(delta) || !delta) return 0;
      const pixels = delta * (mode === 1 ? 16 : mode === 2 ? 160 : 1);
      if (now - lastEvent > 180 || Math.sign(pixels) !== Math.sign(accumulated))
        accumulated = 0;
      lastEvent = now;
      if (now - lastStep < 90) return 0;
      accumulated += pixels;
      if (Math.abs(accumulated) < 40) return 0;
      const direction = Math.sign(accumulated);
      accumulated = 0;
      lastStep = now;
      return direction;
    },
  };
}

export type InstanceWheelBounds = { above: number; below: number };
export const defaultInstanceWheelBounds: InstanceWheelBounds = {
  above: 240,
  below: 140,
};
const minExtent = 72;
const maxExtent = 600;
function extent(value: unknown, fallback: number) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.round(Math.max(minExtent, Math.min(maxExtent, value)))
    : fallback;
}
export function readInstanceWheelBounds(
  value: string | null,
): InstanceWheelBounds {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1)
      return {
        above: extent(
          data.instanceWheel?.above,
          defaultInstanceWheelBounds.above,
        ),
        below: extent(
          data.instanceWheel?.below,
          defaultInstanceWheelBounds.below,
        ),
      };
  } catch {
    /* Older layouts retain the default visible range. */
  }
  return { ...defaultInstanceWheelBounds };
}

export function instanceWheelFrame(
  anchor: { left: number; top: number; width: number; height: number },
  viewport: { width: number; height: number },
  bounds: InstanceWheelBounds = defaultInstanceWheelBounds,
) {
  const width = Math.min(260, viewport.width - 24);
  const leftLabels = anchor.left + anchor.width / 2 > viewport.width / 2;
  const iconX = leftLabels ? width - 32 : 32;
  const left = Math.max(
    12,
    Math.min(
      viewport.width - width - 12,
      anchor.left + anchor.width / 2 - iconX,
    ),
  );
  const centerY = anchor.top + anchor.height / 2;
  const top = Math.max(12, centerY - bounds.above);
  const bottom = Math.min(viewport.height - 12, centerY + bounds.below);
  return {
    left: left - anchor.left,
    top: top - anchor.top,
    width,
    height: bottom - top,
    // Clamping the hover area must never move its selected tile. Keep the
    // anchor in window coordinates and let the surrounding rows be clipped.
    iconX: anchor.left + anchor.width / 2 - left,
    iconY: anchor.top + anchor.height / 2 - top,
    leftLabels,
  };
}

/** Resize the visible edge, even when the saved extent is clipped by the window. */
export function resizeInstanceWheelBounds(
  bounds: InstanceWheelBounds,
  edge: "top" | "bottom",
  delta: number,
  anchor: { left: number; top: number; width: number; height: number },
  viewport: { width: number; height: number },
): InstanceWheelBounds {
  const frame = instanceWheelFrame(anchor, viewport, bounds);
  const top = edge === "top";
  const available = top
    ? anchor.top + anchor.height / 2 - 12
    : viewport.height - 12 - anchor.top - anchor.height / 2;
  const current = top ? frame.iconY : frame.height - frame.iconY;
  return {
    ...bounds,
    [top ? "above" : "below"]: extent(
      Math.min(
        Math.max(minExtent, available),
        current + (top ? -delta : delta),
      ),
      top ? bounds.above : bounds.below,
    ),
  };
}
