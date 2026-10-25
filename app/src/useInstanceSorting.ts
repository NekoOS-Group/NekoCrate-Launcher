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
import type { Instance } from "./types";
import {
  instanceGridDropTarget,
  instanceZoneDropTarget,
  moveInstanceBefore,
  VISIBLE_INSTANCES,
  instanceVisibleLimit,
} from "./instanceOrder.ts";
import {
  createInstanceLanding,
  instanceTranslation,
} from "./instanceMotion.ts";

export function useInstanceSorting(
  instances: Readonly<Ref<Instance[]>>,
  blocked: Readonly<Ref<boolean>>,
  move: (id: string, beforeId: string | null) => void,
  hideTooltip: () => void,
  removal?: {
    allowed: (id: string) => boolean;
    remove: (id: string) => Promise<boolean>;
  },
) {
  const strip = ref<HTMLElement>();
  const overflow = ref<HTMLElement>();
  const toggle = ref<HTMLElement>();
  const expanded = ref(false);
  let hoverStarted = 0;
  function setSurface(zone: "main" | "overflow", element: unknown) {
    (zone === "main" ? strip : overflow).value = (element || undefined) as
      HTMLElement | undefined;
  }
  function toggleExpanded() {
    if (draggedId.value) return;
    expanded.value = !expanded.value;
    hideTooltip();
  }
  const ghost = ref<HTMLElement>();
  const trash = ref<HTMLElement>();
  const trashHovered = ref(false);
  const removing = ref(false);
  const settling = ref(false);
  const draggedId = ref<string | null>(null);
  const draggedSource = shallowRef<Instance>();
  const trashVisible = computed(
    () => !!removal && !!draggedId.value && (!settling.value || removing.value),
  );
  const trashAllowed = computed(
    () =>
      removing.value ||
      (!!draggedId.value && !!removal?.allowed(draggedId.value)),
  );
  const beforeId = ref<string | null>(null);
  const dropAllowed = ref(false);
  const ghostPosition = ref({
    transform: instanceTranslation({ x: 0, y: 0 }),
    opacity: "1",
  });
  const ghostArtStyle = ref({ opacity: "1", filter: "none" });
  const draggedInstance = computed(
    () =>
      instances.value.find((item) => item.id === draggedId.value) ??
      (removing.value ? draggedSource.value : undefined),
  );
  const displayInstances = computed(() =>
    draggedId.value && dropAllowed.value
      ? moveInstanceBefore(instances.value, draggedId.value, beforeId.value)
      : instances.value,
  );
  let pointer: {
    id: number;
    instance: string;
    target: HTMLElement;
    startX: number;
    startY: number;
    x: number;
    y: number;
  } | null = null;
  let frame = 0;
  let suppressClick = false;
  let landing: ReturnType<typeof createInstanceLanding> | undefined;
  let removalAnimation: Animation | undefined;
  let settlement = 0;
  let disposed = false;

  function clearPreview() {
    settlement++;
    hoverStarted = 0;
    landing?.dispose();
    landing = undefined;
    removalAnimation?.cancel();
    removalAnimation = undefined;
    removing.value = false;
    trashHovered.value = false;
    settling.value = false;
    draggedId.value = null;
    draggedSource.value = undefined;
    dropAllowed.value = false;
  }
  async function landPreview() {
    const ticket = ++settlement;
    await nextTick();
    if (disposed || ticket !== settlement || !settling.value) return;
    const lists = [strip.value, ...(expanded.value ? [overflow.value] : [])];
    const list = lists.find((surface) =>
      Array.from(
        surface?.querySelectorAll<HTMLElement>("[data-instance-id]") ?? [],
      ).some((item) => item.dataset.instanceId === draggedId.value),
    );
    const tile = Array.from(
      list?.querySelectorAll<HTMLElement>("[data-instance-id]") ?? [],
    ).find((item) => item.dataset.instanceId === draggedId.value);
    if (!list || !tile || !ghost.value) {
      clearPreview();
      return;
    }
    if (list === overflow.value) {
      if (tile.offsetTop < list.scrollTop + 4)
        list.scrollTop = tile.offsetTop - 4;
      else if (
        tile.offsetTop + tile.offsetHeight >
        list.scrollTop + list.clientHeight - 4
      )
        list.scrollTop =
          tile.offsetTop + tile.offsetHeight - list.clientHeight + 4;
    }
    const box = list.getBoundingClientRect();
    landing ??= createInstanceLanding(
      ghost.value,
      clearPreview,
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    );
    landing.moveTo({
      x: box.left + tile.offsetLeft - list.scrollLeft + tile.offsetWidth / 2,
      y: box.top + tile.offsetTop - list.scrollTop + tile.offsetHeight / 2,
    });
  }

  async function removePreview(id: string) {
    const ticket = ++settlement;
    removing.value = true;
    let removed = false;
    try {
      removed = await removal!.remove(id);
    } catch {
      // The caller reports the failure. Restore the drag preview in either case.
    }
    if (disposed || ticket !== settlement) return;
    if (!removed) {
      removing.value = false;
      trashHovered.value = false;
      void landPreview();
      return;
    }
    if (
      ghost.value &&
      trash.value &&
      !window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      const from = ghost.value.getBoundingClientRect();
      const to = trash.value.getBoundingClientRect();
      removalAnimation = ghost.value.animate(
        [
          {
            transform: instanceTranslation({
              x: from.left + from.width / 2,
              y: from.top + from.height / 2,
            }),
            opacity: 1,
          },
          {
            transform: `${instanceTranslation({ x: to.left + to.width / 2, y: to.top + to.height / 2 })} scale(.2)`,
            opacity: 0,
          },
        ],
        {
          duration: 220,
          easing: "cubic-bezier(0.16, 1, 0.3, 1)",
          fill: "forwards",
        },
      );
      await removalAnimation.finished.catch(() => {});
    }
    if (!disposed && ticket === settlement) {
      // Cancelling the finished Web Animation must not reveal the full-size
      // ghost for Vue's leave transition after it has already shrunk away.
      if (ghost.value) ghost.value.style.opacity = "0";
      clearPreview();
    }
  }

  function updateTarget() {
    if (!pointer || !draggedId.value || !strip.value) return;
    const inside = (box: DOMRect, padding = 0) =>
      pointer!.x >= box.left - padding &&
      pointer!.x <= box.right + padding &&
      pointer!.y >= box.top - padding &&
      pointer!.y <= box.bottom + padding;
    const bin = trash.value?.getBoundingClientRect();
    trashHovered.value = !!bin && inside(bin);
    const trigger = toggle.value?.getBoundingClientRect();
    if (
      instances.value.length > VISIBLE_INSTANCES &&
      !expanded.value &&
      trigger &&
      inside(trigger, 8)
    ) {
      hoverStarted ||= Date.now();
      if (Date.now() - hoverStarted >= 280) expanded.value = true;
    } else hoverStarted = 0;
    const order = displayInstances.value;
    const tray = expanded.value ? overflow.value : undefined;
    const zone =
      tray && inside(tray.getBoundingClientRect(), 8) ? "overflow" : "main";
    const list = zone === "overflow" ? tray! : strip.value;
    const box = list.getBoundingClientRect();
    dropAllowed.value =
      !trashHovered.value && inside(box, zone === "main" ? 16 : 8);
    ghostPosition.value = {
      transform: instanceTranslation(pointer),
      opacity: "1",
    };
    if (!dropAllowed.value) return;
    if (zone === "overflow") {
      const delta =
        pointer.y < box.top + 28
          ? pointer.y - box.top - 28
          : pointer.y > box.bottom - 28
            ? pointer.y - box.bottom + 28
            : 0;
      list.scrollTop += Math.max(-8, Math.min(8, delta / 3));
    }
    const positions = Array.from(
      list.querySelectorAll<HTMLElement>("[data-instance-id]"),
      (item) => ({
        id: item.dataset.instanceId!,
        // Ignore FLIP transforms so the target does not oscillate as tiles move.
        left: box.left + item.offsetLeft - list.scrollLeft,
        top: box.top + item.offsetTop - list.scrollTop,
        width: item.offsetWidth,
        height: item.offsetHeight,
      }),
    );
    let target = instanceGridDropTarget(
      positions,
      draggedId.value,
      pointer.x,
      pointer.y,
    );
    if (zone === "main" && target === null)
      target =
        order
          .slice(instanceVisibleLimit(order.length))
          .find((item) => item.id !== draggedId.value)?.id ?? null;
    beforeId.value = instanceZoneDropTarget(
      order,
      draggedId.value,
      target,
      zone,
    );
  }
  function animate() {
    updateTarget();
    frame = requestAnimationFrame(animate);
  }
  function start(event: PointerEvent, id: string) {
    if (
      blocked.value ||
      settling.value ||
      instances.value.length < (removal ? 1 : 2) ||
      !instances.value.some((item) => item.id === id) ||
      event.button !== 0 ||
      !event.isPrimary ||
      pointer
    )
      return;
    const target = event.currentTarget as HTMLElement;
    pointer = {
      id: event.pointerId,
      instance: id,
      target,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
    };
    target.setPointerCapture(event.pointerId);
  }
  function track(event: PointerEvent) {
    if (!pointer || event.pointerId !== pointer.id) return;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    if (
      !draggedId.value &&
      Math.hypot(pointer.x - pointer.startX, pointer.y - pointer.startY) >= 6
    ) {
      draggedId.value = pointer.instance;
      draggedSource.value = instances.value.find(
        (item) => item.id === pointer!.instance,
      );
      const art = pointer.target.querySelector<HTMLElement>(".instance-art");
      if (art) {
        const style = getComputedStyle(art);
        ghostArtStyle.value = { opacity: style.opacity, filter: style.filter };
      }
      suppressClick = true;
      hideTooltip();
      // Capture on the stable scroll container before Vue moves the keyed tile.
      // Moving the captured tile itself can release capture in a WebView.
      if (strip.value) {
        const previous = pointer.target;
        pointer.target = strip.value;
        if (previous.hasPointerCapture(pointer.id))
          previous.releasePointerCapture(pointer.id);
        pointer.target.setPointerCapture(pointer.id);
      }
      animate();
    }
    if (draggedId.value) event.preventDefault();
  }
  function finish(commit = false) {
    if (!pointer) return;
    updateTarget();
    const previous = pointer;
    const id = draggedId.value;
    const target = beforeId.value;
    const shouldMove = commit && id && dropAllowed.value;
    const shouldRemove =
      commit && id && trashHovered.value && trashAllowed.value;
    pointer = null;
    cancelAnimationFrame(frame);
    frame = 0;
    if (previous.target.hasPointerCapture(previous.id))
      previous.target.releasePointerCapture(previous.id);
    if (!id) return;
    settling.value = true;
    if (shouldRemove) {
      dropAllowed.value = false;
      void removePreview(id);
      return;
    }
    trashHovered.value = false;
    if (shouldMove) {
      const next = moveInstanceBefore(instances.value, id, target);
      if (next.some((item, index) => item.id !== instances.value[index]?.id))
        move(id, target);
    }
    dropAllowed.value = false;
    void landPreview();
  }
  function release(event: PointerEvent) {
    if (!pointer || event.pointerId !== pointer.id) return;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    finish(true);
  }
  function cancel() {
    finish();
  }
  function lostCapture(event: PointerEvent) {
    if (
      pointer &&
      event.target === pointer.target &&
      !pointer.target.hasPointerCapture(pointer.id)
    )
      cancel();
  }
  function resetClick() {
    if (!pointer) suppressClick = false;
  }
  function guardClick(event: MouseEvent) {
    if (!suppressClick || event.detail === 0) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }
  function keyboard(event: KeyboardEvent, id: string) {
    if (!event.altKey || !["ArrowLeft", "ArrowRight"].includes(event.key))
      return;
    event.preventDefault();
    if (blocked.value || draggedId.value) return;
    const index = instances.value.findIndex((item) => item.id === id);
    const nextIndex = index + (event.key === "ArrowLeft" ? -1 : 1);
    if (index < 0 || nextIndex < 0 || nextIndex >= instances.value.length)
      return;
    if (
      Math.max(index, nextIndex) >= instanceVisibleLimit(instances.value.length)
    )
      expanded.value = true;
    move(
      id,
      event.key === "ArrowLeft"
        ? instances.value[index - 1].id
        : (instances.value[index + 2]?.id ?? null),
    );
    void nextTick(() => {
      const tile = [strip.value, ...(expanded.value ? [overflow.value] : [])]
        .flatMap((surface) =>
          Array.from(
            surface?.querySelectorAll<HTMLElement>("[data-instance-id]") ?? [],
          ),
        )
        .find((item) => item.dataset.instanceId === id);
      tile?.querySelector<HTMLButtonElement>("button")?.focus();
    });
  }

  function escape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (pointer) {
      event.preventDefault();
      cancel();
    } else if (expanded.value) {
      event.preventDefault();
      expanded.value = false;
      hideTooltip();
      toggle.value?.focus();
    }
  }
  function outside(event: PointerEvent) {
    if (!expanded.value || draggedId.value) return;
    const target = event.target as Node;
    if (
      ![strip.value, overflow.value, toggle.value].some((element) =>
        element?.contains(target),
      )
    ) {
      expanded.value = false;
      hideTooltip();
    }
  }
  function visibility() {
    if (document.hidden) cancel();
  }
  watch(blocked, (value) => {
    if (value) cancel();
  });
  watch(instances, (items) => {
    if (items.length <= VISIBLE_INSTANCES) expanded.value = false;
    // A committed removal updates the list before the ghost finishes shrinking.
    if (removing.value) return;
    if (draggedId.value && !items.some((item) => item.id === draggedId.value)) {
      cancel();
      clearPreview();
    } else if (settling.value) void landPreview();
  });
  onMounted(() => {
    window.addEventListener("blur", cancel);
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape);
    document.addEventListener("visibilitychange", visibility);
  });
  onBeforeUnmount(() => {
    disposed = true;
    cancel();
    clearPreview();
    window.removeEventListener("blur", cancel);
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", escape);
    document.removeEventListener("visibilitychange", visibility);
  });
  return {
    strip,
    overflow,
    toggle,
    expanded,
    setSurface,
    toggleExpanded,
    ghost,
    trash,
    trashHovered,
    trashVisible,
    trashAllowed,
    removing,
    draggedId,
    draggedInstance,
    displayInstances,
    beforeId,
    dropAllowed,
    ghostPosition,
    ghostArtStyle,
    start,
    track,
    release,
    cancel,
    lostCapture,
    resetClick,
    guardClick,
    keyboard,
  };
}
