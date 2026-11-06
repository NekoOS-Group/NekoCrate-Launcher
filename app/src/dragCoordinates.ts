type Point = { x: number; y: number };

/** Convert the native drag event to the CSS viewport used by elementFromPoint.
 * Wry 0.57 emits Cocoa/GTK logical coordinates, but Win32 client pixels.
 * Tauri wraps both in PhysicalPosition without converting the Unix values.
 * PointerEvent.clientX/Y already use CSS pixels and must never pass through here.
 */
export function nativeDropPoint(
  point: Point,
  platform = typeof navigator === "undefined" ? "" : navigator.platform,
  pixelRatio = window.devicePixelRatio,
): Point {
  const scale =
    /^win/i.test(platform) && Number.isFinite(pixelRatio) && pixelRatio > 0
      ? pixelRatio
      : 1;
  return { x: point.x / scale, y: point.y / scale };
}
