import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import BaseBadge from "./BaseBadge.vue";
import BaseButton from "./BaseButton.vue";
import BaseInput from "./BaseInput.vue";
import BaseSelect from "./BaseSelect.vue";
import BaseSwitch from "./BaseSwitch.vue";
import BaseTabs from "./BaseTabs.vue";

describe("BaseButton", () => {
  it("渲染 variant/size 类并转发点击", async () => {
    const wrapper = mount(BaseButton, {
      props: { variant: "primary", size: "sm" },
      slots: { default: "保存" },
    });
    expect(wrapper.classes()).toContain("base-btn--primary");
    expect(wrapper.classes()).toContain("base-btn--sm");
    expect(wrapper.text()).toBe("保存");
    await wrapper.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
  });

  it("disabled 时拦截点击", async () => {
    const wrapper = mount(BaseButton, { props: { disabled: true } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
  });
});

describe("BaseInput", () => {
  it("v-model 双向绑定", async () => {
    const wrapper = mount(BaseInput, { props: { modelValue: "a" } });
    await wrapper.find("input").setValue("hello");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["hello"]);
  });

  it("error 优先于 hint 展示", () => {
    const wrapper = mount(BaseInput, { props: { hint: "提示", error: "必填" } });
    expect(wrapper.text()).toContain("必填");
    expect(wrapper.text()).not.toContain("提示");
  });
});

describe("BaseSelect", () => {
  it("切换选项发出 update:modelValue", async () => {
    const wrapper = mount(BaseSelect, {
      props: {
        modelValue: "a",
        options: [
          { value: "a", label: "A" },
          { value: "b", label: "B" },
        ],
      },
    });
    await wrapper.find("select").setValue("b");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["b"]);
  });
});

describe("BaseBadge", () => {
  it("tone 类与 dot 渲染", () => {
    const wrapper = mount(BaseBadge, {
      props: { tone: "success", dot: true },
      slots: { default: "正常" },
    });
    expect(wrapper.classes()).toContain("base-badge--success");
    expect(wrapper.find(".base-badge__dot").exists()).toBe(true);
    expect(wrapper.text()).toContain("正常");
  });
});

describe("BaseSwitch", () => {
  it("点击切换开关状态", async () => {
    const wrapper = mount(BaseSwitch, { props: { modelValue: false } });
    await wrapper.trigger("click");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([true]);
  });
});

describe("BaseTabs", () => {
  it("点击标签切换", async () => {
    const wrapper = mount(BaseTabs, {
      props: {
        modelValue: "a",
        tabs: [
          { value: "a", label: "A" },
          { value: "b", label: "B" },
        ],
      },
    });
    await wrapper.findAll("button")[1].trigger("click");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["b"]);
  });
});
