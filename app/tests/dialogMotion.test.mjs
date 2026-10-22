import assert from "node:assert/strict";
import test from "node:test";
import {
  rememberDialogFocus,
  prepareDialogLeave,
  finishDialogLeave,
} from "../src/dialogMotion.ts";

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
