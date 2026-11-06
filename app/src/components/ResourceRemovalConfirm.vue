<script setup lang="ts">
import { Trash2 } from "lucide-vue-next";
import GlassDialog from "./GlassDialog.vue";
import { t } from "../i18n";
defineProps<{ names: string[] }>();
const emit = defineEmits<{ close: []; confirm: [] }>();
</script>
<template>
  <GlassDialog :label="t('删除整合包 Mod')" @close="emit('close')">
    <section class="glass-surface pack-mod-removal">
      <h2 class="dialog-heading" tabindex="-1">{{ t("删除整合包 Mod") }}</h2>
      <p>{{ t("这些 Mod 由整合包清单声明，删除后可能导致游戏无法启动。") }}</p>
      <ul>
        <li v-for="(name, index) in names" :key="index">{{ name }}</li>
      </ul>
      <footer class="modal-actions">
        <button class="button" @click="emit('close')">{{ t("取消") }}</button>
        <button class="button danger" @click="emit('confirm')">
          <Trash2 :size="15" />{{ t("移入回收站") }}
        </button>
      </footer>
    </section>
  </GlassDialog>
</template>
<style scoped>
.pack-mod-removal {
  width: min(440px, 90vw);
  padding: 24px;
}
p {
  margin-top: 14px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--muted);
}
ul {
  max-height: 180px;
  overflow: auto;
  padding-left: 18px;
  margin: 14px 0;
  font-size: 12px;
  line-height: 1.8;
  overflow-wrap: anywhere;
}
</style>
