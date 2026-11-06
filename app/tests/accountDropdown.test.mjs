import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, nextTick, reactive } from "vue";

const file = new URL("../src/components/AccountDropdown.vue", import.meta.url);
const moduleUrl = (text) =>
  `data:text/javascript;base64,${Buffer.from(text).toString("base64")}`;
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "account-dropdown",
  }).content,
  {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  },
).outputText;
code = code.replace(
  /from ["']([^"']+)["']/g,
  (_, name) =>
    `from ${JSON.stringify(name === "../i18n" ? moduleUrl("export const t = x => x") : import.meta.resolve(name))}`,
);
const Dropdown = (await import(moduleUrl(code))).default;

function fixture() {
  const previous = { document: globalThis.document, Node: globalThis.Node };
  const listeners = new Map();
  class Element {
    children = [];
    input = false;
    contains(node) {
      return node === this || this.children.includes(node);
    }
    closest() {
      return this.input ? this : null;
    }
    focus() {
      document.activeElement = this;
    }
    scrollIntoView() {}
  }
  const root = new Element(),
    panel = new Element(),
    trigger = new Element();
  const options = [new Element(), new Element(), new Element()];
  const input = new Element(),
    outside = new Element();
  input.input = true;
  panel.children = [...options, input];
  root.children = [panel, trigger, ...panel.children];
  root.querySelector = () => trigger;
  panel.querySelectorAll = () => options;
  const document = {
    activeElement: trigger,
    addEventListener: (name, handler) => listeners.set(name, handler),
    removeEventListener: (name) => listeners.delete(name),
  };
  Object.assign(globalThis, { document, Node: Element });
  const props = reactive({ open: false });
  let ui;
  const renderer = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = renderer.createApp({
    setup() {
      ui = Dropdown.setup(props, {
        expose() {},
        emit: (_, open) => {
          props.open = open;
        },
      });
      ui.root.value = root;
      ui.panel.value = panel;
      return () => null;
    },
  });
  app.mount({});
  return {
    ui,
    props,
    document,
    trigger,
    options,
    input,
    outside,
    listeners,
    key(key, target = document.activeElement) {
      return {
        key,
        target,
        prevented: false,
        stopped: false,
        preventDefault() {
          this.prevented = true;
        },
        stopPropagation() {
          this.stopped = true;
        },
      };
    },
    dispose() {
      app.unmount();
      Object.assign(globalThis, previous);
    },
  };
}

test("dropdown opens without moving focus, closes outside/on Tab, and releases listeners", async () => {
  const f = fixture();
  try {
    f.props.open = true;
    await nextTick();
    assert.equal(f.document.activeElement, f.trigger);
    f.listeners.get("pointerdown")({ target: f.options[0] });
    assert.equal(f.props.open, true);
    f.ui.leave({ relatedTarget: f.input });
    f.ui.leave({ relatedTarget: null });
    assert.equal(f.props.open, true);
    f.listeners.get("pointerdown")({ target: f.outside });
    assert.equal(f.props.open, false);
    f.props.open = true;
    await nextTick();
    f.outside.focus();
    f.ui.leave({ relatedTarget: f.outside });
    await nextTick();
    assert.equal(f.props.open, false);
    assert.equal(f.document.activeElement, f.outside);
  } finally {
    f.dispose();
  }
  assert.equal(f.listeners.size, 0);
});

test("arrow keys navigate avatars, editing keeps cursor keys, and Escape restores trigger focus", async () => {
  const f = fixture();
  try {
    await f.ui.navigate(f.key("ArrowDown"));
    assert.equal(f.props.open, true);
    assert.equal(f.document.activeElement, f.options[0]);
    await f.ui.navigate(f.key("End"));
    assert.equal(f.document.activeElement, f.options[2]);
    await f.ui.navigate(f.key("ArrowRight"));
    assert.equal(f.document.activeElement, f.options[0]);
    f.input.focus();
    const editing = f.key("ArrowLeft");
    await f.ui.navigate(editing);
    assert.equal(editing.prevented, false);
    assert.equal(f.document.activeElement, f.input);
    f.options[1].focus();
    const escape = f.key("Escape");
    await f.ui.navigate(escape);
    assert.equal(escape.stopped, true);
    assert.equal(f.props.open, false);
    assert.equal(f.document.activeElement, f.trigger);
    f.props.open = true;
    await nextTick();
    f.options[0].focus();
    f.props.open = false; // The parent closes after a successful selection.
    await nextTick();
    assert.equal(f.document.activeElement, f.trigger);
  } finally {
    f.dispose();
  }
});
