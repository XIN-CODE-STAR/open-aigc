import { afterEach, describe, expect, it } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

import ModalDialog from "./ModalDialog.vue";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("ModalDialog", () => {
  it("moves focus into the dialog and restores it after closing", async () => {
    const trigger = document.createElement("button");
    trigger.textContent = "打开";
    document.body.append(trigger);
    trigger.focus();
    const wrapper = mount(ModalDialog, {
      attachTo: document.body,
      props: { open: false, title: "测试对话框" },
      slots: { default: '<input id="dialog-input" autofocus />' },
    });

    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(document.activeElement).toBe(document.querySelector("#dialog-input"));

    await wrapper.setProps({ open: false });
    await flushPromises();
    expect(document.activeElement).toBe(trigger);
  });

  it("traps tab navigation and emits close for Escape", async () => {
    const wrapper = mount(ModalDialog, {
      attachTo: document.body,
      props: { open: true, title: "测试对话框" },
      slots: {
        default: '<button id="first-action">第一项</button>',
        footer: '<button id="last-action">最后一项</button>',
      },
    });
    await flushPromises();
    const first = requiredElement<HTMLButtonElement>('button[aria-label="关闭"]');
    const last = requiredElement<HTMLButtonElement>("#last-action");

    first.focus();
    first.dispatchEvent(keydown("Tab", true));
    expect(document.activeElement).toBe(last);

    last.dispatchEvent(keydown("Tab"));
    expect(document.activeElement).toBe(first);

    first.dispatchEvent(keydown("Escape"));
    expect(wrapper.emitted("close")).toHaveLength(1);
  });
});

function keydown(key: string, shiftKey = false): KeyboardEvent {
  return new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key, shiftKey });
}

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Expected element ${selector}`);
  return element;
}
