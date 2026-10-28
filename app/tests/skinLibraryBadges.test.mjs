import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import ts from "typescript";

const moduleUrl = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const source = new URL(
  "../src/components/SkinLibraryItem.vue",
  import.meta.url,
);
const compiled = compileScript(parse(readFileSync(source, "utf8")).descriptor, {
  id: "skin-badges",
  inlineTemplate: true,
});
const code = ts
  .transpileModule(compiled.content, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  })
  .outputText.replace(
    /from ["']([^"']+)["']/g,
    (_, name) =>
      `from ${JSON.stringify(
        name === "../previewSkin"
          ? moduleUrl(
              "export const loadSkinImage = async () => null; export const skinAvatar = () => ''; ",
            )
          : import.meta.resolve(name),
      )}`,
  );
const Item = (await import(moduleUrl(code))).default;
const render = (props) =>
  renderToString(
    createSSRApp(Item, {
      load: async () => null,
      renderModel: async () => "",
      slim: false,
      live: false,
      ...props,
    }),
  );

test("guest usage always has a guest mark and never a login dot", async () => {
  for (const live of [false, true]) {
    const html = await render({ guest: true, live });
    assert.match(html, /guest-owner-mark/);
    assert.doesNotMatch(html, /<i(?:\s|>)/);
  }
});

test("shared account/guest skin keeps distinct ownership markers", async () => {
  const account = await render({ inUse: "CirnoNeko" });
  assert.match(account, /<i(?:\s|>)/);
  assert.doesNotMatch(account, /guest-owner-mark/);
  const shared = await render({ inUse: "CirnoNeko", guest: true });
  assert.equal((shared.match(/<i(?:\s|>)/g) || []).length, 1);
  assert.match(shared, /guest-owner-mark/);
});
