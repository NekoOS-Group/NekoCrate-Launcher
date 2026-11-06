import { shallowRef, type InjectionKey, type Ref } from "vue";

export const windowControlsKey: InjectionKey<{
  expanded: Ref<boolean>;
  toggle: () => void;
}> = Symbol("window-controls");

type Tool = { id: string; focus: () => void; close: () => void };
const windows: Tool[] = [];
export const toolOrder = shallowRef<string[]>([]);
function publish() {
  toolOrder.value = windows.map((item) => item.id);
}
export function raiseToolWindow(id: string) {
  const index = windows.findIndex((item) => item.id === id);
  if (index < 0) return false;
  const [item] = windows.splice(index, 1);
  windows.push(item);
  publish();
  return true;
}
export function focusToolWindow(id: string) {
  if (!raiseToolWindow(id)) return false;
  windows.at(-1)?.focus();
  return true;
}
export function registerToolWindow(tool: Tool) {
  windows.push(tool);
  publish();
  return () => {
    const index = windows.indexOf(tool);
    if (index < 0) return;
    const wasTop = index === windows.length - 1;
    windows.splice(index, 1);
    publish();
    if (wasTop) windows.at(-1)?.focus();
  };
}
