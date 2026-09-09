import type { VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import App from "./App.vue";
import { createAppRouter } from "./app/router";
import { usePreferencesStore } from "./app/stores/preferences";

let wrapper: VueWrapper | undefined;

async function mountApp(path = "/") {
  const pinia = createPinia();
  setActivePinia(pinia);

  const router = createAppRouter();
  await router.push(path);
  await router.isReady();
  usePreferencesStore(pinia).initialize();

  wrapper = mount(App, {
    attachTo: document.body,
    global: {
      plugins: [pinia, router],
    },
  });
  await flushPromises();

  return { pinia, router, wrapper };
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-density");
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
});

describe("App", () => {
  it("renders the settings page and primary navigation", async () => {
    const mounted = await mountApp("/settings");

    expect(mounted.wrapper.get("h1").text()).toBe("设置");
    const mainNavigation = mounted.wrapper.get('[aria-label="主导航"]').text();
    expect(mainNavigation).toContain("资源库");
    expect(mainNavigation).toContain("提示词库");
  });

  it("changes and persists the color theme", async () => {
    const mounted = await mountApp("/settings");
    const darkThemeButton = mounted.wrapper
      .findAll('[role="radio"]')
      .find((button) => button.text().includes("深色"));

    expect(darkThemeButton).toBeDefined();
    await darkThemeButton?.trigger("click");

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(window.localStorage.getItem("aigc-studio.preferences.v1")).toContain('"theme":"dark"');
  });
});
