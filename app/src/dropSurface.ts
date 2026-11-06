import { shallowRef } from "vue";

// Both OS file drops and in-app transfers use one target indicator.
export const dropInteraction = shallowRef(false);
export const dropHint = shallowRef<{
  left: number;
  top: number;
  width: number;
  height: number;
  message: string;
  invalid: boolean;
}>();
export function showDropHint(
  element: Element | null,
  message: string,
  invalid = false,
) {
  const box = element?.getBoundingClientRect();
  const next = {
    left: box?.left ?? 14,
    top: box?.top ?? 14,
    width: box?.width ?? window.innerWidth - 28,
    height: box?.height ?? window.innerHeight - 28,
    message,
    invalid,
  };
  const current = dropHint.value;
  // Hit testing follows layout changes while the pointer is stationary. Avoid
  // repainting an unchanged hint on every animation frame during a drag.
  if (
    current &&
    current.left === next.left &&
    current.top === next.top &&
    current.width === next.width &&
    current.height === next.height &&
    current.message === next.message &&
    current.invalid === next.invalid
  )
    return;
  dropHint.value = next;
}
export function clearDropHint() {
  dropHint.value = undefined;
}
