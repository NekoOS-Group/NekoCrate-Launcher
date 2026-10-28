import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
  type Ref,
} from "vue";
import { moveInstanceBefore as moveBefore } from "./instanceOrder.ts";
import {
  createInstanceLanding,
  instanceTranslation,
} from "./instanceMotion.ts";

/** Hit test layout positions, never the animated FLIP rectangles. */
export function versionDropTarget(
  positions: readonly { id: string; top: number; height: number }[],
  source: string,
  y: number,
) {
  return (
    positions.find(
      (item) => item.id !== source && y < item.top + item.height / 2,
    )?.id ?? null
  );
}

export function useVersionSorting<T extends { id: string }>(
  items: Readonly<Ref<T[]>>,
  blocked: Readonly<Ref<boolean>>,
  move: (id: string, before: string | null) => Promise<boolean>,
  remove: (id: string) => Promise<boolean>,
) {
  const list = ref<HTMLElement>();
  const ghost = ref<HTMLElement>();
  const trash = ref<HTMLElement>();
  const dragged = shallowRef<T>();
  const settling = ref(false);
  const removing = ref(false);
  const hovered = ref(false);
  const inside = ref(false);
  const before = ref<string | null>(null);
  const position = ref({
    transform: instanceTranslation({ x: 0, y: 0 }),
    width: "0px",
    height: "0px",
    color: "",
    textShadow: "",
  });
  const displayItems = computed(() =>
    dragged.value && inside.value
      ? moveBefore(items.value, dragged.value.id, before.value)
      : items.value,
  );
  let pointer:
    | {
        id: number;
        source: string;
        target: HTMLElement;
        startX: number;
        startY: number;
        x: number;
        y: number;
        dx: number;
        dy: number;
      }
    | undefined;
  let frame = 0,
    generation = 0;
  let suppressed = false,
    disposed = false;
  let landing: ReturnType<typeof createInstanceLanding> | undefined;
  let removal: Animation | undefined;
  const reduced = () =>
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  function clear() {
    generation++;
    landing?.dispose();
    landing = undefined;
    removal?.cancel();
    removal = undefined;
    dragged.value = undefined;
    inside.value = hovered.value = settling.value = removing.value = false;
  }
  function resetClick() {
    if (!pointer && !settling.value) suppressed = false;
  }
  function guardClick(event: MouseEvent) {
    if (!suppressed || event.detail === 0) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }
  function update() {
    if (!pointer || !dragged.value || !list.value) return;
    const p = pointer,
      surface = list.value;
    const contains = (r: DOMRect, pad = 0) =>
      p.x >= r.left - pad &&
      p.x <= r.right + pad &&
      p.y >= r.top - pad &&
      p.y <= r.bottom + pad;
    const bin = trash.value?.getBoundingClientRect();
    hovered.value = !!bin && contains(bin);
    const box = surface.getBoundingClientRect();
    inside.value = !hovered.value && contains(box, 16);
    position.value = {
      ...position.value,
      transform: instanceTranslation({ x: p.x + p.dx, y: p.y + p.dy }),
    };
    if (!inside.value) return;
    const delta =
      p.y < box.top + 28
        ? p.y - box.top - 28
        : p.y > box.bottom - 28
          ? p.y - box.bottom + 28
          : 0;
    surface.scrollTop += Math.max(-8, Math.min(8, delta / 3));
    before.value = versionDropTarget(
      Array.from(
        surface.querySelectorAll<HTMLElement>("[data-version-id]"),
        (row) => ({
          id: row.dataset.versionId!,
          top: box.top + row.offsetTop - surface.scrollTop,
          height: row.offsetHeight,
        }),
      ),
      dragged.value.id,
      p.y,
    );
  }
  function tick() {
    update();
    frame = requestAnimationFrame(tick);
  }
  function start(event: PointerEvent, id: string) {
    resetClick();
    if (
      blocked.value ||
      settling.value ||
      pointer ||
      event.button !== 0 ||
      !event.isPrimary
    )
      return;
    const target = event.currentTarget as HTMLElement,
      box = target.getBoundingClientRect();
    pointer = {
      id: event.pointerId,
      source: id,
      target,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      dx: box.left + box.width / 2 - event.clientX,
      dy: box.top + box.height / 2 - event.clientY,
    };
    target.setPointerCapture(event.pointerId);
  }
  function track(event: PointerEvent) {
    if (!pointer || event.pointerId !== pointer.id) return;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    if (
      !dragged.value &&
      Math.hypot(pointer.x - pointer.startX, pointer.y - pointer.startY) >= 6
    ) {
      const item = items.value.find((item) => item.id === pointer!.source);
      if (!item || !list.value) {
        cancel();
        return;
      }
      const box = pointer.target.getBoundingClientRect(),
        style = getComputedStyle(pointer.target);
      position.value = {
        ...position.value,
        width: `${box.width}px`,
        height: `${box.height}px`,
        color: style.color,
        textShadow: style.textShadow,
      };
      dragged.value = item;
      suppressed = true;
      const old = pointer.target;
      pointer.target = list.value;
      if (old.hasPointerCapture(pointer.id))
        old.releasePointerCapture(pointer.id);
      pointer.target.setPointerCapture(pointer.id);
      tick();
    }
    if (dragged.value) event.preventDefault();
  }
  async function land(ticket: number) {
    await nextTick();
    if (disposed || ticket !== generation) return;
    const row = Array.from(
      list.value?.querySelectorAll<HTMLElement>("[data-version-id]") || [],
    ).find((row) => row.dataset.versionId === dragged.value?.id);
    if (!row || !list.value || !ghost.value) {
      clear();
      return;
    }
    const surface = list.value;
    if (row.offsetTop < surface.scrollTop) surface.scrollTop = row.offsetTop;
    else if (
      row.offsetTop + row.offsetHeight >
      surface.scrollTop + surface.clientHeight
    )
      surface.scrollTop =
        row.offsetTop + row.offsetHeight - surface.clientHeight;
    const box = surface.getBoundingClientRect();
    landing = createInstanceLanding(ghost.value, clear, reduced());
    landing.moveTo({
      x: box.left + row.offsetLeft + row.offsetWidth / 2,
      y: box.top + row.offsetTop - surface.scrollTop + row.offsetHeight / 2,
    });
  }
  async function finish(commit = false) {
    if (!pointer) return;
    update();
    const p = pointer,
      id = dragged.value?.id,
      target = before.value;
    const deleting = !!(commit && id && hovered.value && !blocked.value);
    const reordering = !!(commit && id && inside.value && !blocked.value);
    pointer = undefined;
    cancelAnimationFrame(frame);
    frame = 0;
    if (p.target.hasPointerCapture(p.id)) p.target.releasePointerCapture(p.id);
    if (!id) return;
    settling.value = true;
    removing.value = deleting;
    const ticket = ++generation;
    if (deleting) {
      let removed = false;
      try {
        removed = await remove(id);
      } catch {
        /* Caller reports IPC failures. */
      }
      if (disposed || ticket !== generation) return;
      if (removed) {
        if (ghost.value && trash.value && !reduced()) {
          const box = trash.value.getBoundingClientRect();
          removal = ghost.value.animate(
            [
              { transform: position.value.transform, opacity: 1 },
              {
                transform: `${instanceTranslation({ x: box.left + box.width / 2, y: box.top + box.height / 2 })} scale(.2)`,
                opacity: 0,
              },
            ],
            {
              duration: 220,
              easing: "cubic-bezier(.16,1,.3,1)",
              fill: "forwards",
            },
          );
          await removal.finished.catch(() => {});
        }
        if (!disposed && ticket === generation) clear();
        return;
      }
    } else if (reordering) {
      try {
        await move(id, target);
      } catch {
        /* Restore the row after a rejected move. */
      }
    }
    if (disposed || ticket !== generation) return;
    inside.value = hovered.value = removing.value = false;
    void land(ticket);
  }
  function release(event: PointerEvent) {
    if (!pointer || event.pointerId !== pointer.id) return;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    void finish(true);
  }
  function cancel() {
    void finish();
  }
  function lostCapture(event: PointerEvent) {
    if (
      pointer &&
      event.target === pointer.target &&
      !pointer.target.hasPointerCapture(pointer.id)
    )
      cancel();
  }
  function escape(event: KeyboardEvent) {
    if (event.key === "Escape" && pointer) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  }
  function visibility() {
    if (document.hidden) cancel();
  }
  watch(blocked, (value) => {
    if (value && !settling.value) cancel();
  });
  watch(items, (value) => {
    if (
      dragged.value &&
      !settling.value &&
      !value.some((item) => item.id === dragged.value!.id)
    ) {
      cancel();
      clear();
    }
  });
  onMounted(() => {
    window.addEventListener("pointermove", track, { passive: false });
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    document.addEventListener("keydown", escape, true);
    document.addEventListener("pointerdown", resetClick, true);
    document.addEventListener("click", guardClick, true);
    document.addEventListener("visibilitychange", visibility);
  });
  onBeforeUnmount(() => {
    disposed = true;
    cancel();
    clear();
    window.removeEventListener("pointermove", track);
    window.removeEventListener("pointerup", release);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("blur", cancel);
    document.removeEventListener("keydown", escape, true);
    document.removeEventListener("pointerdown", resetClick, true);
    document.removeEventListener("click", guardClick, true);
    document.removeEventListener("visibilitychange", visibility);
  });
  return {
    list,
    ghost,
    trash,
    dragged,
    displayItems,
    settling,
    removing,
    hovered,
    position,
    start,
    cancel,
    lostCapture,
  };
}
