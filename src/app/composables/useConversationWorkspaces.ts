import { ref } from "vue";

const STORAGE_KEY = "aigc-studio.conversation-workspaces";

/**
 * 「未归属任何工作目录」的哨兵值。
 *
 * 不能直接用默认工作区显示名（"OPEN AIGC"）来判断：那同时也是一个合法的
 * 文件夹名 —— 用户若真的打开了名为 OPEN AIGC 的目录，其对话会被误判为
 * 未分组。因此显式记录哨兵，把「默认区」与「恰好同名的真实空间」区分开。
 */
export const DEFAULT_WORKSPACE = "__default__";

/**
 * 追踪每个对话属于哪个工作目录（localStorage）。
 *
 * - 当用户在某个工作目录下创建对话时，调用 `record(conversationId, workspaceName)`
 * - 侧边栏通过 `getWorkspaceName(conversationId)` 获取分组标签
 * - 删除对话时调用 `remove(conversationId)` 清理
 */
const mappings = ref<Record<string, string>>(loadMappings());

function loadMappings(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(mappings.value));
  } catch {
    // ignore quota errors
  }
}

export function useConversationWorkspaces() {
  /**
   * 记录对话所属工作目录。
   * `workspaceName` 为空（未选择自定义目录）时写入哨兵，与真实目录区分。
   */
  function record(conversationId: string, workspaceName: string | null | undefined): void {
    const value =
      typeof workspaceName === "string" && workspaceName.trim().length > 0
        ? workspaceName.trim()
        : DEFAULT_WORKSPACE;
    mappings.value[conversationId] = value;
    persist();
  }

  /**
   * 取对话所属空间名。
   * 返回 null 表示「默认区」—— 含未记录、以及显式记为哨兵的两种情况，
   * 便于调用方与「恰好叫 OPEN AIGC 的真实目录」区分开。
   */
  function getWorkspaceName(conversationId: string): string | null {
    const value = mappings.value[conversationId];
    if (!value || value === DEFAULT_WORKSPACE) return null;
    return value;
  }

  function remove(conversationId: string): void {
    if (conversationId in mappings.value) {
      mappings.value = Object.fromEntries(
        Object.entries(mappings.value).filter(([key]) => key !== conversationId),
      );
      persist();
    }
  }

  function getAll(): Record<string, string> {
    return { ...mappings.value };
  }

  return { record, getWorkspaceName, remove, getAll, DEFAULT_WORKSPACE };
}
