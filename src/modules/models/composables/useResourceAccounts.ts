import { ref } from "vue";

import {
  createResourceAccount,
  deleteResourceAccount,
  listResourceAccounts,
  setAccountEnabled,
  updateAccountSession,
  updateResourceAccount,
  type CreateResourceAccountInput,
  type ResourceAccountRecord,
} from "../../../bridge/resourceAccounts";

/**
 * 资源账号管理 composable。
 * 封装 bridge 层 CRUD，暴露响应式列表与操作状态。
 */
export function useResourceAccounts() {
  const accounts = ref<ResourceAccountRecord[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const saving = ref(false);

  async function refresh(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      accounts.value = await listResourceAccounts();
    } catch (e) {
      error.value = e instanceof Error ? e.message : "加载账号列表失败。";
    } finally {
      loading.value = false;
    }
  }

  async function add(input: CreateResourceAccountInput): Promise<ResourceAccountRecord | null> {
    saving.value = true;
    error.value = null;
    try {
      const record = await createResourceAccount(input);
      accounts.value = [...accounts.value, record];
      return record;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "连接账号失败。";
      return null;
    } finally {
      saving.value = false;
    }
  }

  async function refreshSession(id: string, sessionSecret: string): Promise<boolean> {
    saving.value = true;
    error.value = null;
    try {
      await updateAccountSession(id, sessionSecret);
      await refresh();
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "更新 Session 失败。";
      return false;
    } finally {
      saving.value = false;
    }
  }

  async function edit(input: {
    id: string;
    displayName: string;
    sessionSecret?: string;
  }): Promise<ResourceAccountRecord | null> {
    saving.value = true;
    error.value = null;
    try {
      // 更新 Session（如提供）
      if (input.sessionSecret) {
        await updateAccountSession(input.id, input.sessionSecret);
      }
      // 更新元数据
      const record = await updateResourceAccount({
        id: input.id,
        displayName: input.displayName,
      });
      await refresh();
      return record;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "更新账号失败。";
      return null;
    } finally {
      saving.value = false;
    }
  }

  async function toggleEnabled(id: string, enabled: boolean): Promise<boolean> {
    error.value = null;
    try {
      await setAccountEnabled(id, enabled);
      accounts.value = accounts.value.map((a) => (a.id === id ? { ...a, enabled } : a));
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "操作失败。";
      return false;
    }
  }

  async function remove(id: string): Promise<boolean> {
    saving.value = true;
    error.value = null;
    try {
      await deleteResourceAccount(id);
      accounts.value = accounts.value.filter((a) => a.id !== id);
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "删除账号失败。";
      return false;
    } finally {
      saving.value = false;
    }
  }

  return {
    accounts,
    loading,
    error,
    saving,
    refresh,
    add,
    edit,
    refreshSession,
    toggleEnabled,
    remove,
  };
}
