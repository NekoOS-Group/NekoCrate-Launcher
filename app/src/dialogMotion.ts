const returnFocus = new WeakMap<Element, HTMLElement | null>();
const pinnedPanels = new WeakMap<HTMLElement, () => void>();

/** A replaced panel leaves outside layout, so two panels cannot push each other. */
export function pinPanelLeave(element: Element) {
  if (!(element instanceof HTMLElement)) return;
  resetPanelLeave(element);
  const rect = element.getBoundingClientRect();
  const properties = [
    "position",
    "left",
    "top",
    "width",
    "height",
    "margin",
    "maxWidth",
    "maxHeight",
  ] as const;
  const original = properties.map((key) => [key, element.style[key]] as const);
  const inert = element.inert;
  const hidden = element.getAttribute("aria-hidden");
  pinnedPanels.set(element, () => {
    for (const [key, value] of original) element.style[key] = value;
    element.inert = inert;
    if (hidden === null) element.removeAttribute("aria-hidden");
    else element.setAttribute("aria-hidden", hidden);
  });
  Object.assign(element.style, {
    position: "fixed",
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
    margin: "0",
    maxWidth: "none",
    maxHeight: "none",
  });
  element.inert = true;
  element.setAttribute("aria-hidden", "true");
}
export function resetPanelLeave(element: Element) {
  const panel = element as HTMLElement;
  pinnedPanels.get(panel)?.();
  pinnedPanels.delete(panel);
}

export function rememberDialogFocus(dialog: HTMLDialogElement) {
  let previous =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  const closing = previous?.closest("dialog[data-closing]");
  if (closing) previous = returnFocus.get(closing) ?? null;
  returnFocus.set(dialog, previous);
}

export function prepareDialogLeave(element: Element) {
  if (!(element instanceof HTMLDialogElement)) return;
  element.dataset.closing = "true";
  element.inert = true;
}

export function finishDialogLeave(element: Element) {
  if (!(element instanceof HTMLDialogElement)) return;
  const previous = returnFocus.get(element);
  returnFocus.delete(element);
  element.close();
  if (
    !document.querySelector("dialog[open]:not([data-closing])") &&
    previous?.isConnected
  )
    previous.focus({ preventScroll: true });
}
