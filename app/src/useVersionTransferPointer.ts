import { onBeforeUnmount } from "vue";
import type { GameTransfer } from "./gameTransfer";
/** Transfer-only pointer gesture for the management list; menu sorting stays independent. */
export function useVersionTransferPointer(
  instance: () => string,
  blocked: () => boolean,
  transfer?: GameTransfer,
) {
  let pointer:
    | {
        id: number;
        version: string;
        element: HTMLElement;
        x: number;
        y: number;
        active: boolean;
      }
    | undefined;
  let suppressed = false;
  function move(e: PointerEvent) {
    if (!pointer || e.pointerId !== pointer.id) return;
    if (Math.hypot(e.clientX - pointer.x, e.clientY - pointer.y) > 6)
      pointer.active = true;
    if (pointer.active) {
      suppressed = true;
      e.preventDefault();
      transfer?.preview(
        instance(),
        pointer.version,
        document.elementFromPoint(e.clientX, e.clientY),
        e.clientX,
        e.clientY,
      );
    }
  }
  function stop() {
    const old = pointer;
    pointer = undefined;
    if (!old) return;
    if (old?.element.hasPointerCapture(old.id))
      old.element.releasePointerCapture(old.id);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", stop);
    window.removeEventListener("blur", stop);
    document.removeEventListener("keydown", escape, true);
    transfer?.finish();
  }
  function up(e: PointerEvent) {
    if (pointer?.id !== e.pointerId) return;
    const p = pointer;
    stop();
    if (p.active && !blocked())
      transfer?.drop(
        instance(),
        p.version,
        document.elementFromPoint(e.clientX, e.clientY),
        e.clientX,
        e.clientY,
      );
  }
  function escape(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      stop();
    }
  }
  function start(e: PointerEvent, version: string) {
    suppressed = false;
    if (blocked() || e.button !== 0 || !e.isPrimary || !transfer) return;
    pointer = {
      id: e.pointerId,
      version,
      element: e.currentTarget as HTMLElement,
      x: e.clientX,
      y: e.clientY,
      active: false,
    };
    pointer.element.setPointerCapture(e.pointerId);
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
    document.addEventListener("keydown", escape, true);
  }
  onBeforeUnmount(stop);
  return { start, suppress: () => suppressed };
}
