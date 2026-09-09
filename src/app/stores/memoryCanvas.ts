import { ref } from "vue";
import { defineStore } from "pinia";

/**
 * 无限画布面板的全局展开/收起状态。
 * 按钮在 AppShell 顶部栏，面板在 GenerationsPage，通过此 store 同步。
 */
export const useMemoryCanvasStore = defineStore("memoryCanvas", () => {
  const visible = ref(false);

  function toggle(): void {
    visible.value = !visible.value;
  }

  function open(): void {
    visible.value = true;
  }

  function close(): void {
    visible.value = false;
  }

  return { visible, toggle, open, close };
});
