<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import {
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
  RefreshCw,
} from "lucide-vue-next";
import {
  projectImageUrl,
  projectMedia,
  type ResourceRpc,
} from "../projectPresentation";
import { t } from "../i18n";

const props = defineProps<{
  images: readonly { url: string; title: string }[];
  rpc: ResourceRpc;
}>();
const slides = computed(() => {
  const seen = new Set<string>();
  return props.images.filter((item) => {
    if (!projectImageUrl(item.url) || seen.has(item.url)) return false;
    seen.add(item.url);
    return true;
  });
});
const root = ref<HTMLElement>();
const shown = shallowRef<{
  index: number;
  url: string;
  title: string;
  src: string;
}>();
const requested = ref(0);
const loading = ref(false);
const failed = ref(false);
const direction = ref(1);
const reducedMotion = ref(false);
const hovered = ref(false);
const focusPaused = ref(false);
const visible = ref(false);
const pageVisible = ref(!document.hidden);
const active = computed(() => shown.value?.index ?? requested.value);
const playing = computed(
  () =>
    slides.value.length > 1 &&
    !!shown.value &&
    visible.value &&
    pageVisible.value &&
    !reducedMotion.value &&
    !hovered.value &&
    !focusPaused.value &&
    !loading.value &&
    !failed.value,
);
let request = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let observer: IntersectionObserver | undefined;
let motion: MediaQueryList | undefined;
function stopTimer() {
  clearTimeout(timer);
  timer = undefined;
}
async function show(index: number, movement = index >= active.value ? 1 : -1) {
  if (!slides.value.length) return;
  stopTimer();
  index = (index + slides.value.length) % slides.value.length;
  const current = ++request;
  const item = slides.value[index];
  requested.value = index;
  loading.value = true;
  failed.value = false;
  try {
    const src = await projectMedia(props.rpc).image(item.url);
    // Decode before swapping so slow downloads cannot clear the current slide.
    const image = new Image();
    image.src = src;
    await image.decode();
    if (current !== request) return;
    direction.value = movement;
    shown.value = { ...item, src, index };
    if (visible.value && pageVisible.value && slides.value.length > 1) {
      const next = slides.value[(index + 1) % slides.value.length];
      void projectMedia(props.rpc)
        .image(next.url)
        .catch(() => {});
    }
  } catch {
    if (current === request) failed.value = true;
  } finally {
    if (current === request) loading.value = false;
  }
}
function step(movement: number) {
  void show(requested.value + movement, movement);
}
function leaveFocus(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !root.value?.contains(event.relatedTarget)
  )
    focusPaused.value = false;
}
function visibilityChanged() {
  pageVisible.value = !document.hidden;
}
function motionChanged() {
  reducedMotion.value = !!motion?.matches;
}
watch([playing, active], () => {
  stopTimer();
  if (playing.value) timer = setTimeout(() => step(1), 5000);
});
watch(
  [slides, () => props.rpc],
  () => {
    request++;
    stopTimer();
    shown.value = undefined;
    requested.value = 0;
    loading.value = false;
    failed.value = false;
    if (visible.value) void show(0);
  },
  { immediate: true },
);
watch(visible, (value) => {
  if (value && !shown.value && !loading.value && !failed.value) void show(0);
});
onMounted(() => {
  motion = matchMedia("(prefers-reduced-motion: reduce)");
  motionChanged();
  motion.addEventListener("change", motionChanged);
  document.addEventListener("visibilitychange", visibilityChanged);
  if (typeof IntersectionObserver === "undefined") visible.value = true;
  else {
    observer = new IntersectionObserver(
      ([entry]) => {
        visible.value = entry.isIntersecting && entry.intersectionRatio >= 0.1;
      },
      { threshold: [0, 0.1] },
    );
    if (root.value) observer.observe(root.value);
  }
});
onBeforeUnmount(() => {
  request++;
  stopTimer();
  observer?.disconnect();
  motion?.removeEventListener("change", motionChanged);
  document.removeEventListener("visibilitychange", visibilityChanged);
});
</script>

