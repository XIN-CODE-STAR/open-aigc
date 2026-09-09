<script setup lang="ts">
/**
 * ProviderModelSelector：模型下拉选择器。
 *
 * 支持两种模型来源：
 * - API 凭据（provider_credentials）：deepseek、grok 等
 * - AI 账号（resource_accounts）：jimeng 等 session-cookie 类型
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { CheckCircle2, ChevronDown, Sparkles } from "@lucide/vue";

import type { CredentialRecord } from "../../../bridge/credentials";
import type { ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import {
  isAccountAvailable,
  statusColor,
  statusLabel,
  type ProviderAccountStatus,
} from "../../../bridge/providers";

const props = defineProps<{
  /** API 凭据列表（deepseek、grok 等）。 */
  credentials: CredentialRecord[];
  /** 当前选中的凭据 id。 */
  selectedCredentialId: string | null;
  /** 资源账号列表（jimeng 等）。 */
  resourceAccounts?: ResourceAccountRecord[];
  /** 当前选中的资源账号 id。 */
  selectedAccountId?: string | null;
  /** 当前创作模式（用于过滤模型）。 */
  taskType?: "image_generation" | "video_generation";
  disabled?: boolean;
  showStatusIndicator?: boolean;
}>();

const emit = defineEmits<{
  select: [credentialId: string];
  "select-account": [accountId: string];
  "request-reauth": [credentialId: string];
}>();

const modelMenuOpen = ref(false);
const modelMenuRef = ref<HTMLElement | null>(null);

const hasAnySource = computed(
  () => (filteredCredentials.value?.length ?? 0) > 0 || (filteredAccounts.value?.length ?? 0) > 0,
);

/** 根据 taskType 过滤 API 凭据。 */
const filteredCredentials = computed(() => {
  const creds = props.credentials ?? [];
  if (!props.taskType) return creds;
  if (props.taskType === "video_generation") {
    // 视频模式：只显示包含 "video" 或 "seedance" 的模型
    return creds.filter((c) => /video|seedance/i.test(c.modelName));
  }
  // 图片模式：排除纯视频模型
  return creds.filter((c) => !/video|seedance/i.test(c.modelName));
});

/** 根据 taskType 过滤资源账号。jimeng 同时支持图片和视频。 */
const filteredAccounts = computed(() => {
  const accounts = props.resourceAccounts ?? [];
  if (!props.taskType) return accounts;
  // jimeng 支持所有模式，直接返回
  return accounts;
});

function toggleMenu(): void {
  if (props.disabled) return;
  if (!hasAnySource.value) return;
  modelMenuOpen.value = !modelMenuOpen.value;
}

function pick(id: string): void {
  emit("select", id);
  modelMenuOpen.value = false;
}

function pickAccount(id: string): void {
  emit("select-account", id);
  modelMenuOpen.value = false;
}

function reauth(id: string, event: MouseEvent): void {
  event.stopPropagation();
  emit("request-reauth", id);
}

function handleDocumentClick(event: MouseEvent): void {
  if (!modelMenuOpen.value) return;
  const target = event.target as Node | null;
  if (!target) return;
  if (modelMenuRef.value && modelMenuRef.value.contains(target)) return;
  modelMenuOpen.value = false;
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && modelMenuOpen.value) {
    modelMenuOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener("click", handleDocumentClick);
  document.addEventListener("keydown", handleKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleDocumentClick);
  document.removeEventListener("keydown", handleKeydown);
});

watch(
  () => hasAnySource.value,
  (ok) => {
    if (!ok) modelMenuOpen.value = false;
  },
);

const selectedCredential = (): CredentialRecord | null => {
  const id = props.selectedCredentialId;
  if (!id) return null;
  return props.credentials.find((c) => c.id === id) ?? null;
};

const selectedAccount = (): ResourceAccountRecord | null => {
  const id = props.selectedAccountId;
  if (!id) return null;
  return props.resourceAccounts?.find((a) => a.id === id) ?? null;
};

