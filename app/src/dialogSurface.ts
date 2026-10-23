import { shallowRef } from "vue";

// Notifications must stay in the native modal's top layer, outside its panel.
export const activeDialog = shallowRef<HTMLElement | null>(null);
const dialogs: HTMLElement[] = [];
export function registerDialog(dialog: HTMLElement) {
  dialogs.push(dialog);
  activeDialog.value = dialog;
  return () => {
    const index = dialogs.indexOf(dialog);
    if (index >= 0) dialogs.splice(index, 1);
    activeDialog.value = dialogs.at(-1) || null;
  };
}
