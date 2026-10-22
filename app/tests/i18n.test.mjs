import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parse } from "@vue/compiler-sfc";
import { baseParse } from "@vue/compiler-dom";
import { computed } from "vue";
import {
  english,
  locale,
  resolveLanguage,
  setLanguage,
  t,
  translateMessage,
} from "../src/i18n.ts";

test("跟随系统仅将简体中文映射为中文，其他语言和缺失区域回退英语", () => {
  for (const system of ["zh", "zh-CN", "zh_SG", "zh-Hans", "zh-Hans-HK"]) {
    assert.equal(resolveLanguage("system", system), "zh-CN");
    assert.equal(resolveLanguage("en-US", system), "en-US");
  }
  for (const system of [
    "",
    "en-US",
    "de-DE",
    "ja-JP",
    "zh-TW",
    "zh-HK",
    "zh-MO",
    "zh-Hant-CN",
  ]) {
    assert.equal(resolveLanguage("system", system), "en-US");
    assert.equal(resolveLanguage("zh-CN", system), "zh-CN");
  }
});

test("语言即时更新已有文案，恢复已保存偏好时同步恢复界面", () => {
  const title = computed(() => t("启动游戏"));
  setLanguage("system", "zh-CN");
  assert.equal(title.value, "启动游戏");
  setLanguage("en-US", "zh-CN");
  assert.equal(title.value, "Launch game");
  setLanguage("system", "zh-CN");
  assert.equal(title.value, "启动游戏");
  setLanguage("system", "fr-FR");
  assert.equal(locale.value, "en-US");
  assert.equal(title.value, "Launch game");
});

test("插值保留实例名、路径、Unicode 和大括号，不递归翻译用户内容", () => {
  setLanguage("en-US", "en-US");
  const name = "游戏目录 / {version} 😺";
  assert.equal(
    t("为 {name} 下载游戏版本", { name }),
    `Download a game version for ${name}`,
  );
  assert.equal(t("unknown {name}", { name }), `unknown ${name}`);
  assert.equal(t("constructor"), "constructor");
  assert.equal(t("English"), "English");
  for (const [source, target] of Object.entries(english)) {
    const slots = (value) =>
      [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    assert.deepEqual(slots(target), slots(source), source);
  }
});

test("后台阶段、错误链和动态提示译为英文，路径与未知原始错误保留", () => {
  setLanguage("en-US", "en-US");
  assert.equal(translateMessage("正在安装 Java 21"), "Installing Java 21");
  assert.equal(
    translateMessage("校验并下载 341 个游戏文件"),
    "Verifying and downloading 341 game files",
  );
  assert.equal(
    translateMessage(
      "无法访问导入目录：/Users/猫猫/.minecraft: Permission denied",
    ),
    "Could not access the import folder: /Users/猫猫/.minecraft: Permission denied",
  );
  assert.equal(
    translateMessage("无法获取元数据，请检查网络或切换下载源：操作已取消"),
    "Could not fetch metadata. Check your connection or change the download source: Operation cancelled",
  );
  assert.equal(
    translateMessage("Minecraft 登录失败（HTTP 403）：Forbidden"),
    "Minecraft sign-in failed (HTTP 403): Forbidden",
  );
  assert.equal(translateMessage("raw custom 错误"), "raw custom 错误");
  setLanguage("zh-CN", "en-US");
  assert.equal(translateMessage("正在安装 Java 21"), "正在安装 Java 21");
});

test("界面文案有英文条目，模板没有绕过翻译的中文文本或静态标签", () => {
  const src = fileURLToPath(new URL("../src/", import.meta.url));
  const files = [
    "App.vue",
    ...readdirSync(`${src}/components`)
      .filter((file) => file.endsWith(".vue"))
      .map((file) => `components/${file}`),
  ];
  const han = /\p{Script=Han}/u;
  for (const file of files) {
    const source = readFileSync(`${src}/${file}`, "utf8");
    for (const match of source.matchAll(/\bt\(\s*["']([^"'\n]+)["']/g)) {
      if (han.test(match[1]))
        assert.ok(Object.hasOwn(english, match[1]), `${file}: ${match[1]}`);
    }
    const template = parse(source).descriptor.template;
    if (!template) continue;
    function visit(node) {
      if (node.type === 2)
        assert.equal(han.test(node.content), false, `${file}: ${node.content}`);
      for (const prop of node.props || []) {
        if (prop.type === 6 && prop.value)
          assert.equal(
            han.test(prop.value.content),
            false,
            `${file}: ${prop.name}`,
          );
      }
      for (const child of node.children || []) visit(child);
    }
    visit(baseParse(template.content));
  }
});
