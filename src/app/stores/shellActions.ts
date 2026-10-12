import { ref } from "vue";
import { defineStore } from "pinia";

/**
 * Shell 级动作总线：侧栏等 shell 组件 → 页面（GenerationsPage）的单向动作通知。
 * 用 tick 计数而非布尔标记，连续触发也会被 watch 捕获。
 */
export const useShellActionsStore = defineStore("shellActions", () => {
  /** 新建对话请求计数；GenerationsPage watch 它并执行 startNewAgentConversation。 */
  const newConversationTick = ref(0);

  function requestNewConversation(): void {
    newConversationTick.value += 1;
  }

  return { newConversationTick, requestNewConversation };
});
