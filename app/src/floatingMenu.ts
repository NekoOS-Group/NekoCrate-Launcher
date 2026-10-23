export function floatingMenuPosition(
  anchor: { left: number; bottom: number; width: number },
  viewport: { width: number; height: number },
) {
  const width = Math.min(anchor.width, Math.max(1, viewport.width - 32));
  const left = Math.max(16, Math.min(anchor.left, viewport.width - width - 16));
  const top = Math.max(16, Math.min(anchor.bottom + 8, viewport.height - 96));
  return {
    left: `${left}px`,
    top: `${top}px`,
    width: `${width}px`,
    maxHeight: `${Math.max(80, viewport.height - top - 16)}px`,
  };
}
