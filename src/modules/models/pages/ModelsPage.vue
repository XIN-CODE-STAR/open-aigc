<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Bot, Link, LoaderCircle, Pencil, Plus, Trash2, UserCircle } from "@lucide/vue";

import ProviderLogo from "../components/ProviderLogo.vue";

import type { CredentialRecord } from "../../../bridge/credentials";
import type { ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import { useToast } from "../../../shared/ui/useToast";
import AccountConnectDialog from "../components/AccountConnectDialog.vue";
import CredentialFormDialog from "../components/CredentialFormDialog.vue";
import { useCredentials } from "../composables/useCredentials";
import { useResourceAccounts } from "../composables/useResourceAccounts";

const toast = useToast();
const { credentials, loading, error, saving, refresh, add, edit, remove } = useCredentials();
const {
  accounts,
  loading: accountsLoading,
  saving: accountsSaving,
  refresh: refreshAccounts,
  add: addAccount,
  edit: editAccount,
  toggleEnabled,
  remove: removeAccount,
} = useResourceAccounts();

/** 按 provider 分组的账号列表 */
const groupedAccounts = computed(() => {
  const groups = new Map<string, ResourceAccountRecord[]>();
  for (const a of accounts.value) {
    const key = a.providerId;
    const existing = groups.get(key) || [];
    existing.push(a);
    groups.set(key, existing);
  }
  return [...groups.entries()].map(([providerId, items]) => ({
    providerId,
    displayName: providerDisplayNames[providerId] || providerId,
    items,
    activeCount: items.filter((a) => a.status === "active").length,
    totalCount: items.length,
  }));
});

/** 供应商显示名称映射 */
const providerDisplayNames: Record<string, string> = {
  jimeng: "即梦AI",
  kling: "可灵",
  seedance: "Seedance",
  midjourney: "Midjourney",
  doubao: "豆包",
};

const dialogOpen = ref(false);
const accountDialogOpen = ref(false);
const editingCredential = ref<CredentialRecord | null>(null);
const editingAccount = ref<ResourceAccountRecord | null>(null);
const deletingId = ref<string | null>(null);
const deletingAccountId = ref<string | null>(null);

onMounted(() => {
  void refresh();
  void refreshAccounts();
});

function openAddDialog(): void {
  editingCredential.value = null;
  dialogOpen.value = true;
}

function openAccountDialog(): void {
  editingAccount.value = null;
  accountDialogOpen.value = true;
}

function openEditAccountDialog(account: ResourceAccountRecord): void {
  editingAccount.value = account;
  accountDialogOpen.value = true;
}

function openEditDialog(credential: CredentialRecord): void {
  editingCredential.value = credential;
  dialogOpen.value = true;
}

function closeDialog(): void {
  dialogOpen.value = false;
  editingCredential.value = null;
}

function closeAccountDialog(): void {
  accountDialogOpen.value = false;
}

async function handleSubmit(payload: {
  providerName: string;
  displayName: string;
  baseUrl: string;
  modelName: string;
  apiKey: string;
}): Promise<void> {
  if (editingCredential.value) {
    const result = await edit({
      id: editingCredential.value.id,
      providerName: payload.providerName,
      displayName: payload.displayName,
      baseUrl: payload.baseUrl,
      modelName: payload.modelName,
      apiKey: payload.apiKey || undefined,
    });
    if (result) {
      toast.success("模型已更新。");
      closeDialog();
    } else {
      toast.error(error.value ?? "更新失败。");
    }
  } else {
    const result = await add(payload);
    if (result) {
      toast.success("模型已添加。");
      closeDialog();
    } else {
      toast.error(error.value ?? "添加失败。");
    }
  }
}

async function handleAccountSubmit(payload: {
  providerName: string;
  displayName: string;
  baseUrl: string;
  modelName: string;
  sessionCookie: string;
}): Promise<void> {
  if (editingAccount.value) {
    // 编辑模式：更新显示名称 + 可选更新 Session
    const result = await editAccount({
      id: editingAccount.value.id,
      displayName: payload.displayName,
      sessionSecret: payload.sessionCookie || undefined,
    });
    if (result) {
      toast.success("账号已更新。");
      closeAccountDialog();
    } else {
      toast.error(error.value ?? "更新失败。");
    }
  } else {
    const result = await addAccount({
      providerId: payload.providerName,
      displayName: payload.displayName,
      baseUrl: payload.baseUrl,
      sessionSecret: payload.sessionCookie,
    });
    if (result) {
      toast.success("账号已连接。");
      closeAccountDialog();
    } else {
      toast.error(error.value ?? "连接失败。");
    }
  }
}

async function handleDeleteAccount(account: ResourceAccountRecord): Promise<void> {
  deletingAccountId.value = account.id;
  const ok = await removeAccount(account.id);
  deletingAccountId.value = null;
  if (ok) {
    toast.success(`已断开「${account.displayName}」。`);
  } else {
    toast.error("断开账号失败。");
  }
}

async function handleToggleAccount(account: ResourceAccountRecord): Promise<void> {
  await toggleEnabled(account.id, !account.enabled);
}

function statusLabel(status: string): string {
  switch (status) {
    case "active":
      return "正常";
    case "need_login":
      return "需重新登录";
    case "expired":
      return "已过期";
    case "blocked":
      return "已封禁";
    default:
      return "未知";
  }
}

function statusClass(status: string): string {
  switch (status) {
    case "active":
      return "badge--active";
    case "need_login":
    case "expired":
      return "badge--warning";
    case "blocked":
      return "badge--danger";
    default:
      return "badge--disabled";
  }
}

async function handleDelete(credential: CredentialRecord): Promise<void> {
  deletingId.value = credential.id;
  const ok = await remove(credential.id);
  deletingId.value = null;
  if (ok) {
    toast.success(`已删除「${credential.displayName}」。`);
  } else {
    toast.error(error.value ?? "删除失败。");
  }
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}
</script>

<template>
  <div class="models-page">
    <header class="page-header">
      <div class="page-header__text">
        <h2 class="page-subtitle">模型管理</h2>
        <p class="page-description">配置 AI 供应商凭据，创意工坊将使用这些模型进行生成。</p>
      </div>
      <button class="btn btn--primary" type="button" @click="openAddDialog">
        <Plus :size="16" :stroke-width="2" />
        <span>添加模型</span>
      </button>
      <button class="btn btn--secondary" type="button" @click="openAccountDialog">
        <Link :size="16" :stroke-width="2" />
        <span>连接AI账号</span>
      </button>
    </header>

    <div v-if="loading" class="state-block">
      <LoaderCircle class="is-spinning" :size="20" />
      <span>正在加载模型列表...</span>
    </div>

    <div v-else-if="error && credentials.length === 0" class="state-block state-block--error">
      <span>{{ error }}</span>
      <button class="btn btn--secondary" type="button" @click="refresh">重试</button>
    </div>

    <div v-else-if="credentials.length === 0" class="state-block">
      <Bot :size="32" :stroke-width="1.4" />
      <span>尚未配置任何模型</span>
      <button class="btn btn--primary" type="button" @click="openAddDialog">
        <Plus :size="16" :stroke-width="2" />
        <span>添加第一个模型</span>
      </button>
    </div>

    <ul v-else class="credential-list">
      <li v-for="credential in credentials" :key="credential.id" class="credential-card">
        <div class="credential-card__main">
          <ProviderLogo
            :provider="credential.providerName"
            :label="credential.displayName"
            :size="36"
          />
          <div class="credential-card__info">
            <div class="credential-card__name">
              {{ credential.displayName }}
              <span v-if="!credential.enabled" class="badge badge--disabled">已禁用</span>
            </div>
            <div class="credential-card__meta">
              <span class="meta-tag">{{ credential.providerName }}</span>
              <span class="meta-tag">{{ credential.modelName }}</span>
              <span class="meta-url">{{ credential.baseUrl }}</span>
            </div>
            <div class="credential-card__time">添加于 {{ formatDate(credential.createdAt) }}</div>
          </div>
        </div>
        <div class="credential-card__actions">
          <button
            class="icon-btn"
            type="button"
            title="编辑"
            aria-label="编辑"
            @click="openEditDialog(credential)"
          >
            <Pencil :size="15" :stroke-width="1.8" />
          </button>
          <button
            class="icon-btn icon-btn--danger"
            type="button"
            title="删除"
            aria-label="删除"
            :disabled="deletingId === credential.id"
            @click="handleDelete(credential)"
          >
            <Trash2 :size="15" :stroke-width="1.8" />
          </button>
        </div>
      </li>
    </ul>

    <!-- AI 账号区块 -->
    <section class="accounts-section">
      <h3 class="section-title">
        <UserCircle :size="18" :stroke-width="1.8" />
        <span>AI 账号</span>
      </h3>
      <p class="section-description">已连接的 AI 平台账号，使用你的会员权益进行创作。</p>

      <div v-if="accountsLoading" class="state-block state-block--compact">
        <LoaderCircle class="is-spinning" :size="16" />
        <span>加载账号...</span>
      </div>

      <div v-else-if="accounts.length === 0" class="state-block state-block--compact">
        <span class="empty-hint">尚未连接任何 AI 账号</span>
      </div>

      <div v-else class="provider-groups">
        <div v-for="group in groupedAccounts" :key="group.providerId" class="provider-group">
          <div class="provider-group__header">
            <span class="provider-group__name">{{ group.displayName }}</span>
            <span class="provider-group__badge">
              {{ group.activeCount }}/{{ group.totalCount }} 可用
            </span>
          </div>
          <ul class="credential-list">
            <li v-for="account in group.items" :key="account.id" class="credential-card">
              <div class="credential-card__main">
                <div
                  class="credential-card__icon credential-card__icon--account"
                  aria-hidden="true"
                >
                  <UserCircle :size="20" :stroke-width="1.6" />
                </div>
                <div class="credential-card__info">
                  <div class="credential-card__name">
                    {{ account.displayName }}
                    <span class="badge" :class="statusClass(account.status)">
                      {{ statusLabel(account.status) }}
                    </span>
                    <span v-if="!account.enabled" class="badge badge--disabled">已禁用</span>
                  </div>
                  <div class="credential-card__meta">
                    <span class="meta-tag">{{ account.accountType }}</span>
                  </div>
                  <div class="credential-card__time">
                    连接于 {{ formatDate(account.createdAt) }}
                  </div>
                </div>
              </div>
              <div class="credential-card__actions">
                <button
                  class="icon-btn"
                  type="button"
                  title="编辑"
                  aria-label="编辑"
                  @click="openEditAccountDialog(account)"
                >
                  <Pencil :size="15" :stroke-width="1.8" />
                </button>
                <button
                  class="icon-btn"
                  type="button"
                  :title="account.enabled ? '禁用' : '启用'"
                  :aria-label="account.enabled ? '禁用' : '启用'"
                  @click="handleToggleAccount(account)"
                >
                  <span class="toggle-dot" :class="{ 'toggle-dot--off': !account.enabled }" />
                </button>
                <button
                  class="icon-btn icon-btn--danger"
                  type="button"
                  title="断开"
                  aria-label="断开"
                  :disabled="deletingAccountId === account.id || accountsSaving"
                  @click="handleDeleteAccount(account)"
                >
                  <Trash2 :size="15" :stroke-width="1.8" />
                </button>
              </div>
            </li>
          </ul>
        </div>
      </div>
    </section>

    <CredentialFormDialog
      :open="dialogOpen"
      :editing="editingCredential"
      :busy="saving"
      @close="closeDialog"
      @submit="handleSubmit"
    />

    <AccountConnectDialog
      :open="accountDialogOpen"
      :editing="editingAccount"
      :busy="accountsSaving"
      @close="closeAccountDialog"
      @submit="handleAccountSubmit"
    />
  </div>
</template>

<style scoped>
.models-page {
  display: flex;
  max-width: 800px;
  flex-direction: column;
  gap: var(--space-5);
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
}

.page-subtitle {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-title-3);
  font-weight: 600;
  line-height: 24px;
  letter-spacing: -0.01em;
}

