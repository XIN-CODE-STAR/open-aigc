import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";

import CanvasContextMenu from "./CanvasContextMenu.vue";

afterEach(() => {
  document.body.innerHTML = "";
});

const items = [
  { key: "note", label: "新建便签" },
  { key: "paste", label: "粘贴节点", disabled: true },
  { key: "delete", label: "删除节点", danger: true },
];

function requiredElement<T extends Element>(selector: string): T {
  const el = document.body.querySelector(selector);
  if (!(el instanceof Element)) throw new Error(`missing element: ${selector}`);
  return el as T;
}

describe("CanvasContextMenu", () => {
  it("teleports menu items to body and emits select + close on choose", async () => {
    const wrapper = mount(CanvasContextMenu, {
      attachTo: document.body,
      props: { x: 40, y: 60, items },
    });

    const buttons = [...requiredElement(".canvas-menu").querySelectorAll("button")];
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(["新建便签", "粘贴节点", "删除节点"]);

    buttons[0]?.click();
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted("select")).toEqual([["note"]]);
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("ignores disabled items and styles danger items", async () => {
    const wrapper = mount(CanvasContextMenu, {
      attachTo: document.body,
      props: { x: 40, y: 60, items },
    });

    const buttons = [...requiredElement(".canvas-menu").querySelectorAll("button")];
    expect(buttons[1]?.disabled).toBe(true);
    expect(buttons[2]?.classList.contains("canvas-menu__item--danger")).toBe(true);

    buttons[1]?.click();
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted("select")).toBeUndefined();
  });

  it("emits close when the backdrop is clicked or right-clicked", async () => {
    const wrapper = mount(CanvasContextMenu, {
      attachTo: document.body,
      props: { x: 40, y: 60, items },
    });

    const backdrop = requiredElement(".canvas-menu-backdrop");
    backdrop.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted("close")).toHaveLength(1);

    backdrop.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted("close")).toHaveLength(2);
  });

  it("repositions when x/y props change", async () => {
    const wrapper = mount(CanvasContextMenu, {
      attachTo: document.body,
      props: { x: 40, y: 60, items },
    });
    await wrapper.vm.$nextTick();
    const menu = requiredElement(".canvas-menu") as HTMLElement;
    expect(menu.style.left).toBe("40px");
    expect(menu.style.top).toBe("60px");

    await wrapper.setProps({ x: 120, y: 80 });
    await wrapper.vm.$nextTick();
    expect(menu.style.left).toBe("120px");
    expect(menu.style.top).toBe("80px");
  });
});
