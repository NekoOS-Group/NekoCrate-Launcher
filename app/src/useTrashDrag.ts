import { onBeforeUnmount, shallowRef } from "vue";
import { clearDropHint, dropInteraction } from "./dropSurface.ts";
import {
  resourceTrashHit,
  type ResourceDestination,
} from "./useResourceTransfer.ts";

// Cached runtimes have no copy/export destination. Reuse the resource trash
// presentation without treating a Java directory as a game resource.
export function useTrashDrag<T>(options: {
  canRemove: (item: T) => boolean;
  describe: (item: T) => { kind: "java"; name: string };
  remove: (item: T) => Promise<void>;
}) {
  const drag = shallowRef<{
    source: { kind: "java"; name: string };
    x: number;
    y: number;
    target?: ResourceDestination;
  }>();
  let pending:
    | { item: T; x: number; y: number; pointer: number; element: HTMLElement }
    | undefined;
  let clickTimer: ReturnType<typeof setTimeout> | undefined;
  let guardingClick = false;
  const suppressClick = (event: MouseEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  function clearClickGuard() {
    clearTimeout(clickTimer);
    if (guardingClick)
      document.removeEventListener("click", suppressClick, true);
    guardingClick = false;
  }
  function stop() {
    if (!pending) return;
    const previous = pending;
    pending = undefined;
    if (drag.value) {
      clearDropHint();
      dropInteraction.value = false;
      document.documentElement.classList.remove("resource-transferring");
      clickTimer = setTimeout(clearClickGuard, 0);
    }
    drag.value = undefined;
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", end);
    document.removeEventListener("pointercancel", stop);
    document.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", stop);
    if (previous.element.hasPointerCapture?.(previous.pointer))
      previous.element.releasePointerCapture(previous.pointer);
  }
  function move(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer) return;
    if (!options.canRemove(pending.item)) return stop();
    if (
      !drag.value &&
      Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < 6
    )
      return;
    event.preventDefault();
    if (!drag.value) {
      window.getSelection()?.removeAllRanges();
      clearDropHint();
      dropInteraction.value = true;
      pending.element.setPointerCapture?.(pending.pointer);
      document.documentElement.classList.add("resource-transferring");
      document.addEventListener("click", suppressClick, true);
      guardingClick = true;
    }
    drag.value = {
      source: options.describe(pending.item),
      x: event.clientX,
      y: event.clientY,
      target: resourceTrashHit(event.clientX, event.clientY, window.innerWidth)
        ? { trash: true, instance: "", version: "" }
        : undefined,
    };
  }
  function end(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer) return;
    const item = pending.item;
    const remove =
      !!drag.value &&
      options.canRemove(item) &&
      resourceTrashHit(event.clientX, event.clientY, window.innerWidth);
    stop();
    if (remove) void options.remove(item);
  }
  function key(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    stop();
  }
  function start(event: PointerEvent, item: T) {
    if (
      event.button !== 0 ||
      !event.isPrimary ||
      event.shiftKey ||
      event.metaKey ||
      event.ctrlKey ||
      dropInteraction.value ||
      !options.canRemove(item)
    )
      return;
    stop();
    clearClickGuard();
    event.preventDefault();
    pending = {
      item,
      x: event.clientX,
      y: event.clientY,
      pointer: event.pointerId,
      element: event.currentTarget as HTMLElement,
    };
    document.addEventListener("pointermove", move, { passive: false });
    document.addEventListener("pointerup", end);
    document.addEventListener("pointercancel", stop);
    document.addEventListener("keydown", key, true);
    window.addEventListener("blur", stop);
  }
  onBeforeUnmount(() => {
    stop();
    clearClickGuard();
  });
  return { drag, start };
}
