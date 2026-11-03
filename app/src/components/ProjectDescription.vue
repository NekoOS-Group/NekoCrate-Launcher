<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { ExternalLink, X } from "lucide-vue-next";
import GlassDialog from "./GlassDialog.vue";
import { finishDialogLeave, prepareDialogLeave } from "../dialogMotion";
import { locale, t } from "../i18n";
import { projectDescription } from "../projectDescription";
import {
  projectLink,
  projectMedia,
  type ResourceRpc,
} from "../projectPresentation";
const props = defineProps<{
  body: string;
  format: "markdown" | "html";
  base: string;
  rpc: ResourceRpc;
}>();
const emit = defineEmits<{ link: [url: string] }>();
const root = ref<HTMLElement>();
const html = ref("");
const preview = ref<{ src: string; alt: string; url: string }>();
let previewTrigger: HTMLElement | undefined;
const timers = new Set<ReturnType<typeof setTimeout>>();
let generation = 0;
let observer: IntersectionObserver | undefined;
async function render() {
  const request = ++generation;
  observer?.disconnect();
  for (const timer of timers) clearTimeout(timer);
  timers.clear();
  preview.value = undefined;
  html.value = projectDescription(props.body, props.format, props.base);
  await nextTick();
  if (request !== generation) return;
  const images =
    root.value?.querySelectorAll<HTMLImageElement>("img[data-project-image]") ||
    [];
  const load = async (img: HTMLImageElement) => {
    const fallback = () => {
      if (generation === request)
        (img.closest(".description-image") || img).replaceWith(
          document.createTextNode(img.alt),
        );
    };
    try {
      const source = await projectMedia(props.rpc).image(
        img.dataset.projectImage!,
      );
      if (generation === request && root.value?.contains(img)) {
        img.onerror = fallback;
        img.src = source;
        const button = img.closest("button");
        if (button) button.disabled = false;
      }
    } catch {
      fallback();
    }
  };
  if (typeof IntersectionObserver === "undefined") {
    for (const img of images) void load(img);
    return;
  }
  observer = new IntersectionObserver((entries) => {
    for (const entry of entries)
      if (entry.isIntersecting) {
        observer?.unobserve(entry.target);
        void load(entry.target as HTMLImageElement);
      }
  });
  for (const img of images) observer.observe(img);
}
watch(
  () => [props.body, props.format, props.base, locale.value],
  () => void render(),
  { immediate: true },
);
onBeforeUnmount(() => {
  generation++;
  observer?.disconnect();
  for (const timer of timers) clearTimeout(timer);
});
function closePreview(element: Element) {
  finishDialogLeave(element);
  previewTrigger?.focus({ preventScroll: true });
}
async function copyCode(button: HTMLButtonElement) {
  const code = button.closest(".description-code")?.querySelector("code");
  if (!code || button.disabled) return;
  button.disabled = true;
  try {
    if (isTauri())
      await props.rpc("copy_text", { text: code.textContent || "" });
    else await navigator.clipboard.writeText(code.textContent || "");
    button.textContent = t("已复制");
  } catch {
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(code);
    selection?.removeAllRanges();
    selection?.addRange(range);
    button.textContent = t("请手动复制");
  } finally {
    button.disabled = false;
    if (root.value?.contains(button)) {
      const timer = setTimeout(() => {
        button.textContent = t("复制代码");
        timers.delete(timer);
      }, 1800);
      timers.add(timer);
    }
  }
}
function follow(event: MouseEvent) {
  const element = event.target instanceof Element ? event.target : null;
  const copy = element?.closest<HTMLButtonElement>("button[data-copy-code]");
  if (copy) {
    void copyCode(copy);
    return;
  }
  const imageButton = element?.closest<HTMLButtonElement>(
    "button[data-preview-image]",
  );
  const img = imageButton?.querySelector("img");
  if (img?.src && img.dataset.projectImage) {
    previewTrigger = imageButton!;
    preview.value = {
      src: img.src,
      alt: img.alt,
      url: img.dataset.projectImage,
    };
    return;
  }
  const target = element?.closest("a");
  if (!target) return;
  event.preventDefault();
  event.stopPropagation();
  const href = target.getAttribute("href") || "";
  if (href.startsWith("#")) {
    let fragment: string;
    try {
      fragment = decodeURIComponent(href.slice(1));
    } catch {
      return;
    }
    const destination = Array.from(
      root.value?.querySelectorAll<HTMLElement>("[data-project-anchor]") || [],
    ).find(
      (entry) =>
        entry.dataset.projectAnchor === fragment ||
        `user-content-${entry.dataset.projectAnchor}` === fragment,
    );
    if (destination) {
      for (
        let parent = destination.parentElement;
        parent && parent !== root.value;
        parent = parent.parentElement
      ) {
        if (parent instanceof HTMLDetailsElement) parent.open = true;
      }
      destination.scrollIntoView({
        behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
          ? "instant"
          : "smooth",
        block: "start",
      });
      destination.tabIndex = -1;
      destination.focus({ preventScroll: true });
    }
    return;
  }
  const url = projectLink(href);
  if (url) emit("link", url);
}
</script>
<template>
  <div
    ref="root"
    class="project-description"
    @click="follow"
    @auxclick="$event.button === 1 && follow($event)"
    v-html="html"
  />
  <Teleport :to="root?.closest('.shell') || 'body'">
    <Transition
      name="dialog"
      @before-leave="prepareDialogLeave"
      @after-leave="closePreview"
    >
      <GlassDialog
        v-if="preview"
        :label="t('图片预览')"
        @close="preview = undefined"
      >
        <section class="modal glass-surface project-image-preview">
          <header class="modal-header">
            <h2 class="dialog-heading" tabindex="-1">
              {{ preview.alt || t("图片预览") }}
            </h2>
            <button
              class="icon-button"
              :aria-label="t('打开原图')"
              :title="t('打开原图')"
              @click="emit('link', preview.url)"
            >
              <ExternalLink :size="16" />
            </button>
            <button
              class="icon-button"
              :aria-label="t('关闭对话框')"
              @click="preview = undefined"
            >
              <X :size="16" />
            </button>
          </header>
          <img :src="preview.src" :alt="preview.alt" />
        </section>
      </GlassDialog>
    </Transition>
  </Teleport>
