import {
  onBeforeUnmount,
  provide,
  shallowRef,
  watch,
  type InjectionKey,
} from "vue";
import { activeDialog } from "./dialogSurface.ts";
import { clearDropHint, dropInteraction, showDropHint } from "./dropSurface.ts";
import { resourceDropTarget, resourceDropIssue } from "./resources.ts";
import { t } from "./i18n.ts";
import type { Instance } from "./types.ts";

export type ResourceCopySource = {
  kind: ResourceDragKind;
  instance: string;
  version: string;
  file: string;
  name: string;
};
export type ResourceCopyKind = "mods" | "resourcepacks" | "shaderpacks";
export type ResourceDragKind = ResourceCopyKind | "datapacks" | "saves";
export function isResourceDragKind(kind: string): kind is ResourceDragKind {
  return isResourceCopyKind(kind) || kind === "datapacks" || kind === "saves";
}
export function isResourceCopyKind(kind: string): kind is ResourceCopyKind {
  return kind === "mods" || kind === "resourcepacks" || kind === "shaderpacks";
}
export type ResourceDestination = { instance: string; version: string };
export const resourceTransferKey: InjectionKey<
  (event: PointerEvent, source: ResourceCopySource) => void
> = Symbol("resource-transfer");

function copyIssue(
  element: Element | null,
  source: ResourceCopySource,
  instances: readonly Instance[],
  running: readonly string[],
) {
  const kind = element?.closest<HTMLElement>(
    "[data-drop-kind], [data-resource-instance]",
  )?.dataset.dropKind;
  const mismatch = resourceDropIssue(
    [
      {
        path: source.file,
        name: source.name,
        format: "resource",
        kinds: [source.kind],
        issue: null,
      },
    ],
    kind,
  );
  if (mismatch) return mismatch;
  if (!isResourceCopyKind(source.kind)) return t("此资源可拖出到文件管理器");
  const target = resourceDropTarget(element, instances);
  if (!target) return t("请拖到目标版本");
  if (!target.instance.versions.find((v) => v.id === target.version)?.installed)
    return t("请先安装目标游戏版本");
  if (running.includes(target.instance.id)) return t("请先结束目标实例的游戏");
  if (
    target.instance.id === source.instance &&
    target.version === source.version
  )
    return t("资源已在此版本中");
}
export function resourceCopyDestination(
  element: Element | null,
  source: ResourceCopySource,
  instances: readonly Instance[],
  running: readonly string[],
): ResourceDestination | undefined {
  if (copyIssue(element, source, instances, running)) return;
  const target = resourceDropTarget(element, instances)!;
  return { instance: target.instance.id, version: target.version };
}

export function useResourceTransfer(options: {
  instances: () => readonly Instance[];
  running: () => readonly string[];
  busy: () => boolean;
  copy: (
    source: ResourceCopySource,
    target: ResourceDestination,
  ) => Promise<void>;
  export?: (source: ResourceCopySource) => Promise<unknown>;
  browse?: (element: Element | null) => void;
  finishBrowse?: () => void;
  error?: (error: unknown) => void;
}) {
  const drag = shallowRef<{
    source: ResourceCopySource;
    x: number;
    y: number;
    target?: ResourceDestination;
    native?: boolean;
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
    options.browse?.(element);
    if (!pending) return;
    const surface = element?.closest<HTMLElement>(
      "[data-drop-kind], [data-resource-instance]",
    );
    const issue = copyIssue(
      element,
      pending.source,
      options.instances(),
      options.running(),
    );
    const target = resourceCopyDestination(
      element,
      pending.source,
      options.instances(),
      options.running(),
    );
    if (surface)
      showDropHint(
        surface,
        issue || t("复制到 {version}", { version: target!.version }),
        !!issue,
      );
    else clearDropHint();
    return target;
  }
  function scroll() {
    const state = drag.value;
    if (!state) return;
    const list = document
      .elementFromPoint(state.x, state.y)
      ?.closest<HTMLElement>(
        ".version-menu-list, .version-manager-list, .instance-overflow",
      );
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
  async function handoff() {
    if (!pending || !drag.value || drag.value.native || !options.export) return;
    const session = pending;
    drag.value = { ...drag.value, native: true };
    clearDropHint();
    if (session.element.hasPointerCapture?.(session.pointer))
      session.element.releasePointerCapture(session.pointer);
    try {
      await options.export(session.source);
    } catch (error) {
      options.error?.(error);
    } finally {
      if (pending === session) stop();
    }
  }
  function move(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer || drag.value?.native)
      return;
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
      dropInteraction.value = true;
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
    // Start the native file drag while the mouse is still down, before leaving
    // the webview. Once native, the OS supplies same-window drop events too.
    if (
      event.clientX < 24 ||
      event.clientY < 24 ||
      event.clientX > window.innerWidth - 24 ||
      event.clientY > window.innerHeight - 24
    )
      void handoff();
  }
  function stop() {
    if (drag.value) {
      clearTimeout(clickTimer);
      clickTimer = setTimeout(clearClickGuard, 0);
    }
    const active = !!drag.value;
    const previous = pending;
    pending = undefined;
    drag.value = undefined;
    clearDropHint();
    dropInteraction.value = false;
    if (active) options.finishBrowse?.();
    document.documentElement.classList?.remove("resource-transferring");
    cancelAnimationFrame(frame);
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", end);
    document.removeEventListener("pointercancel", cancelPointer);
    document.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", cancelPointer);
    if (previous?.element.hasPointerCapture?.(previous.pointer))
      previous.element.releasePointerCapture(previous.pointer);
  }
  function complete(x: number, y: number) {
    if (!pending) return;
    const source = pending.source;
    const target = drag.value && hit(x, y);
    stop();
    if (target && !options.busy()) void options.copy(source, target);
  }
  function end(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointer || drag.value?.native)
      return;
    complete(event.clientX, event.clientY);
  }
  function cancelPointer() {
    if (!drag.value?.native) stop();
  }
  function key(event: KeyboardEvent) {
    if (event.key !== "Escape" || drag.value?.native) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    stop();
  }
  function start(event: PointerEvent, source: ResourceCopySource) {
    if (event.button !== 0 || !event.isPrimary || options.busy()) return;
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
    document.addEventListener("pointercancel", cancelPointer);
    document.addEventListener("keydown", key, true);
    window.addEventListener("blur", cancelPointer);
  }
  function nativeEvent(type: string, x: number, y: number) {
    if (!drag.value?.native) return false;
    if (type === "drop") complete(x, y);
    else if (type === "leave") clearDropHint();
    else drag.value = { ...drag.value, x, y, target: hit(x, y) };
    return true;
  }
  provide(resourceTransferKey, start);
  watch(activeDialog, () => {
    if (pending && !dropInteraction.value) stop();
  });
  onBeforeUnmount(() => {
    if (pending) stop();
    clearClickGuard();
  });
  return Object.assign(drag, { nativeEvent });
}
