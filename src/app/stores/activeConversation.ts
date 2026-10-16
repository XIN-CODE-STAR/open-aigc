import { ref } from "vue";
import { defineStore } from "pinia";

/**
 * 「当前活跃会话」的跨页面共享状态。
 *
 * 为什么需要它：记忆画布是**每会话一份**的（后端按 `conv-<会话ID>` 查找）。
 * 打开画布时应落在用户此刻正在创意工坊里查看/运行的那个会话上，而不是
 * 机械地取「最近更新的会话」—— 后者在多个会话之间来回切换时会跳错。
 *
 * 由创意工坊（GenerationsPage）在选中会话变化时写入；记忆画布页读取。
 */
export const useActiveConversationStore = defineStore("activeConversation", () => {
  const id = ref<string | null>(null);

  function set(next: string | null): void {
    // 相同值不重复赋值，避免无意义的响应式触发
    if (id.value === next) return;
    id.value = next;
  }

  return { id, set };
});
