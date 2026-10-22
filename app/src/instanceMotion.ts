export interface ScreenPoint {
  x: number;
  y: number;
}

export function instanceTranslation(point: ScreenPoint) {
  return `translate3d(${point.x}px, ${point.y}px, 0) translate(-50%, -50%)`;
}

/** Keep a released drag preview visible until it reaches its final layout slot. */
export function createInstanceLanding(
  element: Pick<HTMLElement, "getBoundingClientRect" | "animate" | "style">,
  onComplete: () => void,
  reducedMotion = false,
) {
  let active: Animation | undefined;
  let generation = 0;
  let disposed = false;
  return {
    moveTo(target: ScreenPoint) {
      if (disposed) return;
      const ticket = ++generation;
      const box = element.getBoundingClientRect();
      const from = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
      active?.cancel();
      active = undefined;
      element.style.transform = instanceTranslation(target);
      if (
        reducedMotion ||
        Math.hypot(from.x - target.x, from.y - target.y) < 0.5
      ) {
        onComplete();
        return;
      }
      active = element.animate(
        [
          { transform: instanceTranslation(from) },
          { transform: instanceTranslation(target) },
        ],
        { duration: 360, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
      );
      active.finished.then(
        () => {
          if (!disposed && ticket === generation) {
            active = undefined;
            onComplete();
          }
        },
        () => {
          /* Retargeting or unmounting cancels the preceding animation. */
        },
      );
    },
    dispose() {
      disposed = true;
      generation++;
      active?.cancel();
      active = undefined;
    },
  };
}
