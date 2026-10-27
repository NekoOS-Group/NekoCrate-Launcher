<script setup lang="ts">
import { ExternalLink } from "lucide-vue-next";
import { t } from "../i18n";
import launcherIcon from "../assets/neko-cat.png";
import { listRevealStyle } from "../listMotion";

defineProps<{ version: string }>();
const emit = defineEmits<{ openLink: [url: string] }>();
</script>

<template>
  <div
    id="settings-panel-about"
    class="settings-page settings-about"
    role="tabpanel"
    aria-labelledby="settings-tab-about"
  >
    <img
      class="about-icon list-reveal-item"
      :style="listRevealStyle(0)"
      :src="launcherIcon"
      alt=""
      draggable="false"
    />
    <p class="about-identity list-reveal-item" :style="listRevealStyle(1)">
      <span>{{ t("猫咪盒子启动器") }}</span>
      <span class="about-version">v{{ version }}</span>
    </p>
    <p class="about-copyright list-reveal-item" :style="listRevealStyle(2)">
      © NekoOS Group · GPL-3.0
    </p>
    <div class="about-links list-reveal-item" :style="listRevealStyle(3)">
      <button
        type="button"
        class="about-link"
        @click="
          emit(
            'openLink',
            'https://github.com/NekoOS-Group/NekoCrate-Launcher/issues',
          )
        "
      >
        <ExternalLink :size="14" /><span>{{ t("反馈问题") }}</span>
      </button>
      <button
        type="button"
        class="about-link"
        @click="
          emit('openLink', 'https://github.com/NekoOS-Group/NekoCrate-Launcher')
        "
      >
        <ExternalLink :size="14" /><span>{{ t("源代码") }}</span>
      </button>
    </div>
  </div>
</template>
