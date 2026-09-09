import { computed, ref } from "vue";

import {
  createCredential,
  deleteCredential,
  listCredentials,
  updateCredential,
  type CreateCredentialInput,
  type CredentialRecord,
  type UpdateCredentialInput,
} from "../../../bridge/credentials";

/**
 * 模型凭据管理 composable。
 * 封装 bridge 层 CRUD，暴露响应式列表与操作状态。
 */
export function useCredentials() {
  const credentials = ref<CredentialRecord[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const saving = ref(false);

  const enabledCredentials = computed(() => credentials.value.filter((c) => c.enabled));

  async function refresh(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      credentials.value = await listCredentials();
    } catch (e) {
      error.value = e instanceof Error ? e.message : "加载模型列表失败。";
    } finally {
      loading.value = false;
    }
  }

  async function add(input: CreateCredentialInput): Promise<CredentialRecord | null> {
    saving.value = true;
    error.value = null;
    try {
      const record = await createCredential(input);
      credentials.value = [...credentials.value, record];
      return record;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "添加模型失败。";
      return null;
    } finally {
      saving.value = false;
    }
  }

  async function edit(input: UpdateCredentialInput): Promise<CredentialRecord | null> {
    saving.value = true;
    error.value = null;
    try {
      const record = await updateCredential(input);
      credentials.value = credentials.value.map((c) => (c.id === record.id ? record : c));
      return record;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "更新模型失败。";
      return null;
    } finally {
      saving.value = false;
    }
  }

  async function remove(id: string): Promise<boolean> {
    saving.value = true;
    error.value = null;
    try {
      await deleteCredential(id);
      credentials.value = credentials.value.filter((c) => c.id !== id);
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "删除模型失败。";
      return false;
    } finally {
      saving.value = false;
    }
  }

  return {
    credentials,
    enabledCredentials,
    loading,
    error,
    saving,
    refresh,
    add,
    edit,
    remove,
  };
}
