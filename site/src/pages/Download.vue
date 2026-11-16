<script setup>
import { computed, ref, watch, onMounted, onBeforeUnmount } from "vue";
import {
  ArrowDown,
  ArrowRight,
  ArrowUpRight,
  ChevronDown,
  Monitor,
  Command,
  Terminal,
  Check,
} from "lucide-vue-next";
import logo from "../icon.webp";
import { useReleases } from "../useRelease";
import {
  releaseAsset,
  detectPlatform,
  formatSize,
  repositoryUrl,
} from "../releases.mjs";
import { href } from "../router";
import { lang, t } from "../i18n";

const releases = useReleases();
const detected = detectPlatform(navigator.userAgent);
const platforms = [
  { id: "windows", label: "Windows", arch: "x64", icon: Monitor, ext: { installed: ".exe", portable: ".exe" } },
  { id: "macos", label: "macOS", arch: "Apple Silicon", icon: Command, ext: { installed: ".dmg", portable: ".app.tar.gz" } },
  { id: "linux", label: "Linux", arch: "x64", icon: Terminal, ext: { installed: ".deb", portable: ".elf" } },
];
const modes = ["installed", "portable"];

const platform = ref(detected || "windows");
const mode = ref("installed");
const tag = ref(""); // empty = always the newest release
const open = ref(false);
const picker = ref();
const toggle = ref();

const release = computed(
  () => releases.value.find((item) => item.tag === tag.value) || releases.value[0],
);
const current = computed(() => platforms.find((item) => item.id === platform.value));
const asset = computed(() => releaseAsset(release.value, platform.value, mode.value));
const formatDate = (value) =>
  new Date(value).toLocaleDateString(lang.value === "zh" ? "zh-CN" : "en", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
const available = (rel) => Boolean(releaseAsset(rel, platform.value, mode.value));
const summary = computed(() =>
  [
    t.value.dl[mode.value],
    current.value.ext[mode.value],
    release.value.tag,
    asset.value && formatSize(asset.value.size),
  ]
    .filter(Boolean)
    .join(" · "),
);

function close(focusToggle = false) {
  open.value = false;
  if (focusToggle) toggle.value?.focus();
}
function outside(event) {
  if (open.value && !picker.value?.contains(event.target)) close();
}
function escape(event) {
  if (open.value && event.key === "Escape") close(true);
}
onMounted(() => {
  document.addEventListener("pointerdown", outside);
  document.addEventListener("keydown", escape);
});
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", outside);
  document.removeEventListener("keydown", escape);
});
watch(releases, () => {
  if (tag.value && !releases.value.some((item) => item.tag === tag.value))
    tag.value = "";
});
</script>

<template>
  <section class="hero" aria-labelledby="hero-title">
    <div class="hero-copy">
      <h1 id="hero-title">
        {{ t.hero.title[0] }}<br />{{ t.hero.title[1] }}
      </h1>
      <p class="hero-lede">{{ t.hero.lede }}</p>

      <div ref="picker" class="download-picker">
        <div class="download-split">
          <a
            v-if="asset"
            class="download-main"
            :href="asset.url"
            :aria-label="`${t.dl.download(current.label)}, ${summary}`"
          >
            <ArrowDown :size="22" />
            <span class="download-text">
              <strong>{{ t.dl.download(current.label) }}</strong>
              <small>{{ summary }}</small>
            </span>
          </a>
          <a
            v-else
            class="download-main"
            :href="release.url"
            target="_blank"
            rel="noopener noreferrer"
          >
            <ArrowUpRight :size="22" />
            <span class="download-text">
              <strong>{{ t.dl.viewRelease }}</strong>
              <small>{{ t.dl.missing }}</small>
            </span>
          </a>
          <button
            ref="toggle"
            class="download-toggle"
            :aria-expanded="open"
            aria-controls="download-options"
            :aria-label="t.dl.options"
            :title="t.dl.options"
            @click="open = !open"
          >
            <ChevronDown :size="22" />
          </button>
        </div>

        <Transition name="drop">
          <div
            v-show="open"
            id="download-options"
            class="download-options crate"
          >
            <fieldset>
              <legend>{{ t.dl.system }}</legend>
              <div class="option-row">
                <button
                  v-for="item in platforms"
                  :key="item.id"
                  :aria-pressed="platform === item.id"
                  @click="platform = item.id"
                >
                  <component :is="item.icon" :size="18" />
                  <span
                    >{{ item.label }}<small>{{ item.arch }}</small></span
                  >
                </button>
              </div>
            </fieldset>
            <fieldset>
              <legend>
                {{ t.dl.mode }}
                <a :href="href('docs', 'portable')" @click="close()">{{
                  t.dl.choose
                }}</a>
              </legend>
              <div class="option-row">
                <button
                  v-for="item in modes"
                  :key="item"
                  :aria-pressed="mode === item"
                  @click="mode = item"
                >
                  <span
                    >{{ t.dl[item] }}
                    <code>{{ current.ext[item] }}</code
                    ><small>{{ t.dl.notes[platform][item] }}</small></span
                  >
                </button>
              </div>
            </fieldset>
            <fieldset>
              <legend>{{ t.dl.version }}</legend>
              <ul class="version-list">
                <li v-for="(item, index) in releases" :key="item.tag">
                  <button
                    :aria-pressed="release === item"
                    @click="tag = index === 0 ? '' : item.tag"
                  >
                    <span class="version-check"
                      ><Check v-if="release === item" :size="15"
                    /></span>
                    <code>{{ item.tag }}</code>
                    <span v-if="index === 0" class="version-badge">{{
                      t.dl.latest
                    }}</span>
                    <span class="version-kind">{{
                      item.prerelease ? t.dl.preview : t.dl.stable
                    }}</span>
                    <span v-if="!available(item)" class="version-missing">{{
                      t.dl.missing
                    }}</span>
                    <time :datetime="item.publishedAt">{{
                      formatDate(item.publishedAt)
                    }}</time>
                  </button>
                </li>
              </ul>
            </fieldset>
            <footer class="options-foot">
              <a :href="release.url" target="_blank" rel="noopener noreferrer"
                >{{ t.dl.viewRelease }} {{ release.tag
                }}<ArrowUpRight :size="13"
              /></a>
              <a
                :href="`${repositoryUrl}/releases`"
                target="_blank"
                rel="noopener noreferrer"
                >{{ t.dl.history }}<ArrowUpRight :size="13"
              /></a>
            </footer>
          </div>
        </Transition>
      </div>

      <p class="download-note">
        {{ detected ? asset && t.dl.notes[platform][mode] : t.hero.noMatch }}
      </p>
    </div>
    <div class="hero-art" aria-hidden="true">
      <img :src="logo" width="384" height="384" fetchpriority="high" alt="" />
      <span class="hero-shadow" />
    </div>
  </section>

  <nav class="next-links" :aria-label="t.next.label">
    <a :href="href('features')">
      <strong>{{ t.next.features[0] }}</strong>
      <span>{{ t.next.features[1] }}</span>
      <ArrowRight :size="20" />
    </a>
    <a :href="href('docs', 'install')">
      <strong>{{ t.next.docs[0] }}</strong>
      <span>{{ t.next.docs[1] }}</span>
      <ArrowRight :size="20" />
    </a>
  </nav>
  <p class="source-note">{{ t.dl.source }}</p>
</template>
