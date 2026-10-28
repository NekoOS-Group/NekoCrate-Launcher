import {
  onBeforeUnmount,
  provide,
  shallowRef,
  watch,
  type InjectionKey,
} from "vue";
import { activeDialog } from "./dialogSurface.ts";
import type { Instance } from "./types.ts";

export type ResourceCopySource = {
  kind: ResourceCopyKind;
  instance: string;
  version: string;
  file: string;
  name: string;
};
export type ResourceCopyKind = "mods" | "resourcepacks" | "shaderpacks";
export function isResourceCopyKind(kind: string): kind is ResourceCopyKind {
  return kind === "mods" || kind === "resourcepacks" || kind === "shaderpacks";
}
export type ResourceDestination = { instance: string; version: string };
export const resourceTransferKey: InjectionKey<
  (event: PointerEvent, source: ResourceCopySource) => void
> = Symbol("resource-transfer");

export function resourceCopyDestination(
  element: Element | null,
  source: ResourceCopySource,
  instances: readonly Instance[],
  running: readonly string[],
): ResourceDestination | undefined {
  const row = element?.closest<HTMLElement>("[data-resource-copy-instance]");
  const instance = instances.find(
    (item) => item.id === row?.dataset.resourceCopyInstance,
  );
  const version = instance?.versions.find(
    (item) => item.id === row?.dataset.resourceCopyVersion,
  );
  if (
    !instance ||
    !version?.installed ||
    running.includes(instance.id) ||
    (instance.id === source.instance && version.id === source.version)
  )
    return;
  return { instance: instance.id, version: version.id };
}

export function useResourceTransfer(options: {
  instances: () => readonly Instance[];
  running: () => readonly string[];
  busy: () => boolean;
  copy: (
    source: ResourceCopySource,
    target: ResourceDestination,
  ) => Promise<void>;
}) {
  const drag = shallowRef<{
    source: ResourceCopySource;
    x: number;
    y: number;
    target?: ResourceDestination;
  }>();
  let pending:
    | {
        source: ResourceCopySource;
        x: number;
        y: number;
        pointer: number;
        element: HTMLElement;
      }
    | undefined;
  let hovered: HTMLElement | undefined;
  let frame = 0;
  let clickTimer: ReturnType<typeof setTimeout> | undefined;
  const suppressClick = (event: MouseEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  function clearClickGuard() {
    clearTimeout(clickTimer);
    document.removeEventListener("click", suppressClick, true);
  }
  function hit(x: number, y: number) {
    const element = document.elementFromPoint(x, y);
    const target =
      pending &&
      resourceCopyDestination(
        element,
        pending.source,
        options.instances(),
        options.running(),
      );
    hovered?.classList.remove("resource-drop-hover");
    hovered = target
      ? element?.closest<HTMLElement>("[data-resource-copy-instance]") ||
        undefined
      : undefined;
    hovered?.classList.add("resource-drop-hover");
    return target;
  }
  function scroll() {
    const state = drag.value;
    if (!state) return;
    const list = document
      .elementFromPoint(state.x, state.y)
      ?.closest<HTMLElement>(".resource-drop-list, .version-manager-list");
    if (list) {
      const box = list.getBoundingClientRect();
      const speed =
        state.y < box.top + 36 ? -7 : state.y > box.bottom - 36 ? 7 : 0;
      if (speed) {
        list.scrollTop += speed;
        drag.value = { ...state, target: hit(state.x, state.y) };
      }
    }
    frame = requestAnimationFrame(scroll);
  }
  function move(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer) return;
    if (options.busy()) {
      stop();
      return;
    }
    if (
      !drag.value &&
      Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < 6
    )
      return;
    event.preventDefault();
    if (!drag.value) {
      window.getSelection()?.removeAllRanges();
      pending.element.setPointerCapture?.(event.pointerId);
      document.addEventListener("click", suppressClick, true);
      document.documentElement.classList.add("resource-transferring");
      frame = requestAnimationFrame(scroll);
    }
    drag.value = {
      source: pending.source,
      x: event.clientX,
      y: event.clientY,
      target: hit(event.clientX, event.clientY),
    };
  }
  function stop() {
    if (drag.value) {
      clearTimeout(clickTimer);
      clickTimer = setTimeout(clearClickGuard, 0);
    }
    const previous = pending;
    pending = undefined;
    drag.value = undefined;
    hovered?.classList.remove("resource-drop-hover");
    hovered = undefined;
    document.documentElement.classList.remove("resource-transferring");
    cancelAnimationFrame(frame);
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", end);
    document.removeEventListener("pointercancel", stop);
    document.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", stop);
    if (previous?.element.hasPointerCapture?.(previous.pointer))
      previous.element.releasePointerCapture(previous.pointer);
  }
  function end(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer) return;
    const source = pending.source;
    const target = drag.value && hit(event.clientX, event.clientY);
    stop();
    if (target && !options.busy()) void options.copy(source, target);
  }
  function key(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    stop();
  }
  function start(event: PointerEvent, source: ResourceCopySource) {
    if (event.button !== 0 || !event.isPrimary || options.busy()) return;
    // Selection begins before the drag threshold is reached. Prevent it on
    // pointerdown, while preserving the ordinary click to expand the row.
    event.preventDefault();
    clearClickGuard();
    pending = {
      source,
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
  provide(resourceTransferKey, start);
  watch(activeDialog, () => {
    if (pending) stop();
  });
  onBeforeUnmount(() => {
    if (pending) stop();
    clearClickGuard();
  });
  return drag;
}
