<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { t } from "../i18n";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [open: boolean] }>();
const root = ref<HTMLElement>();
const panel = ref<HTMLElement>();
function trigger() {
  return root.value?.querySelector<HTMLButtonElement>(".account-card");
}
function close(restoreFocus = false) {
  if (!props.open) return;
  emit("update:open", false);
  if (restoreFocus) trigger()?.focus({ preventScroll: true });
}
function outside(event: PointerEvent) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
    close();
}
function leave(event: FocusEvent) {
  // A removed avatar or a browser taking focus has no related target. Neither
  // should discard an in-progress name edit or interrupt browser authorization.
  if (
    event.relatedTarget instanceof Node &&
    !root.value?.contains(event.relatedTarget)
  )
    close();
}
async function navigate(event: KeyboardEvent) {
  if (event.key === "Escape" && props.open) {
    event.preventDefault();
    event.stopPropagation();
    close(true);
    return;
  }
  if ((event.target as HTMLElement)?.closest("input")) return;
  if (
    ![
      "ArrowDown",
      "ArrowUp",
      "ArrowLeft",
      "ArrowRight",
      "Home",
      "End",
    ].includes(event.key)
  )
    return;
  event.preventDefault();
  if (!props.open) {
    emit("update:open", true);
    await nextTick();
  }
  if (!props.open) return;
  const items = Array.from(
    panel.value?.querySelectorAll<HTMLButtonElement>(
      ".account-select:not(:disabled), .account-add:not(:disabled)",
    ) || [],
  );
  if (!items.length) return;
  const current = items.indexOf(document.activeElement as HTMLButtonElement);
  const backwards = ["ArrowUp", "ArrowLeft"].includes(event.key);
  const index =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : current < 0
          ? backwards
            ? items.length - 1
            : 0
          : (current + (backwards ? -1 : 1) + items.length) % items.length;
  items[index].focus({ preventScroll: true });
  items[index].scrollIntoView({ block: "nearest", inline: "nearest" });
}
watch(
  () => props.open,
  (open) => {
    if (!open && panel.value?.contains(document.activeElement))
      trigger()?.focus({ preventScroll: true });
  },
);
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", outside, true),
);
</script>

<template>
  <div
    ref="root"
    class="account-switcher"
    @focusout="leave"
    @keydown="navigate"
  >
    <slot name="trigger" />
    <Transition name="account-dropdown">
      <div
        v-if="open"
        id="account-options"
        ref="panel"
        class="account-dropdown glass-surface"
        role="group"
        :aria-label="t('账户管理')"
        @keydown.esc.stop.prevent="close(true)"
      >
        <slot />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.account-switcher {
  position: relative;
  flex-shrink: 0;
}
.account-dropdown {
  position: absolute;
  top: calc(100% + 10px);
  left: 0;
  width: 100%;
  max-width: calc(100vw - 2 * var(--edge));
  max-height: calc(100vh - 2 * var(--edge) - 76px);
  padding: 8px;
  overflow: auto;
  overscroll-behavior: contain;
  z-index: 1;
  transform-origin: top left;
}
.account-dropdown-enter-active {
  transition:
    opacity 140ms var(--motion-ease),
    transform 240ms var(--motion-ease);
}
.account-dropdown-leave-active {
  transition:
    opacity 100ms var(--motion-ease),
    transform 160ms var(--motion-ease);
  pointer-events: none;
}
.account-dropdown-enter-from,
.account-dropdown-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
@media (prefers-reduced-motion: reduce) {
  .account-dropdown-enter-active,
  .account-dropdown-leave-active {
    transition: none;
  }
}
</style>
