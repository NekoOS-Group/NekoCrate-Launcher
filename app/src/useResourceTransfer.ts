import {
  onBeforeUnmount,
  provide,
  shallowRef,
  watch,
  type InjectionKey,
} from "vue";
import { activeDialog } from "./dialogSurface.ts";
import { clearDropHint, dropInteraction, showDropHint } from "./dropSurface.ts";
import {
  resourceDropTarget,
  resourceDropIssue,
  resourceDropTypeMatches,
} from "./resources.ts";
import { t } from "./i18n.ts";
import type { Instance } from "./types.ts";

export type ResourceCopySource = {
  global?: boolean;
  // The library may browse installed files, but only delete its own cache.
  removable?: boolean;
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
export type ResourceDestination = {
  trash?: boolean;
  global?: boolean;
  selection?: boolean;
  instance: string;
  version: string;
  world?: string;
};
export const resourceSourceKey = (source: ResourceCopySource) =>
  JSON.stringify([
    !!source.global,
    source.kind,
    source.instance,
    source.version,
    source.file,
  ]);
export const resourceTransferKey: InjectionKey<
  (
    event: PointerEvent,
    source: ResourceCopySource | ResourceCopySource[],
  ) => void
> = Symbol("resource-transfer");
export const resourceRemovalKey: InjectionKey<
  (sources: ResourceCopySource[]) => Promise<void>
> = Symbol("resource-removal");

// Matches the existing 72px trash target, 20px from the top/right edge.
export function resourceTrashHit(x: number, y: number, width: number) {
  return x >= width - 92 && x <= width - 20 && y >= 20 && y <= 92;
}

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
  if (kind === "selection") {
    const surface = element?.closest<HTMLElement>("[data-drop-kind]");
    if (surface?.dataset.dropDisabled === "true")
      return t("请先选择游戏版本并等待当前操作完成");
    if (source.kind === "mods" && surface?.dataset.dropLoader === "vanilla")
      return t("请先选择 Mod 加载器");
    return;
  }
  if (
    element?.closest<HTMLElement>("[data-resource-library]")?.dataset
      .resourceLibrary !== undefined
  ) {
    if (source.kind === "saves")
      return t("全局资源库支持 Mods、资源包、光影和数据包");
    if (source.global) return t("资源已在全局缓存中");
    return;
  }
  const world =
    element?.closest<HTMLElement>("[data-drop-world]")?.dataset.dropWorld;
  if (source.kind === "datapacks" && !world) return t("请拖到目标存档");
  const target = resourceDropTarget(element, instances);
  if (!target) return t("请拖到目标版本");
  if (!target.instance.versions.find((v) => v.id === target.version)?.installed)
    return t("请先安装目标游戏版本");
  if (running.includes(target.instance.id)) return t("请先结束目标实例的游戏");
  if (
    target.instance.id === source.instance &&
    target.version === source.version &&
    source.kind !== "datapacks"
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
  if (
    element?.closest<HTMLElement>("[data-drop-kind]")?.dataset.dropKind ===
    "selection"
  )
    return { selection: true, instance: "", version: "" };
  if (
    element?.closest<HTMLElement>("[data-resource-library]")?.dataset
      .resourceLibrary !== undefined
  )
    return { global: true, instance: "", version: "" };
  const target = resourceDropTarget(element, instances)!;
  const world =
    element?.closest<HTMLElement>("[data-drop-world]")?.dataset.dropWorld;
  return {
    instance: target.instance.id,
    version: target.version,
    ...(world ? { world } : {}),
  };
}

export function useResourceTransfer(options: {
  instances: () => readonly Instance[];
  running: () => readonly string[];
  busy: () => boolean;
  copy: (
    source: ResourceCopySource,
    target: ResourceDestination,
  ) => Promise<void>;
  copyMany?: (
    sources: ResourceCopySource[],
    target: ResourceDestination,
  ) => Promise<void>;
  exportMany?: (sources: ResourceCopySource[]) => Promise<unknown>;
  remove?: (sources: ResourceCopySource[]) => Promise<void>;
  export?: (source: ResourceCopySource) => Promise<unknown>;
  preview?: (
    sources: ResourceCopySource[],
    target: ResourceDestination,
  ) => Promise<string | undefined>;
  browse?: (element: Element | null) => void;
  finishBrowse?: () => void;
  error?: (error: unknown) => void;
}) {
  const drag = shallowRef<{
    source: ResourceCopySource;
    sources: ResourceCopySource[];
    x: number;
    y: number;
    target?: ResourceDestination;
    native?: boolean;
    outside?: boolean;
  }>();
  let pending:
    | {
        source: ResourceCopySource;
        sources: ResourceCopySource[];
        x: number;
        y: number;
        pointer: number;
        element: HTMLElement;
      }
    | undefined;
  let frame = 0;
  let previewKey = "",
    previewMessage: string | undefined;
  let previewInvalid = false;
  const previews = new Map<string, Promise<string | undefined>>();
  let clickTimer: ReturnType<typeof setTimeout> | undefined;
  const suppressClick = (event: MouseEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  function clearClickGuard() {
    previews.clear();
    previewKey = "";
    previewMessage = undefined;
    clearTimeout(clickTimer);
    document.removeEventListener("click", suppressClick, true);
  }
  function hit(x: number, y: number) {
    if (
      pending &&
      options.remove &&
      resourceTrashHit(x, y, window.innerWidth)
    ) {
      previewKey = "";
      clearDropHint();
      if (
        options.busy() ||
        pending.sources.some((source) => source.removable === false) ||
        pending.sources.some(
          (source) =>
            !source.global && options.running().includes(source.instance),
        )
      )
        return;
      return { trash: true, instance: "", version: "" };
    }
    const element = document.elementFromPoint(x, y);
    options.browse?.(element);
    if (!pending) return;
    const surface = element?.closest<HTMLElement>(
      "[data-drop-kind], [data-resource-instance]",
    );
    if (
      !surface ||
      !resourceDropTypeMatches(
        pending.sources.map((source) => ({
          path: source.file,
          name: source.name,
          format: "resource",
          kinds: [source.kind],
          issue: null,
        })),
        surface.dataset.dropKind,
      )
    ) {
      previewKey = "";
      clearDropHint();
      return;
    }
    const issue = pending.sources
      .map((source) =>
        copyIssue(element, source, options.instances(), options.running()),
      )
      .find(Boolean);
    const target = issue
      ? undefined
      : resourceCopyDestination(
          element,
          pending.source,
          options.instances(),
          options.running(),
        );
    const key = target ? JSON.stringify([pending.sources, target]) : "";
    if (key !== previewKey) {
      previewKey = key;
      previewMessage = undefined;
      previewInvalid = false;
      if (target && options.preview) {
        const session = pending;
        if (!previews.has(key))
          previews.set(key, options.preview(pending.sources, target));
        void previews
          .get(key)!
          .then((message) => {
            if (pending === session && previewKey === key) {
              previewMessage = message;
              refreshHit();
            }
          })
          .catch((error) => {
            if (pending === session && previewKey === key) {
              previewMessage = String(error);
              previewInvalid = true;
              refreshHit();
            }
          });
      }
    }
    if (surface)
      showDropHint(
        surface,
        issue ||
          previewMessage ||
          (target?.selection
            ? t("加入新版本的预选资源")
            : target?.global
              ? t("缓存到启动器")
              : t(
                  pending.source.kind === "saves"
                    ? "复制存档到 {name} · {version}"
                    : "复制到 {name} · {version}",
                  {
                    name:
                      options.instances().find((i) => i.id === target!.instance)
                        ?.name || "",
                    version: target!.world || target!.version,
                  },
                )),
        !!issue || previewInvalid,
      );
    else clearDropHint();
    return previewInvalid ? undefined : target;
  }
  function refreshHit() {
    const state = drag.value;
    if (!state || state.outside) return;
    const target = hit(state.x, state.y);
    if (
      drag.value === state &&
      JSON.stringify(state.target) !== JSON.stringify(target)
    )
      drag.value = { ...state, target };
  }
  function scroll() {
    const state = drag.value;
    if (!state) return;
    const list =
      !state.outside &&
      document
        .elementFromPoint(state.x, state.y)
        ?.closest<HTMLElement>(
          '.version-menu-list, .version-manager-list, .instance-overflow, .instance-strip[data-orientation="vertical"]',
        );
    if (list) {
      const box = list.getBoundingClientRect();
      const speed =
        state.y < box.top + 36 ? -7 : state.y > box.bottom - 36 ? 7 : 0;
      if (speed) {
        list.scrollTop += speed;
      }
    }
    // Menus can expand and windows can rise under a stationary pointer. Hit
    // testing and the visible rectangle must both follow the current layout.
    refreshHit();
    frame = requestAnimationFrame(scroll);
  }
  async function handoff() {
    if (!pending || !drag.value || drag.value.native || !options.export) return;
    const session = pending;
    drag.value = {
      ...drag.value,
      native: true,
      outside: true,
      target: undefined,
    };
    clearDropHint();
    if (session.element.hasPointerCapture?.(session.pointer))
      session.element.releasePointerCapture(session.pointer);
    try {
      if (session.sources.length > 1)
        await options.exportMany?.(session.sources);
      else await options.export(session.source);
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
      sources: pending.sources,
      x: event.clientX,
      y: event.clientY,
      target: hit(event.clientX, event.clientY),
    };
    // Start the native file drag while the mouse is still down, before leaving
    // the webview. Once native, the OS supplies same-window drop events too.
    if (
      !(
        options.remove &&
        resourceTrashHit(event.clientX, event.clientY, window.innerWidth)
      ) &&
      (event.clientX < 24 ||
        event.clientY < 24 ||
        event.clientX > window.innerWidth - 24 ||
        event.clientY > window.innerHeight - 24)
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
    const sources = pending.sources;
    const target = drag.value && hit(x, y);
    stop();
    if (target && !options.busy()) {
      if (target.trash)
        void options.remove?.(sources).catch((error) => options.error?.(error));
      else if (sources.length > 1) void options.copyMany?.(sources, target);
      else void options.copy(source, target);
    }
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
  function start(
    event: PointerEvent,
    input: ResourceCopySource | ResourceCopySource[],
  ) {
    if (
      event.button !== 0 ||
      !event.isPrimary ||
      event.shiftKey ||
      event.metaKey ||
      event.ctrlKey ||
      options.busy()
    )
      return;
    const sources = [
      ...new Map(
        (Array.isArray(input) ? input : [input]).map((source) => [
          resourceSourceKey(source),
          source,
        ]),
      ).values(),
    ];
    if (!sources.length) return;
    if (sources.length > 256) {
      options.error?.(new Error(t("每次最多拖动 256 项资源")));
      return;
    }
    const source = sources[0]!;
    event.preventDefault();
    clearClickGuard();
    pending = {
      source,
      sources,
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
    else if (type === "leave") {
      drag.value = { ...drag.value, outside: true, target: undefined };
      clearDropHint();
    } else
      drag.value = { ...drag.value, outside: false, x, y, target: hit(x, y) };
    return true;
  }
  provide(resourceTransferKey, start);
  if (options.remove) provide(resourceRemovalKey, options.remove);
  watch(activeDialog, () => {
    if (pending && !dropInteraction.value) stop();
  });
  onBeforeUnmount(() => {
    if (pending) stop();
    clearClickGuard();
  });
  return Object.assign(drag, { nativeEvent });
}
