<script setup lang="ts">
import { onMounted, ref } from "vue";
import { t } from "../i18n";
const props = defineProps<{
  load: () => Promise<boolean>;
  save: (key: string) => Promise<void>;
}>();
const emit = defineEmits<{ error: [error: unknown] }>();
const configured = ref(false),
  pending = ref(false),
  draft = ref("");
onMounted(async () => {
  try {
    configured.value = await props.load();
  } catch (e) {
    emit("error", e);
  }
});
async function change() {
  const key = draft.value;
  if (!key.trim() || pending.value) return;
  pending.value = true;
  try {
    await props.save(key);
    configured.value = true;
    draft.value = "";
  } catch (e) {
    emit("error", e);
  } finally {
    pending.value = false;
  }
}
async function clear() {
  pending.value = true;
  try {
    await props.save("");
    configured.value = false;
    draft.value = "";
  } catch (e) {
    emit("error", e);
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <div class="curseforge-key-row">
    <label for="curseforge-api-key">CurseForge API Key</label
    ><input
      id="curseforge-api-key"
      v-model="draft"
      type="password"
      autocomplete="new-password"
      spellcheck="false"
      :disabled="pending"
      :placeholder="t(configured ? '已配置，输入以替换' : '未配置')"
      @change="change"
    /><button
      v-if="configured"
      type="button"
      class="text-button"
      :disabled="pending"
      @click="clear"
    >
      {{ t("清除") }}
    </button>
  </div>
</template>
<style scoped>
.curseforge-key-row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
}
.curseforge-key-row input {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  height: 30px;
}
.curseforge-key-row label {
  flex-shrink: 0;
  color: var(--muted);
}
</style>