/** 当前选中项的显示信息。 */
const selectedDisplay = computed(() => {
  const cred = selectedCredential();
  if (cred)
    return {
      name: cred.modelName,
      provider: cred.providerName,
      status: cred.status,
      isAccount: false,
    };
  const acc = selectedAccount();
  if (acc)
    return {
      name: acc.displayName || acc.providerId,
      provider: "即梦",
      status: acc.status,
      isAccount: true,
    };
  return null;
});

/** 资源账号状态显示。 */
function accountStatusLabel(status: string): string {
  switch (status) {
    case "active":
      return "正常";
    case "need_login":
      return "需登录";
    case "blocked":
      return "已封禁";
    default:
      return status;
  }
}

function accountStatusColor(status: string): string {
  switch (status) {
    case "active":
      return "#22c55e";
    case "need_login":
      return "#ff9f40";
    case "blocked":
      return "#ef4444";
    default:
      return "#94a3b8";
  }
}

const showStatus = (): boolean => props.showStatusIndicator !== false;
</script>

<template>
  <div ref="modelMenuRef" class="model-picker">
    <button
      type="button"
      class="model-btn"
      :disabled="!hasAnySource || disabled"
      :title="!hasAnySource ? '请先配置模型或连接 AI 账号' : '切换模型'"
      @click="toggleMenu"
    >
      <Sparkles :size="11" />
      <span class="model-btn-label">
        <template v-if="selectedDisplay">
          <span class="model-btn-name">{{ selectedDisplay.name }}</span>
          <span class="model-btn-provider">{{ selectedDisplay.provider }}</span>
        </template>
        <template v-else-if="!hasAnySource">未配置模型</template>
        <template v-else>选择模型</template>
      </span>
      <span
        v-if="selectedDisplay && showStatus()"
        class="model-status-dot"
        :style="{
          background: selectedDisplay.isAccount
            ? accountStatusColor(selectedDisplay.status ?? '')
            : (statusColor(selectedDisplay.status as ProviderAccountStatus) ?? '#94a3b8'),
        }"
        :title="`状态：${selectedDisplay.isAccount ? accountStatusLabel(selectedDisplay.status ?? '') : statusLabel(selectedDisplay.status as ProviderAccountStatus)}`"
        aria-hidden="true"
      />
      <ChevronDown
        v-if="hasAnySource"
        :size="10"
        class="model-btn-caret"
        :class="{ 'is-open': modelMenuOpen }"
      />
    </button>
    <Transition name="menu">
      <div v-if="modelMenuOpen && hasAnySource" class="model-pop" role="listbox">
        <!-- API 凭据分组 -->
        <template v-if="filteredCredentials.length > 0">
          <div class="model-pop-group-label">API 模型</div>
          <button
            v-for="cred in filteredCredentials"
            :key="cred.id"
            type="button"
            class="model-pop-item"
            :class="{
              'is-selected': cred.id === selectedCredentialId,
              'is-unavailable': !isAccountAvailable(cred.status),
            }"
            role="option"
            :aria-selected="cred.id === selectedCredentialId"
            @click="pick(cred.id)"
          >
            <div class="model-pop-info">
              <span class="model-pop-name">{{ cred.modelName }}</span>
              <span class="model-pop-provider">
                {{ cred.providerName }}
                <span
                  v-if="showStatus()"
                  class="model-pop-status"
                  :style="{ color: statusColor(cred.status) }"
                >
                  · {{ statusLabel(cred.status) }}
                </span>
              </span>
            </div>
            <CheckCircle2
              v-if="cred.id === selectedCredentialId"
              :size="12"
              class="model-pop-check"
            />
            <button
              v-if="showStatus() && !isAccountAvailable(cred.status)"
              type="button"
              class="model-pop-reauth"
              title="重新登录 / 切换账号"
              @click="reauth(cred.id, $event)"
            >
              重登
            </button>
          </button>
        </template>

        <!-- 资源账号分组（jimeng 等） -->
        <template v-if="filteredAccounts.length > 0">
          <div class="model-pop-group-label">AI 账号</div>
          <button
            v-for="acc in filteredAccounts"
            :key="acc.id"
            type="button"
            class="model-pop-item"
            :class="{
              'is-selected': acc.id === selectedAccountId,
              'is-unavailable': acc.status === 'need_login' || acc.status === 'blocked',
            }"
            role="option"
            :aria-selected="acc.id === selectedAccountId"
            @click="pickAccount(acc.id)"
          >
            <div class="model-pop-info">
              <span class="model-pop-name">{{ acc.displayName || acc.providerId }}</span>
              <span class="model-pop-provider">
                即梦
                <span
                  v-if="showStatus()"
                  class="model-pop-status"
                  :style="{
                    color:
                      acc.status === 'active'
                        ? '#22c55e'
                        : acc.status === 'need_login'
                          ? '#ff9f40'
                          : '#ef4444',
                  }"
                >
                  ·
                  {{
                    acc.status === "active"
                      ? "正常"
                      : acc.status === "need_login"
                        ? "需登录"
                        : acc.status
                  }}
                </span>
              </span>
            </div>
            <CheckCircle2 v-if="acc.id === selectedAccountId" :size="12" class="model-pop-check" />
          </button>
        </template>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.model-picker {
  position: relative;
  margin-left: 4px;
  min-width: 0;
}

