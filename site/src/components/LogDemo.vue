<script setup>
import { ref, computed } from "vue";
import { Search, TriangleAlert, Wrench, Check } from "lucide-vue-next";

const props = defineProps({ copy: { type: Object, required: true } });
const lines = [
  ["info", "12:04:11", "Loading Minecraft 1.21.1 with Fabric Loader 0.16.5"],
  ["info", "12:04:11", "Setting user: Steve, token ••••••••"],
  ["info", "12:04:12", "Loading 47 mods"],
  ["warn", "12:04:12", "Mod sodium-extra uses an outdated mixin config"],
  ["info", "12:04:13", "Initializing OpenGL 4.1 Metal"],
  ["error", "12:04:13", "Mod resolution failed: sodium-extra requires fabric-api >=0.100.0"],
  ["error", "12:04:13", "net.fabricmc.loader.impl.FormattedException: Incompatible mods"],
  ["warn", "12:04:14", "Shutting down; saving nothing"],
];
const level = ref("all");
const query = ref("");
const fixed = ref(false);
const shown = computed(() => {
  const term = query.value.trim().toLowerCase();
  return lines.filter(
    ([lvl, , text]) =>
      (level.value === "all" || lvl === level.value) &&
      (!term || text.toLowerCase().includes(term)),
  );
});
const levels = ["all", "warn", "error"];
const count = (lvl) =>
  lvl === "all" ? lines.length : lines.filter(([l]) => l === lvl).length;
function mark(text) {
  const term = query.value.trim();
  if (!term) return [{ text }];
  const i = text.toLowerCase().indexOf(term.toLowerCase());
  if (i < 0) return [{ text }];
  return [
    { text: text.slice(0, i) },
    { text: text.slice(i, i + term.length), hit: true },
    { text: text.slice(i + term.length) },
  ];
}
</script>

<template>
  <div class="log-demo crate">
    <div class="log-diagnosis">
      <TriangleAlert :size="18" />
      <div>
        <strong>{{ copy.diagnosis }}</strong>
        <p>{{ copy.diagnosisText }}</p>
      </div>
      <button :disabled="fixed" @click="fixed = true">
        <Check v-if="fixed" :size="15" /><Wrench v-else :size="15" />
        {{ fixed ? copy.fixed : copy.fix }}
      </button>
    </div>
    <div class="log-toolbar">
      <div class="log-levels" role="group">
        <button
          v-for="item in levels"
          :key="item"
          :aria-pressed="level === item"
          :data-level="item"
          @click="level = item"
        >
          {{ copy.levels[item] }}<span>{{ count(item) }}</span>
        </button>
      </div>
      <label class="log-search"
        ><Search :size="14" /><input
          v-model="query"
          type="search"
          :placeholder="copy.search"
          :aria-label="copy.search"
      /></label>
    </div>
    <ol class="log-lines">
      <li v-for="line in shown" :key="line[2]" :data-level="line[0]">
        <time>{{ line[1] }}</time>
        <span
          ><template v-for="(part, i) in mark(line[2])" :key="i"
            ><mark v-if="part.hit">{{ part.text }}</mark
            ><template v-else>{{ part.text }}</template></template
          ></span
        >
      </li>
    </ol>
    <p v-if="!shown.length" class="log-empty">{{ copy.noMatch }}</p>
    <p class="log-exit">{{ copy.exit }}</p>
  </div>
</template>
