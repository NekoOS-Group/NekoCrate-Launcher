<script setup>
import { ref } from "vue";
import {
  Settings2,
  ScrollText,
  Boxes,
  Plus,
  ChevronUp,
  Square,
  Play,
} from "lucide-vue-next";
import logo from "../icon.webp";
import grass from "../../../app/src/assets/blocks/grass_block_side.png";
import ice from "../../../app/src/assets/blocks/ice.png";
import crafting from "../../../app/src/assets/blocks/crafting_table_front.png";
const selected = ref(0);
const versions = ["1.21.1 · Fabric", "1.20.1 · Forge", "1.21.1 · 原版"];
const blocks = [grass, ice, crafting];
</script>

<template>
  <div class="launcher-preview glass">
    <div class="preview-top">
      <div class="preview-account">
        <span class="preview-avatar"><img :src="logo" alt="" /></span
        ><span>游客<small>开始你的游戏</small></span>
      </div>
      <div class="preview-tools" aria-hidden="true">
        <Boxes :size="16" /><ScrollText :size="16" /><Settings2 :size="16" />
      </div>
      <span class="preview-caption">交互示意</span>
    </div>
    <div class="preview-center">
      <img :src="logo" alt="猫咪盒子像素图标" width="200" height="200" />
    </div>
    <div class="preview-bottom">
      <div class="preview-instances" role="group" aria-label="预览实例切换">
        <button
          v-for="(block, index) in blocks"
          :key="block"
          :aria-label="`预览 ${versions[index]}`"
          :aria-pressed="selected === index"
          @click="selected = index"
        >
          <img :src="block" alt="" /></button
        ><Plus :size="15" aria-hidden="true" />
      </div>
      <div class="preview-launch">
        <span><Play :size="15" />启动游戏<ChevronUp :size="13" /></span
        ><Transition name="text-change" mode="out-in"
          ><small :key="selected">{{ versions[selected] }}</small></Transition
        >
      </div>
    </div>
  </div>
</template>
