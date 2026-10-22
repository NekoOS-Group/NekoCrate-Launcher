import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
} from "vue";
import type { Instance } from "./types";
import { instanceDropTarget, moveInstanceBefore } from "./instanceOrder";
import { createInstanceLanding, instanceTranslation } from "./instanceMotion";

export function useInstanceSorting(
  instances: Readonly<Ref<Instance[]>>,
  blocked: Readonly<Ref<boolean>>,
  move: (id: string, beforeId: string | null) => void,
  hideTooltip: () => void,
) {
  const strip = ref<HTMLElement>();
  const ghost = ref<HTMLElement>();
  const settling = ref(false);
  const draggedId = ref<string | null>(null);
  const beforeId = ref<string | null>(null);
  const dropAllowed = ref(false);
  const ghostPosition = ref({ transform: instanceTranslation({ x: 0, y: 0 }) });
  const ghostArtStyle = ref({ opacity: "1", filter: "none" });
  const draggedInstance = computed(() =>
    instances.value.find((item) => item.id === draggedId.value),
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
  let settlement = 0;
  let disposed = false;

  function clearPreview() {
    settlement++;
    landing?.dispose();
    landing = undefined;
    settling.value = false;
    draggedId.value = null;
    dropAllowed.value = false;
  }
  async function landPreview() {
    const ticket = ++settlement;
    await nextTick();
    if (disposed || ticket !== settlement || !settling.value) return;
    const list = strip.value;
    const tile = Array.from(
      list?.querySelectorAll<HTMLElement>("[data-instance-id]") ?? [],
    ).find((item) => item.dataset.instanceId === draggedId.value);
    if (!list || !tile || !ghost.value) {
      clearPreview();
      return;
    }
    const left = tile.offsetLeft;
    if (left < list.scrollLeft + 4) list.scrollLeft = left - 4;
    else if (left + tile.offsetWidth > list.scrollLeft + list.clientWidth - 4)
      list.scrollLeft = left + tile.offsetWidth - list.clientWidth + 4;
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

  function updateTarget() {
    const list = strip.value;
    if (!pointer || !draggedId.value || !list) return;
    const box = list.getBoundingClientRect();
    dropAllowed.value =
      pointer.x >= box.left - 16 &&
      pointer.x <= box.right + 16 &&
      pointer.y >= box.top - 24 &&
      pointer.y <= box.bottom + 24;
    ghostPosition.value = { transform: instanceTranslation(pointer) };
    if (!dropAllowed.value) return;
    const edge = Math.min(28, box.width / 4);
    const delta =
      pointer.x < box.left + edge
        ? pointer.x - box.left - edge
        : pointer.x > box.right - edge
          ? pointer.x - box.right + edge
          : 0;
    list.scrollLeft += Math.max(-8, Math.min(8, delta / 3));
    const positions = Array.from(
      list.querySelectorAll<HTMLElement>("[data-instance-id]"),
      (item) => {
        return {
          id: item.dataset.instanceId!,
          // Layout coordinates ignore FLIP animation transforms, so an item
          // moving out of the way cannot move the drop boundary back and forth.
          left: box.left + item.offsetLeft - list.scrollLeft,
          width: item.offsetWidth,
        };
      },
    );
    beforeId.value = instanceDropTarget(positions, draggedId.value, pointer.x);
  }
  function animate() {
    updateTarget();
    frame = requestAnimationFrame(animate);
  }
  function start(event: PointerEvent, id: string) {
    if (
      blocked.value ||
      settling.value ||
      instances.value.length < 2 ||
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
    pointer = null;
    cancelAnimationFrame(frame);
    frame = 0;
    if (previous.target.hasPointerCapture(previous.id))
      previous.target.releasePointerCapture(previous.id);
    if (!id) return;
    settling.value = true;
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
    if (event.key === "ArrowLeft" && index > 0)
      move(id, instances.value[index - 1].id);
    if (
      event.key === "ArrowRight" &&
      index >= 0 &&
      index < instances.value.length - 1
    )
      move(id, instances.value[index + 2]?.id ?? null);
  }
  function escape(event: KeyboardEvent) {
    if (event.key === "Escape" && pointer) {
      event.preventDefault();
      cancel();
    }
  }
  function visibility() {
    if (document.hidden) cancel();
  }
  watch(blocked, (value) => {
    if (value) cancel();
  });
  watch(instances, (items) => {
    if (draggedId.value && !items.some((item) => item.id === draggedId.value)) {
      cancel();
      clearPreview();
    } else if (settling.value) void landPreview();
  });
  onMounted(() => {
    window.addEventListener("blur", cancel);
    document.addEventListener("keydown", escape);
    document.addEventListener("visibilitychange", visibility);
  });
  onBeforeUnmount(() => {
    disposed = true;
    cancel();
    clearPreview();
    window.removeEventListener("blur", cancel);
    document.removeEventListener("keydown", escape);
    document.removeEventListener("visibilitychange", visibility);
  });
  return {
    strip,
    ghost,
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
