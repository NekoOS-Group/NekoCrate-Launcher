import { computed, onBeforeUnmount, ref, shallowRef, type Ref } from "vue";
import { newsDockTarget, type Area, type Rect } from "./homeLayout.ts";
import type { EdgeWidgetLayout, NewsDock } from "./homeWidgets.ts";

/** Edge placement offers auto-hide; only an explicit toggle enables it. */
export function useEdgeWidget<T extends EdgeWidgetLayout>(
  initial: T,
  editing: Ref<boolean>,
) {
  const saved = shallowRef({ ...initial });
  const draft = shallowRef({ ...initial });
  const previewDock = ref<NewsDock>();
  const layout = computed(() => (editing.value ? draft.value : saved.value));
  const dock = computed(() =>
    editing.value && previewDock.value !== undefined
      ? previewDock.value
      : layout.value.dock,
  );
  const open = ref(false);
  const host = shallowRef<HTMLElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  function candidate(rect: Rect | undefined, area: Area): NewsDock {
    const side = rect ? newsDockTarget(rect, area, dock.value) : null;
    return side === layout.value.align ? side : null;
  }
  function toggleDock(rect: Rect | undefined, area: Area) {
    if (!editing.value) return;
    draft.value = {
      ...draft.value,
      dock: dock.value ? null : candidate(rect, area),
    };
  }
  function toggleAlignment() {
    if (!editing.value) return;
    draft.value = {
      ...draft.value,
      align: layout.value.align === "left" ? "right" : "left",
      dock: null,
    };
    previewDock.value = undefined;
  }
  function move(rect: Rect, area: Area) {
    // Moving inward detaches. Arriving at either edge never enables auto-hide.
    previewDock.value =
      dock.value && candidate(rect, area) === dock.value ? dock.value : null;
  }
  function commitMove() {
    // null deliberately means detached, whereas undefined means no gesture.
    if (previewDock.value !== undefined)
      draft.value = { ...draft.value, dock: previewDock.value };
  }
  function setHost(element: unknown) {
    host.value = element instanceof HTMLElement ? element : undefined;
  }
  function show() {
    clearTimeout(timer);
    if (!editing.value && dock.value) open.value = true;
  }
  function close() {
    clearTimeout(timer);
    open.value = false;
  }
  function leave() {
    clearTimeout(timer);
    // WebKit may retain focus-visible after pointer movement. Stale focus
    // must not pin the panel open; re-entry and real focus changes cancel this.
    timer = setTimeout(() => {
      open.value = false;
    }, 180);
  }
  function blur(event: FocusEvent) {
    if (
      !(event.relatedTarget instanceof Node) ||
      !host.value?.contains(event.relatedTarget)
    )
      leave();
  }
  function escape(event: KeyboardEvent) {
    if (editing.value || !dock.value) return;
    event.stopPropagation();
    event.preventDefault();
    host.value
      ?.querySelector<HTMLButtonElement>(".widget-edge-trigger")
      ?.focus({ preventScroll: true });
    close();
  }
  onBeforeUnmount(close);
  return {
    saved,
    draft,
    layout,
    dock,
    previewDock,
    open,
    host,
    candidate,
    toggleDock,
    toggleAlignment,
    move,
    commitMove,
    setHost,
    show,
    close,
    leave,
    blur,
    escape,
  };
}
