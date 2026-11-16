<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { Pause, Play, RotateCcw, Check } from "lucide-vue-next";

defineProps({ copy: { type: Object, required: true } });
const phases = [
  { id: "reuse", total: 214 },
  { id: "download", total: 168 },
  { id: "verify", total: 382 },
  { id: "install", total: 46 },
];
// Rough per-second rate per phase so the demo reads like a real install.
const rates = { reuse: 260, download: 70, verify: 340, install: 30 };
const done = ref(phases.map(() => 0));
const paused = ref(true);
const root = ref();
let frame = 0;
let last = 0;
let observer;
let started = false;

const active = computed(() => done.value.findIndex((n, i) => n < phases[i].total));
const finished = computed(() => active.value === -1);
const overall = computed(
  () =>
    done.value.reduce((a, b) => a + b, 0) /
    phases.reduce((a, p) => a + p.total, 0),
);

function step(now) {
  frame = requestAnimationFrame(step);
  const dt = Math.min((now - (last || now)) / 1000, 0.1);
  last = now;
  if (paused.value || finished.value) return;
  const i = active.value;
  const next = [...done.value];
  next[i] = Math.min(phases[i].total, next[i] + rates[phases[i].id] * dt);
  done.value = next;
}
function toggle() {
  if (finished.value) {
    done.value = phases.map(() => 0);
    paused.value = false;
    return;
  }
  paused.value = !paused.value;
}
onMounted(() => {
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce) done.value = phases.map((p, i) => (i < 2 ? p.total : i === 2 ? 190 : 0));
  frame = requestAnimationFrame(step);
  // Start the first time the demo scrolls into view.
  observer = new IntersectionObserver(([entry]) => {
    if (entry.isIntersecting && !started && !reduce) {
      started = true;
      paused.value = false;
    }
  }, { threshold: 0.5 });
  observer.observe(root.value);
});
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
  observer?.disconnect();
});
</script>

<template>
  <div ref="root" class="task-demo crate">
    <header class="task-head">
      <span class="task-name">{{ copy.name }}</span>
      <button class="task-toggle" @click="toggle">
        <RotateCcw v-if="finished" :size="15" /><Play
          v-else-if="paused"
          :size="15"
        /><Pause v-else :size="15" />
        {{ finished ? copy.restart : paused ? copy.resume : copy.pause }}
      </button>
    </header>
    <div class="task-total" aria-hidden="true">
      <span :style="{ scale: `${overall} 1` }" />
    </div>
    <ol class="task-phases">
      <li
        v-for="(phase, index) in phases"
        :key="phase.id"
        :class="{
          active: index === active,
          complete: done[index] >= phase.total,
        }"
      >
        <span class="phase-mark"
          ><Check v-if="done[index] >= phase.total" :size="13"
        /></span>
        <span class="phase-name">{{ copy.phases[phase.id] }}</span>
        <span class="phase-bar"
          ><span :style="{ scale: `${done[index] / phase.total} 1` }"
        /></span>
        <span class="phase-count">{{
          copy.files(Math.floor(done[index]), phase.total)
        }}</span>
      </li>
    </ol>
    <p class="task-status" role="status">
      {{ finished ? copy.finished : paused ? copy.paused : copy.running }}
    </p>
  </div>
</template>
