import { ref } from "vue";

const STORAGE_KEY = "aigc-studio.conversation-workspaces";

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
  function record(conversationId: string, workspaceName: string): void {
    mappings.value[conversationId] = workspaceName;
    persist();
  }

  function getWorkspaceName(conversationId: string): string | null {
    return mappings.value[conversationId] ?? null;
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

  return { record, getWorkspaceName, remove, getAll };
}
