<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from "vue";
import { CheckCircle2 } from "lucide-vue-next";
const props = defineProps<{
  label: string;
  locked?: boolean;
  notice?: string;
}>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
let previousFocus: HTMLElement | null = null;
function close() {
  if (!props.locked) emit("close");
}
onMounted(() => {
  previousFocus = document.activeElement as HTMLElement | null;
  dialog.value?.showModal();
  // Start on the static title, keeping Tab navigation inside the dialog
  // without highlighting or arming its first action button.
  dialog.value
    ?.querySelector<HTMLElement>(".dialog-heading")
    ?.focus({ preventScroll: true });
});
onBeforeUnmount(() => {
  dialog.value?.close();
  previousFocus?.focus();
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
