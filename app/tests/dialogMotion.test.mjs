import assert from "node:assert/strict";
import test from "node:test";
import {
  rememberDialogFocus,
  prepareDialogLeave,
  finishDialogLeave,
  pinPanelLeave,
  resetPanelLeave,
} from "../src/dialogMotion.ts";

test("replaced panels leave without affecting layout and interrupted transitions restore interaction", () => {
  const previous = globalThis.HTMLElement;
  class Panel {
    style = {
      position: "relative",
      left: "",
      top: "",
      width: "",
      height: "",
      margin: "",
      maxWidth: "",
      maxHeight: "",
    };
    inert = false;
    attributes = new Map();
    getBoundingClientRect() {
      return { left: 40, top: 60, width: 840, height: 590 };
    }
    getAttribute(key) {
      return this.attributes.get(key) ?? null;
    }
    setAttribute(key, value) {
      this.attributes.set(key, value);
    }
    removeAttribute(key) {
      this.attributes.delete(key);
    }
  }
  globalThis.HTMLElement = Panel;
  try {
    const panel = new Panel();
    pinPanelLeave(panel);
    assert.equal(panel.style.position, "fixed");
    assert.equal(panel.style.left, "40px");
    assert.equal(panel.style.width, "840px");
    assert.equal(panel.inert, true);
    assert.equal(panel.getAttribute("aria-hidden"), "true");
    // A fast back-and-forth selection may interrupt the same panel twice.
    pinPanelLeave(panel);
    resetPanelLeave(panel);
    assert.equal(panel.style.position, "relative");
    assert.equal(panel.style.width, "");
    assert.equal(panel.inert, false);
    assert.equal(panel.getAttribute("aria-hidden"), null);
  } finally {
    globalThis.HTMLElement = previous;
  }
});

test("弹窗离场保留模态直到动画结束，换窗时不抢新窗口焦点且保留最初返回目标", () => {
  const previous = {
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    HTMLDialogElement: globalThis.HTMLDialogElement,
  };
  class Element {
    isConnected = true;
    parent = null;
    focused = 0;
    closest() {
      return this.parent?.dataset.closing ? this.parent : null;
    }
    focus() {
      this.focused++;
    }
  }
  class Dialog extends Element {
    dataset = {};
    inert = false;
    open = true;
    close() {
      this.open = false;
    }
  }
  const first = new Dialog(),
    second = new Dialog(),
    trigger = new Element(),
    child = new Element();
  child.parent = first;
  globalThis.HTMLElement = Element;
  globalThis.HTMLDialogElement = Dialog;
  globalThis.document = {
    activeElement: trigger,
    querySelector: () =>
      [first, second].find((dialog) => dialog.open && !dialog.dataset.closing),
  };
  try {
    second.open = false;
    rememberDialogFocus(first);
    prepareDialogLeave(first);
    assert.equal(first.open, true);
    assert.equal(first.inert, true);
    assert.equal(trigger.focused, 0);
    document.activeElement = child;
    rememberDialogFocus(second);
    second.open = true;
    finishDialogLeave(first);
    assert.equal(first.open, false);
    assert.equal(trigger.focused, 0);
    prepareDialogLeave(second);
    finishDialogLeave(second);
    assert.equal(second.open, false);
    assert.equal(trigger.focused, 1);
    assert.equal(child.focused, 0);
  } finally {
    Object.assign(globalThis, previous);
  }
});
