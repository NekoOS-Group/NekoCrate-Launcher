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
  dropHint.value = {
    left: box?.left ?? 14,
    top: box?.top ?? 14,
    width: box?.width ?? window.innerWidth - 28,
    height: box?.height ?? window.innerHeight - 28,
    message,
    invalid,
  };
}
export function clearDropHint() {
  dropHint.value = undefined;
}
