<script setup>
import { ref } from "vue";
import {
  Settings2,
  ScrollText,
  Boxes,
  Plus,
  ChevronUp,
  Play,
  LayoutGrid,
  Wifi,
  Globe,
  ListTodo,
} from "lucide-vue-next";
import logo from "../icon.webp";
import grass from "../../../app/src/assets/blocks/grass_block_side.png";
import ice from "../../../app/src/assets/blocks/ice.png";
import crafting from "../../../app/src/assets/blocks/crafting_table_front.png";

defineProps({ copy: { type: Object, required: true } });
const blocks = [grass, ice, crafting];
const selected = ref(0);
// Each layout moves the instance bar and widgets, like the launcher's editor.
const layouts = ["row", "column", "widgets"];
const layout = ref(0);
const nextLayout = () => (layout.value = (layout.value + 1) % layouts.length);
</script>

<template>
  <figure class="launcher-preview crate" :data-layout="layouts[layout]">
    <div class="preview-top">
      <div class="preview-account">
        <span class="preview-avatar"><img :src="logo" alt="" /></span
        ><span
          >{{ copy.guest }}<small>{{ copy.guestSub }}</small></span
        >
      </div>
      <div class="preview-tools" aria-hidden="true">
        <Boxes :size="16" /><ScrollText :size="16" /><Settings2 :size="16" />
      </div>
      <button class="preview-relayout" @click="nextLayout">
        <LayoutGrid :size="15" />{{ copy.relayout }}
      </button>
    </div>

    <div class="preview-stage">
      <div class="preview-center">
        <Transition name="swap" mode="out-in">
          <img
            :key="selected"
            :src="selected === 0 ? logo : blocks[selected]"
            :class="{ block: selected !== 0 }"
            alt=""
            width="200"
            height="200"
          />
        </Transition>
      </div>
      <TransitionGroup tag="div" name="widget" class="preview-widgets">
        <template v-if="layouts[layout] === 'widgets'">
          <div key="servers" class="preview-widget">
            <h4><Wifi :size="13" />{{ copy.servers }}</h4>
            <p>
              mc.neko.example<span>12 {{ copy.online }} · 28 ms</span>
            </p>
          </div>
          <div key="worlds" class="preview-widget">
            <h4><Globe :size="13" />{{ copy.worlds }}</h4>
            <p v-for="name in copy.worldNames" :key="name">{{ name }}</p>
          </div>
          <div key="todo" class="preview-widget">
            <h4><ListTodo :size="13" />{{ copy.todo }}</h4>
            <p v-for="item in copy.todoItems" :key="item">{{ item }}</p>
          </div>
        </template>
      </TransitionGroup>
    </div>

    <div class="preview-bottom">
      <div
        class="preview-instances"
        role="group"
        :aria-label="copy.instances"
      >
        <button
          v-for="(block, index) in blocks"
          :key="block"
          :aria-label="copy.versions[index]"
          :aria-pressed="selected === index"
          @click="selected = index"
        >
          <img :src="block" alt="" /></button
        ><Plus :size="15" aria-hidden="true" />
      </div>
      <div class="preview-launch">
        <span
          ><Play :size="15" />{{ copy.launch }}<ChevronUp :size="13"
        /></span>
        <Transition name="text-change" mode="out-in"
          ><small :key="selected">{{ copy.versions[selected] }}</small></Transition
        >
      </div>
    </div>
  </figure>
</template>
