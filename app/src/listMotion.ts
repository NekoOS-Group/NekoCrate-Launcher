/** Stable, compositor-only entrances. Long offscreen lists stay idle. */
const ROW_STAGGER_MS = 30;
const ROW_FADE_MS = 160;
const ROW_EASE = "cubic-bezier(.22,1,.36,1)";
export function listRevealStyle(index: number, initialDelay = 30) {
  return {
    "--row-delay": `${initialDelay + Math.min(index, 11) * ROW_STAGGER_MS}ms`,
    ...(index >= 12 ? { animation: "none" } : {}),
  };
}

const leaving = new WeakMap<Element, () => void>();

export function resetListLeave(element: Element) {
  leaving.get(element)?.();
  element.removeAttribute("inert");
  element.removeAttribute("aria-hidden");
}

export function leaveList(element: Element, done: () => void) {
  element.setAttribute("inert", "");
  element.setAttribute("aria-hidden", "true");
  const root = element as HTMLElement;
  if (!root.animate || matchMedia("(prefers-reduced-motion: reduce)").matches) {
    done();
    return;
  }
  const viewport =
    root.querySelector<HTMLElement>("[data-list-viewport]") || root;
  const bounds = viewport.getBoundingClientRect();
  const rows = Array.from(
    viewport.querySelectorAll<HTMLElement>(".list-reveal-item"),
  )
    .filter((row) => {
      const rect = row.getBoundingClientRect();
      return rect.bottom > bounds.top && rect.top < bounds.bottom;
    })
    .map((row) => {
      const style = getComputedStyle(row);
      return {
        row,
        opacity: style.opacity,
        transform: style.transform,
        animation: row.style.animation,
      };
    });
  let cancelled = false;
  const animations: Animation[] = [];
  const clean = () => {
    cancelled = true;
    animations.forEach((animation) => animation.cancel());
    rows.forEach(({ row, animation }) => {
      row.style.animation = animation;
    });
    leaving.delete(element);
  };
  leaving.set(element, clean);
  for (const [index, { row, opacity, transform }] of rows.entries()) {
    // Start from the current appearance even if entry is still in progress.
    row.style.animation = "none";
    const delay = Math.min(index, 11) * ROW_STAGGER_MS;
    // Complement the entering row's opacity on the same staggered timeline.
    // Position takes longer, so the fading row does not visibly snap to its end.
    animations.push(
      row.animate([{ opacity }, { opacity: 0 }], {
        duration: ROW_FADE_MS,
        delay,
        easing: ROW_EASE,
        fill: "both",
      }),
      row.animate([{ transform }, { transform: "translateY(-8px)" }], {
        duration: 240,
        delay,
        easing: ROW_EASE,
        fill: "both",
      }),
    );
  }
  if (rows.length) {
    for (const footer of root.querySelectorAll<HTMLElement>(
      "[data-list-footer]",
    )) {
      animations.push(
        footer.animate(
          [{ opacity: getComputedStyle(footer).opacity }, { opacity: 0 }],
          { duration: ROW_FADE_MS, easing: ROW_EASE, fill: "both" },
        ),
      );
    }
  }
  if (!rows.length)
    animations.push(
      root.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: 140,
        fill: "both",
      }),
    );
  void Promise.allSettled(
    animations.map((animation) => animation.finished),
  ).then(() => {
    if (cancelled) return;
    clean();
    done();
  });
}