<template>
  <figure
    v-if="slides.length"
    ref="root"
    class="project-gallery"
    :style="{ '--gallery-direction': direction }"
    role="region"
    :aria-label="t('项目图组')"
    :aria-roledescription="t('轮播图')"
    tabindex="0"
    @mouseenter="hovered = true"
    @mouseleave="hovered = false"
    @focusin="focusPaused = true"
    @focusout="leaveFocus"
    @keydown.left.prevent="step(-1)"
    @keydown.right.prevent="step(1)"
  >
    <div class="gallery-stage" :aria-busy="loading">
      <Transition name="gallery-slide">
        <img
          v-if="shown"
          :key="shown.url"
          class="gallery-image"
          :src="shown.src"
          :alt="shown.title || t('第 {index} 张图片', { index: active + 1 })"
          draggable="false"
        />
      </Transition>
      <div v-if="!shown" class="gallery-placeholder">
        <button
          v-if="failed"
          type="button"
          class="text-button"
          @click="show(requested)"
        >
          <RefreshCw :size="16" />{{ t("重新加载图片") }}
        </button>
        <LoaderCircle
          v-else
          :size="20"
          class="spin"
          :aria-label="t('正在加载图片')"
        />
      </div>
      <template v-if="slides.length > 1">
        <button
          type="button"
          class="gallery-arrow gallery-previous"
          :aria-label="t('上一张图片')"
          @click="step(-1)"
        >
          <ChevronLeft :size="22" />
        </button>
        <button
          type="button"
          class="gallery-arrow gallery-next"
          :aria-label="t('下一张图片')"
          @click="step(1)"
        >
          <ChevronRight :size="22" />
        </button>
      </template>
    </div>
    <figcaption
      v-if="slides.length > 1 || shown?.title"
      class="gallery-caption"
    >
      <span class="gallery-title" :title="shown?.title">{{
        shown?.title
      }}</span>
      <div
        v-if="slides.length > 1"
        class="gallery-pagination"
        :aria-label="t('选择图片')"
      >
        <button
          v-for="(item, index) in slides"
          :key="item.url"
          type="button"
          :class="{ active: active === index }"
          :aria-current="active === index ? 'true' : undefined"
          :aria-label="t('第 {index} 张图片', { index: index + 1 })"
          :title="item.title || undefined"
          @click="show(index)"
        >
          <span />
        </button>
      </div>
      <div class="gallery-actions">
        <button
          v-if="failed && shown"
          type="button"
          class="icon-button"
          :title="t('重新加载图片')"
          :aria-label="t('重新加载图片')"
          @click="show(requested)"
        >
          <RefreshCw :size="13" />
        </button>
        <LoaderCircle
          v-else-if="loading && shown"
          :size="12"
          class="spin"
          :aria-label="t('正在加载图片')"
        />
        <span
          v-if="slides.length > 1"
          class="gallery-count"
          :aria-live="playing ? 'off' : 'polite'"
          >{{ active + 1 }} / {{ slides.length }}</span
        >
      </div>
    </figcaption>
  </figure>
</template>

<style scoped>
.project-gallery {
  margin: 16px 0 20px;
  min-width: 0;
  outline: none;
}
.gallery-stage {
  position: relative;
  width: 100%;
  height: clamp(220px, 48vh, 420px);
  overflow: hidden;
  background: color-mix(in srgb, var(--night) 58%, transparent);
}
.gallery-image {
  position: absolute;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.gallery-placeholder {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: var(--muted);
}
.gallery-placeholder button {
  font-size: 12px;
}
.gallery-arrow {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  display: grid;
  place-items: center;
  width: 36px;
  height: 40px;
  padding: 0;
  border: 0;
  background: var(--control-glass-fill);
  color: var(--ink);
  cursor: pointer;
  transition:
    color 160ms,
    background-color 160ms;
}
.gallery-previous {
  left: 12px;
}
.gallery-next {
  right: 12px;
}
.gallery-arrow:hover {
  background: var(--control-glass-hover);
  color: var(--blue);
}
.gallery-caption {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 10px;
  min-height: 32px;
  font-size: 11px;
  color: var(--muted);
}
.gallery-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.gallery-pagination {
  display: flex;
  align-items: center;
}
.gallery-pagination button {
  display: grid;
  place-items: center;
  width: 24px;
  height: 30px;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
}
.gallery-pagination span {
  width: 12px;
  height: 2px;
  background: color-mix(in srgb, var(--blue) 28%, transparent);
  transition:
    background-color 180ms,
    box-shadow 180ms;
}
.gallery-pagination .active span,
.gallery-pagination button:hover span {
  background: var(--blue);
  box-shadow: 0 0 6px var(--glow);
}
.gallery-actions {
  grid-column: 3;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
.gallery-count {
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.gallery-actions .icon-button {
  width: 28px;
  height: 28px;
}
.project-gallery button:focus-visible {
  outline: 1px solid var(--blue);
  outline-offset: 2px;
}
.project-gallery:focus-visible .gallery-arrow {
  color: var(--blue);
  box-shadow: 0 0 10px var(--glow);
}
.gallery-slide-enter-active,
.gallery-slide-leave-active {
  transition:
    opacity 320ms var(--motion-ease),
    transform 400ms var(--motion-ease);
}
.gallery-slide-enter-from {
  opacity: 0;
  transform: translateX(calc(var(--gallery-direction) * 20px));
}
.gallery-slide-leave-to {
  opacity: 0;
  transform: translateX(calc(var(--gallery-direction) * -20px));
}
@media (prefers-reduced-motion: reduce) {
  .gallery-slide-enter-active,
  .gallery-slide-leave-active,
  .gallery-pagination span,
  .gallery-arrow {
    transition: none;
  }
}
</style>
