import { computed, onBeforeUnmount, ref, shallowRef, type Ref } from "vue";
/** Small home lists share pointer/keyboard sorting, with no document text selection. */
export function useWidgetSorting<T>(
  items: Readonly<Ref<T[]>>,
  key: (item: T) => string,
  save: (items: T[]) => void,
) {
  const preview = shallowRef<T[]>(),
    active = ref<string>();
  const rows = computed(() => preview.value || items.value);
  let gesture:
    | {
        pointer: number;
        target: HTMLElement;
        surface: HTMLElement;
        slots: { x: number; y: number; width: number; height: number }[];
      }
    | undefined;
  function stop() {
    const old = gesture;
    gesture = undefined;
    if (old?.target.hasPointerCapture(old.pointer))
      old.target.releasePointerCapture(old.pointer);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("blur", cancel);
    document.removeEventListener("keydown", escape, true);
    preview.value = undefined;
    active.value = undefined;
  }
  function move(event: PointerEvent) {
    if (!gesture || event.pointerId !== gesture.pointer) return;
    event.preventDefault();
    // Hit-test stable layout slots, not the transformed rows travelling through
    // those slots during FLIP. Otherwise passing rows repeatedly swap back.
    const rect = gesture.surface.getBoundingClientRect();
    const x = event.clientX - rect.left + gesture.surface.scrollLeft;
    const y = event.clientY - rect.top + gesture.surface.scrollTop;
    const to = gesture.slots.findIndex(
      (slot) =>
        x >= slot.x &&
        x <= slot.x + slot.width &&
        y >= slot.y &&
        y <= slot.y + slot.height,
    );
    const list = [...rows.value] as T[];
    const from = list.findIndex((item) => key(item) === active.value);
    if (from < 0 || to < 0 || from === to) return;
    list.splice(to, 0, list.splice(from, 1)[0]!);
    preview.value = list;
  }
  function up(event: PointerEvent) {
    if (event.pointerId !== gesture?.pointer) return;
    const value = preview.value;
    stop();
    if (value) save(value as T[]);
  }
  function cancel() {
    stop();
  }
  function escape(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      event.preventDefault();
      cancel();
    }
  }
  function start(event: PointerEvent, id: string) {
    if (event.button !== 0 || !event.isPrimary || gesture) return;
    const target = event.currentTarget as HTMLElement,
      surface = target.closest<HTMLElement>("[data-widget-list]");
    if (!surface) return;
    event.preventDefault();
    event.stopPropagation();
    active.value = id;
    gesture = {
      pointer: event.pointerId,
      target,
      surface,
      slots: [
        ...surface.querySelectorAll<HTMLElement>("[data-widget-row]"),
      ].map((row) => ({
        x: row.offsetLeft,
        y: row.offsetTop,
        width: row.offsetWidth,
        height: row.offsetHeight,
      })),
    };
    target.setPointerCapture(event.pointerId);
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    document.addEventListener("keydown", escape, true);
  }
  function nudge(event: KeyboardEvent, id: string) {
    if (!event.altKey || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
    event.preventDefault();
    const list = [...items.value],
      from = list.findIndex((item) => key(item) === id),
      to = from + (event.key === "ArrowUp" ? -1 : 1);
    if (from < 0 || to < 0 || to >= list.length) return;
    list.splice(to, 0, list.splice(from, 1)[0]!);
    save(list);
  }
  onBeforeUnmount(stop);
  return { rows, active, start, nudge };
}
