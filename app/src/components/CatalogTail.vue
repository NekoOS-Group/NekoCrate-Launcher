<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { LoaderCircle, RefreshCw } from "lucide-vue-next";
import { t, translateMessage } from "../i18n";
const props = defineProps<{
  loading: boolean;
  more: boolean;
  failure: string;
  disabled?: boolean;
  position: number;
}>();
const emit = defineEmits<{
  load: [];
  retry: [];
  "update:position": [position: number];
}>();
const tail = ref<HTMLElement>();
let viewport: HTMLElement | null = null;
let observer: IntersectionObserver | undefined;
function check() {
  if (
    !props.more ||
    props.loading ||
    props.failure ||
    props.disabled ||
    !viewport ||
    !tail.value ||
    tail.value.closest("[inert]")
  )
    return;
  const view = viewport.getBoundingClientRect(),
    end = tail.value.getBoundingClientRect();
  // Check fresh geometry after append; an old observer entry must not fetch every page.
  if (
    view.height > 0 &&
    end.height > 0 &&
    end.top <= view.bottom + 180 &&
    end.bottom >= view.top
  )
    emit("load");
}
function scroll() {
  if (!viewport || tail.value?.closest("[inert]")) return;
  emit("update:position", viewport.scrollTop);
  if (!observer) check();
}
onMounted(() => {
  viewport =
    tail.value?.closest<HTMLElement>("[data-catalog-viewport]") || null;
  if (!viewport || !tail.value) return;
  viewport.scrollTop = props.position;
  viewport.addEventListener("scroll", scroll, { passive: true });
  if (typeof IntersectionObserver !== "undefined") {
    observer = new IntersectionObserver(check, {
      root: viewport,
      rootMargin: "180px",
    });
    observer.observe(tail.value);
  }
  void nextTick(check);
});
watch(
  () => [props.loading, props.more, props.failure, props.disabled],
  () => void nextTick(check),
  { flush: "post" },
);
onBeforeUnmount(() => {
  observer?.disconnect();
  viewport?.removeEventListener("scroll", scroll);
});
</script>
<template>
  <div
    ref="tail"
    class="catalog-tail"
    :class="{ visible: loading || more || failure }"
  >
    <button
      v-if="failure"
      class="text-button"
      :title="translateMessage(failure)"
      :disabled="disabled"
      @click="emit('retry')"
    >
      <RefreshCw :size="13" />{{ t("重试") }}
    </button>
    <LoaderCircle
      v-else-if="loading"
      class="spin"
      :size="16"
      :aria-label="t('加载更多')"
    />
  </div>
</template>
<style scoped>
.catalog-tail {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--muted);
  height: 0;
  flex-shrink: 0;
}
.catalog-tail.visible {
  height: 40px;
}
.catalog-tail .text-button {
  font-size: 11px;
}
</style>