.page-description {
  margin: var(--space-1) 0 0;
  color: var(--color-text-secondary);
  font-size: var(--text-subhead);
  line-height: 20px;
}

.btn {
  display: inline-flex;
  height: 36px;
  padding: 0 var(--space-4);
  align-items: center;
  gap: var(--space-2);
  border: none;
  border-radius: var(--radius-control);
  font-size: var(--text-subhead);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition:
    background var(--duration-fast) var(--ease-out),
    opacity var(--duration-fast) var(--ease-out);
}

.btn:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.btn--primary {
  color: var(--color-on-accent, #fff);
  background: var(--color-accent);
}

.btn--primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn--secondary {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.btn--secondary:hover:not(:disabled) {
  background: var(--color-border);
}

.state-block {
  display: flex;
  min-height: 200px;
  padding: var(--space-6);
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  color: var(--color-text-secondary);
  border: 1px dashed var(--color-border);
  border-radius: var(--radius-surface);
  font-size: var(--text-subhead);
}

.state-block--error {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.is-spinning {
  color: var(--color-accent);
  animation: spin 900ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.credential-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  list-style: none;
  margin: 0;
  padding: 0;
}

.credential-card {
  display: flex;
  padding: var(--space-4);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.credential-card:hover {
  border-color: var(--color-border);
}

.credential-card__main {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: flex-start;
  gap: var(--space-3);
}

.credential-card__info {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.credential-card__name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text);
  font-size: var(--text-subhead);
  font-weight: 500;
  line-height: 20px;
}

.badge--disabled {
  padding: 1px 6px;
  color: var(--color-text-tertiary);
  border-radius: var(--radius-pill);
  background: var(--color-surface-hover);
  font-size: var(--text-caption);
  font-weight: 400;
}

.credential-card__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.meta-tag {
  padding: 1px 6px;
  color: var(--color-text-secondary);
  border-radius: 4px;
  background: var(--color-surface-hover);
  font-family: var(--font-mono, monospace);
  font-size: var(--text-caption);
  line-height: 16px;
}

.meta-url {
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  line-height: 16px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.credential-card__time {
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  line-height: 16px;
}

.credential-card__actions {
  display: flex;
  flex: 0 0 auto;
  gap: var(--space-1);
}

.icon-btn {
  display: inline-grid;
  width: 30px;
  height: 30px;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.icon-btn:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.icon-btn--danger:hover:not(:disabled) {
  color: var(--color-danger);
}

.icon-btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.accounts-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.provider-groups {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.provider-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.provider-group__header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-bottom: var(--space-1);
  border-bottom: 1px solid var(--color-border-subtle);
}

.provider-group__name {
  font-size: var(--text-subhead);
  font-weight: 600;
  color: var(--color-text);
}

.provider-group__badge {
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: var(--text-caption);
  font-weight: 500;
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--color-text);
  font-family: var(--font-display);
  font-size: var(--text-headline);
  font-weight: 600;
}

.section-description {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
}

.state-block--compact {
  min-height: 80px;
  padding: var(--space-4);
  flex-direction: row;
  gap: var(--space-2);
  font-size: var(--text-footnote);
}

.empty-hint {
  color: var(--color-text-tertiary);
}

.credential-card__icon--account {
  color: var(--color-success, #22c55e);
  background: rgba(34, 197, 94, 0.08);
}

.badge {
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  font-size: var(--text-caption);
  font-weight: 400;
}

.badge--active {
  color: var(--color-success, #22c55e);
  background: rgba(34, 197, 94, 0.1);
}

.badge--warning {
  color: var(--color-warning, #f59e0b);
  background: rgba(245, 158, 11, 0.1);
}

.badge--danger {
  color: var(--color-danger);
  background: rgba(239, 68, 68, 0.1);
}

.toggle-dot {
  display: block;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--color-success, #22c55e);
  transition: background var(--duration-fast) var(--ease-out);
}

.toggle-dot--off {
  background: var(--color-text-tertiary);
}
</style>
