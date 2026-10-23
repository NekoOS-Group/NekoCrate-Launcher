<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { CheckCircle2 } from "lucide-vue-next";
import { rememberDialogFocus } from "../dialogMotion";
import { registerDialog } from "../dialogSurface";
const props = defineProps<{
  label: string;
  locked?: boolean;
  notice?: string;
}>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
let unregister: (() => void) | undefined;
onBeforeUnmount(() => unregister?.());
function close() {
  if (!props.locked) emit("close");
}
onMounted(() => {
  if (dialog.value) rememberDialogFocus(dialog.value);
  dialog.value?.showModal();
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
    :aria-label="label"
    @cancel.prevent="close"
    @click.self="close"
  >
    <slot />
    <div v-if="notice" class="dialog-notice glass-surface" role="status">
      <CheckCircle2 :size="15" />{{ notice }}
    </div>
  </dialog>
</template>
