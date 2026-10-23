import assert from "node:assert/strict";
import test from "node:test";
import { createPanelSizer } from "../src/panelSize.ts";

test("设置弹窗按内容展开和收缩，连续变化从当前动画尺寸重新定位，隐藏时不锁成零高", () => {
  const previous = {
    ResizeObserver: globalThis.ResizeObserver,
    getComputedStyle: globalThis.getComputedStyle,
  };
  let observer;
  globalThis.ResizeObserver = class {
    elements = [];
    disconnected = false;
    constructor(callback) {
      this.callback = callback;
      observer = this;
    }
    observe(element) {
      this.elements.push(element);
    }
    disconnect() {
      this.disconnected = true;
    }
  };
  globalThis.getComputedStyle = (element) => element.metrics;
  let visible = false;
  const header = { offsetHeight: 76, metrics: { marginBottom: "20px" } };
  const content = { offsetHeight: 268, metrics: {} };
  const classes = new Set();
  const panel = {
    children: [header, content],
    style: { height: "" },
    metrics: { borderTopWidth: "1px", borderBottomWidth: "1px" },
    classList: {
      add: (key) => classes.add(key),
      remove: (key) => classes.delete(key),
    },
    getClientRects: () => (visible ? [{}] : []),
  };
  try {
    const dispose = createPanelSizer(panel);
    assert.equal(panel.style.height, "");
    assert.deepEqual(observer.elements, [header, content]);
    visible = true;
    observer.callback();
    assert.equal(panel.style.height, "366px");
    content.offsetHeight = 390;
    observer.callback();
    assert.equal(panel.style.height, "488px");
    content.offsetHeight = 310;
    observer.callback();
    assert.equal(panel.style.height, "408px");
    content.offsetHeight = 268;
    observer.callback();
    assert.equal(panel.style.height, "366px");
    dispose();
    assert.equal(observer.disconnected, true);
    assert.equal(panel.style.height, "");
    assert.equal(classes.size, 0);
  } finally {
    Object.assign(globalThis, previous);
  }
});
