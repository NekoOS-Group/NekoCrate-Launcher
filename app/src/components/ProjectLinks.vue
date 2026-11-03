<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId } from "vue";
import { ExternalLink, Link2 } from "lucide-vue-next";
import { floatingMenuPosition } from "../floatingMenu";
import { projectLink } from "../projectPresentation";
import { t } from "../i18n";

defineProps<{
  links: readonly { label: string; url: string }[];
  license?: { name: string; url?: string };
}>();
const emit = defineEmits<{ link: [url: string] }>();
const id = useId();
const trigger = ref<HTMLButtonElement>();
const menu = ref<HTMLElement>();
const portal = ref<HTMLElement>();
const expanded = ref(false);
const position = ref<ReturnType<typeof floatingMenuPosition>>();
let frame: number | undefined;
function track() {
  if (!expanded.value || !trigger.value) return;
  const bounds = trigger.value.getBoundingClientRect();
  const next = floatingMenuPosition(
    { left: bounds.right - 192, bottom: bounds.bottom, width: 192 },
    { width: window.innerWidth, height: window.innerHeight },
  );
  if (JSON.stringify(position.value) !== JSON.stringify(next))
    position.value = next;
  frame = requestAnimationFrame(track);
}
function items() {
  return Array.from(
    menu.value?.querySelectorAll<HTMLButtonElement>("[role='menuitem']") || [],
  );
}
function focus(index: number) {
  const options = items();
  options[(index + options.length) % options.length]?.focus({
    preventScroll: true,
  });
}
async function open(last = false) {
  if (expanded.value) return;
  portal.value = trigger.value?.closest("dialog") || document.body;
  expanded.value = true;
  track();
  await nextTick();
  if (expanded.value) focus(last ? items().length - 1 : 0);
}
function close(restore = false) {
  expanded.value = false;
  if (frame !== undefined) cancelAnimationFrame(frame);
  frame = undefined;
  if (restore) trigger.value?.focus({ preventScroll: true });
}
function choose(value: string) {
  const url = projectLink(value);
  close(true);
  if (url) emit("link", url);
}
function navigate(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    close(true);
    return;
  }
  const index = items().indexOf(document.activeElement as HTMLButtonElement);
  const next =
    event.key === "ArrowDown"
      ? index + 1
      : event.key === "ArrowUp"
        ? index - 1
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? items().length - 1
            : null;
  if (next !== null) {
    event.preventDefault();
    focus(next);
  }
}
function contains(target: EventTarget | null) {
  return (
    target instanceof Node &&
    (trigger.value?.contains(target) || menu.value?.contains(target))
  );
}
function outside(event: PointerEvent) {
  if (!contains(event.target)) close();
}
function leave(event: FocusEvent) {
  if (!contains(event.relatedTarget)) close();
}
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() => {
  close();
  document.removeEventListener("pointerdown", outside, true);
});
</script>

<template>
  <div class="project-links" @focusout="leave">
    <button
      ref="trigger"
      class="icon-button project-links-trigger"
      type="button"
      :aria-label="t('链接')"
      :title="t('链接')"
      aria-haspopup="menu"
      :aria-controls="id"
      :aria-expanded="expanded"
      @click="expanded ? close() : open()"
      @keydown.down.prevent="open()"
      @keydown.up.prevent="open(true)"
      @keydown.esc.stop.prevent="close(true)"
    >
      <Link2 :size="17" />
    </button>
    <Teleport v-if="portal" :to="portal">
      <Transition name="source-menu">
        <div
          v-if="expanded"
          :id="id"
          ref="menu"
          role="menu"
          :aria-label="t('链接')"
          class="source-select-menu source-select-floating source-select-compact glass-surface project-links-menu"
          :style="position"
          @keydown.stop="navigate"
          @focusout.stop="leave"
        >
          <button
            v-for="item in links"
            :key="item.url"
            type="button"
            role="menuitem"
            tabindex="-1"
            class="source-select-option"
            @pointerdown.prevent
            @click="choose(item.url)"
          >
            <span>{{ t(item.label) }}</span
            ><ExternalLink :size="12" />
          </button>
          <button
            v-if="license"
            type="button"
            role="menuitem"
            tabindex="-1"
            class="source-select-option project-license"
            :aria-disabled="!license.url || undefined"
            :aria-label="`${t('许可证')}: ${license.name}`"
            :title="license.name"
            @pointerdown.prevent
            @click="license.url && choose(license.url)"
          >
            <span class="project-license-label"
              ><small>{{ t("许可证") }}</small
              ><span>{{ license.name }}</span></span
            >
            <ExternalLink v-if="license.url" :size="12" />
          </button>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.project-links {
  display: flex;
  flex-shrink: 0;
}
.project-links-trigger {
  width: 34px;
  height: 34px;
}
.project-links-trigger:hover,
.project-links-trigger[aria-expanded="true"] {
  color: var(--blue);
  background: transparent;
  filter: drop-shadow(0 0 6px var(--glow));
}
.project-links-menu {
  overflow-y: auto;
  scrollbar-width: none;
}
.project-links-menu button {
  justify-content: space-between;
  gap: 16px;
  font-size: 12px;
}
.project-links-menu button:hover,
.project-links-menu button:focus-visible {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
  outline: none;
}
.project-license-label {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.project-license-label small {
  color: var(--muted);
  font-size: 10px;
  flex-shrink: 0;
}
.project-license-label > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.project-license[aria-disabled="true"] {
  cursor: default;
}
</style>
