<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { dropInteraction } from "../dropSurface";
import { CheckCircle2 } from "lucide-vue-next";
import {
  rememberDialogFocus,
  pinPanelLeave,
  resetPanelLeave,
} from "../dialogMotion";
import { registerDialog } from "../dialogSurface";
const props = defineProps<{
  label: string;
  locked?: boolean;
  notice?: string;
}>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
let focusBeforeDrop: HTMLElement | null = null;
watch(
  dropInteraction,
  (active) => {
    const surface = dialog.value;
    if (!surface?.open) return;
    if (active) focusBeforeDrop = document.activeElement as HTMLElement;
    // Native modal dialogs make the existing dock inert. Temporarily use the
    // same dialog non-modally during a drag, retaining all its component state.
    surface.close();
    if (active) surface.show();
    else surface.showModal();
    if (!active && focusBeforeDrop?.isConnected)
      focusBeforeDrop.focus({ preventScroll: true });
  },
  { flush: "sync" },
);
let unregister: (() => void) | undefined;
onBeforeUnmount(() => unregister?.());
function close() {
  if (!props.locked) emit("close");
}
onMounted(() => {
  if (dialog.value) rememberDialogFocus(dialog.value);
  if (dropInteraction.value) dialog.value?.show();
  else dialog.value?.showModal();
  if (dialog.value) unregister = registerDialog(dialog.value);
  // Start on the static title, keeping Tab navigation inside the dialog
  // without highlighting or arming its first action button.
  dialog.value
    ?.querySelector<HTMLElement>(".dialog-heading")
    ?.focus({ preventScroll: true });
});
</script>

<template>
  <dialog
    ref="dialog"
    class="overlay-dialog"
    data-drop-kind="blocked"
    :class="{ 'drop-interaction': dropInteraction }"
    :aria-label="label"
    @cancel.prevent="close"
    @click.self="!dropInteraction && close()"
  >
    <Transition
      name="dialog-panel"
      @before-leave="pinPanelLeave"
      @after-leave="resetPanelLeave"
      @leave-cancelled="resetPanelLeave"
      ><slot
    /></Transition>
    <div v-if="notice" class="dialog-notice glass-surface" role="status">
      <CheckCircle2 :size="15" />{{ notice }}
    </div>
  </dialog>
</template>