.model-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  max-width: 200px;
}

.model-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

.model-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.model-btn-label {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-btn-name {
  font-weight: 500;
}

.model-btn-provider {
  font-size: 11px;
  color: var(--color-text-tertiary);
  font-weight: 400;
}

.model-btn-caret {
  opacity: 0.6;
  transition: transform var(--duration-fast) var(--ease-out);
  flex-shrink: 0;
}

.model-btn-caret.is-open {
  transform: rotate(180deg);
}

:root:not([data-theme="dark"]) .model-btn {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .model-btn:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.06);
  border-color: rgba(0, 0, 0, 0.2);
}

.model-pop {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 220px;
  max-width: 280px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .model-pop {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.model-pop-group-label {
  padding: 6px 10px 2px;
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: rgba(255, 255, 255, 0.3);
}

:root:not([data-theme="dark"]) .model-pop-group-label {
  color: rgba(0, 0, 0, 0.35);
}

.model-pop-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.model-pop-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.model-pop-item.is-selected {
  background: var(--color-accent-soft);
}

.model-pop-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1;
}

.model-pop-name {
  font-size: 12px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.95);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-pop-provider {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.45);
}

.model-pop-check {
  color: var(--color-accent);
  flex-shrink: 0;
}

.model-status-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.18) inset;
}

.model-pop-status {
  font-size: 10px;
  margin-left: 2px;
  font-weight: 500;
}

.model-pop-item.is-unavailable {
  opacity: 0.65;
}

.model-pop-reauth {
  display: inline-flex;
  align-items: center;
  padding: 3px 8px;
  margin-left: 6px;
  border-radius: 4px;
  background: rgba(255, 159, 64, 0.15);
  border: 1px solid rgba(255, 159, 64, 0.3);
  color: #ff9f40;
  font-size: 10px;
  font-weight: 500;
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease-out);
}

.model-pop-reauth:hover {
  background: rgba(255, 159, 64, 0.25);
}

:root:not([data-theme="dark"]) .model-pop-item {
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .model-pop-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

:root:not([data-theme="dark"]) .model-pop-name {
  color: rgba(0, 0, 0, 0.95);
}

:root:not([data-theme="dark"]) .model-pop-provider {
  color: rgba(60, 60, 67, 0.55);
}

.menu-enter-active,
.menu-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(4px);
}

@media (prefers-reduced-motion: reduce) {
  .menu-enter-active,
  .menu-leave-active {
    transition-duration: 0.01ms !important;
  }
}
</style>
