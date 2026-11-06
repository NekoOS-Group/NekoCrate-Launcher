import { shallowRef } from "vue";

// Notifications must stay in the native modal's top layer, outside its panel.
export const activeDialog = shallowRef<HTMLElement | null>(null);
const dialogs: { element: HTMLElement; modal: boolean }[] = [];
function update() {
  activeDialog.value =
    ([...dialogs].reverse().find((item) => item.modal) || dialogs.at(-1))
      ?.element || null;
}
export function activateDialog(dialog: HTMLElement) {
  const index = dialogs.findIndex((item) => item.element === dialog);
  if (index < 0) return;
  dialogs.push(...dialogs.splice(index, 1));
  update();
}
export function registerDialog(dialog: HTMLElement, modal = true) {
  const entry = { element: dialog, modal };
  dialogs.push(entry);
  update();
  return () => {
    const index = dialogs.indexOf(entry);
    if (index >= 0) dialogs.splice(index, 1);
    update();
  };
}
