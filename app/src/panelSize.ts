import type { ObjectDirective } from "vue";

/** Observe intrinsic children, not the animated panel, to avoid feedback loops.
 * CSS max-height keeps the panel inside its dialog; excess content can scroll.
 */
export function createPanelSizer(panel: HTMLElement) {
  const children = Array.from(panel.children) as HTMLElement[];
  const previous = panel.style.height;
  const px = (value: string) => Number.parseFloat(value) || 0;
  const measure = () => {
    // Child directives mount before GlassDialog calls showModal(). Never pin
    // the initial size to zero while its native dialog is still hidden.
    if (!panel.getClientRects().length) return;
    const style = getComputedStyle(panel);
    let height =
      px(style.paddingTop) +
      px(style.paddingBottom) +
      px(style.borderTopWidth) +
      px(style.borderBottomWidth);
    for (const child of children) {
      const style = getComputedStyle(child);
      height +=
        child.offsetHeight + px(style.marginTop) + px(style.marginBottom);
    }
    const target = `${Math.ceil(height)}px`;
    if (panel.style.height !== target) panel.style.height = target;
  };
  measure();
  panel.classList.add("panel-size-ready");
  const observer = new ResizeObserver(measure);
  for (const child of children) observer.observe(child, { box: "border-box" });
  return () => {
    observer.disconnect();
    panel.classList.remove("panel-size-ready");
    panel.style.height = previous;
  };
}

const active = new WeakMap<HTMLElement, () => void>();
function update(panel: HTMLElement, enabled: boolean) {
  if (enabled && !active.has(panel)) active.set(panel, createPanelSizer(panel));
  else if (!enabled) {
    active.get(panel)?.();
    active.delete(panel);
  }
}
export const vPanelSize: ObjectDirective<HTMLElement, boolean> = {
  mounted: (panel, binding) => update(panel, binding.value),
  updated: (panel, binding) => update(panel, binding.value),
  beforeUnmount: (panel) => update(panel, false),
};