</template>
<style scoped>
.project-description {
  min-width: 0;
  font-size: 13px;
  line-height: 1.85;
  overflow-wrap: anywhere;
  color: var(--ink);
  user-select: text;
}
.project-description :deep(p) {
  margin: 0 0 14px;
}
.project-description :deep(:is(h1, h2, h3, h4, h5, h6)) {
  margin: 28px 0 10px;
  font-weight: 600;
  line-height: 1.4;
  text-wrap: balance;
  scroll-margin-top: 16px;
}
.project-description :deep(h1) {
  font-size: 23px;
}
.project-description :deep(h2) {
  font-size: 19px;
}
.project-description :deep(h3) {
  font-size: 16px;
}
.project-description :deep(:is(h4, h5, h6)) {
  font-size: 14px;
}
.project-description :deep(> :first-child) {
  margin-top: 0;
}
.project-description :deep([data-project-anchor]:focus) {
  outline: none;
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.project-description :deep(:is(ul, ol)) {
  margin: 10px 0 16px;
  padding-left: 24px;
}
.project-description :deep(li) {
  padding-left: 3px;
  margin: 4px 0;
}
.project-description :deep(li > :is(ul, ol)) {
  margin-block: 4px;
}
.project-description :deep(li > p) {
  margin-block: 6px;
}
.project-description :deep(li::marker) {
  color: var(--muted);
}
.project-description :deep(.description-task) {
  list-style: none;
}
.project-description :deep(input[type="checkbox"]) {
  width: 13px;
  height: 13px;
  margin: 0 7px 0 -20px;
  vertical-align: -1px;
  accent-color: var(--blue);
}
.project-description :deep(img) {
  display: inline-block;
  max-width: 100%;
  max-height: 420px;
  height: auto;
  object-fit: contain;
  vertical-align: middle;
}
.project-description :deep(img[data-project-image]:not([src])) {
  min-width: 24px;
  min-height: 24px;
}
.project-description :deep(.description-image) {
  display: inline-block;
  max-width: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  vertical-align: middle;
  cursor: zoom-in;
}
.project-description :deep(.description-image:disabled) {
  cursor: default;
}
.project-description :deep(a[href]) {
  color: var(--blue);
  cursor: pointer;
  text-decoration: underline;
  text-decoration-color: color-mix(in srgb, var(--blue) 40%, transparent);
  text-underline-offset: 3px;
  transition:
    text-decoration-color 150ms,
    text-shadow 150ms;
}
.project-description :deep(a[href]:hover) {
  text-decoration-color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.project-description :deep(:is(a[href], button, summary):focus-visible) {
  outline: 1px solid var(--blue);
  outline-offset: 3px;
}
.project-description :deep(.description-code) {
  margin: 14px 0;
  background: color-mix(in srgb, var(--ink) 4%, transparent);
  border: 1px solid var(--line);
  border-radius: 2px;
  overflow: hidden;
}
.project-description :deep(.description-code-tools) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 5px 12px 0;
  font-size: 10px;
  color: var(--muted);
}
.project-description :deep(.description-code-tools button) {
  padding: 3px 0 3px 8px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font: inherit;
  cursor: pointer;
}
.project-description :deep(.description-code-tools button:hover) {
  color: var(--blue);
}
.project-description :deep(pre) {
  max-width: 100%;
  margin: 0;
  overflow-x: auto;
  overscroll-behavior-x: contain;
  padding: 8px 12px 12px;
  white-space: pre;
}
.project-description :deep(code) {
  font: 12px/1.75 var(--mono);
}
.project-description :deep(:not(pre) > code) {
  padding: 2px 5px;
  background: color-mix(in srgb, var(--ink) 7%, transparent);
  color: var(--blue);
  border-radius: 2px;
}
.project-description :deep(blockquote) {
  margin: 16px 0;
  padding: 4px 0 4px 14px;
  border-left: 2px solid color-mix(in srgb, var(--blue) 60%, transparent);
  color: var(--muted);
}
.project-description :deep(blockquote > :last-child) {
  margin-bottom: 0;
}
.project-description :deep(table) {
  display: block;
  max-width: 100%;
  margin: 16px 0;
  overflow-x: auto;
  overscroll-behavior-x: contain;
  border-collapse: collapse;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.project-description :deep(:is(td, th)) {
  padding: 8px 12px;
  border: 1px solid var(--line);
  text-align: left;
}
.project-description :deep(th) {
  font-weight: 500;
  background: color-mix(in srgb, var(--blue) 8%, transparent);
}
.project-description :deep(tbody tr:nth-child(even)) {
  background: color-mix(in srgb, var(--ink) 3%, transparent);
}
.project-description :deep(hr) {
  border: 0;
  border-top: 1px solid var(--line);
  margin: 24px 0;
}
.project-description :deep(details) {
  margin: 14px 0;
  padding: 0 12px;
  border-left: 1px solid var(--line);
}
.project-description :deep(summary) {
  padding: 6px 0;
  cursor: pointer;
  color: var(--blue);
}
.project-description :deep(details[open] > summary) {
  margin-bottom: 6px;
}
.project-description :deep([data-align="center"]) {
  text-align: center;
}
.project-description :deep([data-align="right"]) {
  text-align: right;
}
.project-description :deep([data-align="left"]) {
  text-align: left;
}
.project-image-preview {
  width: min(1040px, calc(100vw - 64px));
  max-height: calc(100dvh - 64px);
}
.project-image-preview .modal-header {
  flex-shrink: 0;
}
.project-image-preview h2 {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.project-image-preview > img {
  display: block;
  min-height: 0;
  width: 100%;
  max-height: calc(100dvh - 150px);
  object-fit: contain;
  padding: 0 20px 20px;
}
@media (prefers-reduced-motion: reduce) {
  .project-description :deep(a[href]) {
    transition: none;
  }
}
</style>
