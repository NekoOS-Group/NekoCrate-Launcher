import { onBeforeUnmount, onBeforeUpdate, onUpdated, type Ref } from "vue";

// Keep the visual position of each header element while its natural layout changes.
// Only measure around updates; there is no animation-frame layout polling.
export function useHorizontalLayout(root: Ref<HTMLElement | undefined>) {
  let positions = new Map<string, number>();
  const motions = new Map<HTMLElement, Animation>();
  const elements = () =>
    Array.from(
      root.value?.querySelectorAll<HTMLElement>("[data-slide-key]") || [],
    );
  const stop = () => {
    motions.forEach((motion) => motion.cancel());
    motions.clear();
  };
  onBeforeUpdate(() => {
    positions = new Map(
      elements().map((element) => [
        element.dataset.slideKey!,
        element.getBoundingClientRect().left,
      ]),
    );
  });
  onUpdated(() => {
    stop();
    if (
      typeof matchMedia === "undefined" ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    )
      return;
    const moves = elements().map((element) => ({
      element,
      from: positions.get(element.dataset.slideKey!),
      to: element.getBoundingClientRect().left,
    }));
    for (const { element, from, to } of moves) {
      if (from === undefined || Math.abs(from - to) < 0.5 || !element.animate)
        continue;
      const motion = element.animate(
        [
          { transform: `translateX(${from - to}px)` },
          { transform: "translateX(0)" },
        ],
        { duration: 420, easing: "cubic-bezier(.16,1,.3,1)" },
      );
      motions.set(element, motion);
      motion.onfinish = () => {
        if (motions.get(element) === motion) motions.delete(element);
      };
    }
  });
  onBeforeUnmount(stop);
}
