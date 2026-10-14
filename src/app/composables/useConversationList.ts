import { ref } from "vue";

import { listConversations, type ConversationRecord } from "../../bridge/agent";

/**
 * 历史会话列表的**共享数据源**。
 *
 * 侧栏（最近任务）与「任务」页展示的是同一份历史对话。此前两处各自
 * `listConversations()`：侧栏只在挂载时读一次、之后永不刷新，于是新建的
 * 会话/空间要重启应用才出现在侧栏 —— 表现为「侧栏与任务页不同步」。
 *
 * 这里收敛为模块级单例：任一处 refresh() 或直接改写 `conversations`，
 * 另一处立即同步。列表量级很小（几十条），无需分页或缓存失效策略。
 */
const conversations = ref<ConversationRecord[]>([]);
/** 是否至少成功加载过一次（用于避免重复请求与首屏骨架判定）。 */
const loaded = ref(false);

export function useConversationList() {
  /** 重新拉取列表。失败时抛出，交由调用方决定如何呈现错误。 */
  async function refresh(): Promise<ConversationRecord[]> {
    const records = await listConversations();
    conversations.value = records;
    loaded.value = true;
    return records;
  }

  /** 首次使用时的惰性加载：已加载过则直接返回，避免重复请求。 */
  async function ensureLoaded(): Promise<void> {
    if (loaded.value) return;
    try {
      await refresh();
    } catch {
      // 读取失败保持静默：调用方（如侧栏）将其视为辅助信息缺失，
      // 需要错误呈现的页面请直接调用 refresh() 并自行 catch。
    }
  }

  return { conversations, loaded, refresh, ensureLoaded };
}
