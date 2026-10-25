import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { parse, compileScript } from "@vue/compiler-sfc";
import ts from "typescript";
import { createRenderer, reactive } from "vue";
import { flush } from "./helpers/appSettingsFixture.mjs";

const url = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const file = new URL("../src/components/InstanceTarget.vue", import.meta.url);
let code = ts.transpileModule(
  compileScript(parse(readFileSync(file, "utf8")).descriptor, {
    id: "instance-target-test",
    inlineTemplate: true,
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
    `from ${JSON.stringify(
      name.endsWith(".vue")
        ? url("export default {};")
        : name === "../instanceIcons"
          ? url("export const resolveInstanceIcon = () => 0;")
          : name.startsWith(".")
            ? new URL(`${name}.ts`, file).href
            : import.meta.resolve(name),
    )}`,
);
const Target = (await import(url(code))).default;
function nodes(vnode) {
  if (!vnode) return [];
  if (Array.isArray(vnode)) return vnode.flatMap(nodes);
  const children =
    typeof vnode.children?.default === "function"
      ? vnode.children.default()
      : vnode.children;
  return [vnode, ...(typeof children === "object" ? nodes(children) : [])];
}

test("pointer selection survives WebKit's blur-before-click and emits the chosen instance", async () => {
  const previous = { document: globalThis.document, Node: globalThis.Node };
  globalThis.Node = class {};
  globalThis.document = { addEventListener() {}, removeEventListener() {} };
  const first = { id: "a", name: "A" },
    second = { id: "b", name: "B" };
  const props = reactive({
    target: first,
    instances: [first, second],
    disabled: false,
    context: "manage",
  });
  const events = [];
  let renderFunction;
  const cache = [];
  const render = () => renderFunction({}, cache);
  const host = createRenderer({
    createComment: () => ({}),
    insert() {},
    remove() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const app = host.createApp({
    setup() {
      renderFunction = Target.setup(props, {
        expose() {},
        emit: (...args) => events.push(args),
      });
      return () => null;
    },
  });
  app.mount({});
  try {
    nodes(render())
      .find((node) => node.props?.class === "instance-target-trigger")
      .props.onClick();
    await flush();
    const choices = nodes(render()).filter(
      (node) => node.props?.role === "menuitemradio",
    );
    assert.equal(choices.length, 2, "management has no New instance option");
    let prevented = false;
    choices[1].props.onPointerdown?.({
      preventDefault() {
        prevented = true;
      },
    });
    // WebKit does not focus a clicked button. Without prevention, the currently
    // focused menu item blurs to body and focusout removes the menu before click.
    if (!prevented) render().props.onFocusout({ relatedTarget: null });
    assert.equal(
      nodes(render()).filter((node) => node.props?.role === "menuitemradio")
        .length,
      2,
    );
    choices[1].props.onClick();
    assert.deepEqual(events, [["select", "b"]]);
    assert.equal(
      nodes(render()).filter((node) => node.props?.role === "menuitemradio")
        .length,
      0,
    );
  } finally {
    app.unmount();
    Object.assign(globalThis, previous);
  }
});
