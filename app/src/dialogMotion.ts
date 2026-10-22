const returnFocus = new WeakMap<Element, HTMLElement | null>();

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
