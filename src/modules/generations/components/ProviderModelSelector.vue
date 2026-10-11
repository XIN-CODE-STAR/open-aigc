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
import { parseAccountCredits, type ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import {
  isAccountAvailable,
  statusColor,
  statusLabel,
  type ProviderAccountStatus,
} from "../../../bridge/providers";
import { isGenerationSourceFor, type GenerationMediaKind } from "../generationSources";

const props = defineProps<{
  /** API 凭据列表（deepseek、grok 等）。 */
  credentials: CredentialRecord[];
  /** 当前选中的凭据 id。 */
  selectedCredentialId: string | null;
  /** 资源账号列表（jimeng 等）。 */
  resourceAccounts?: ResourceAccountRecord[];
  /** 当前选中的资源账号 id。 */
  selectedAccountId?: string | null;
  /** 当前创作模式（区分对话选择与图片/视频生成来源）。 */
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

/** 当前模式要求的媒体类型；Agent 模式（无 taskType）为 null ⇒ 不过滤。 */
const requiredMediaKind = computed<GenerationMediaKind | null>(() => {
  if (props.taskType === "video_generation") return "video";
  if (props.taskType === "image_generation") return "image";
  return null;
});

/**
 * API 凭据列表。
 *
 * 图片/视频模式下**只列能产出该媒体的来源**：LLM 凭据（zhipu / glm-4.5-air 等）
 * 在 Agent 模式里设置即可，若出现在生成模式下会被选中、把生成任务发给 LLM provider
 * （2026-10-10 用户反馈）。判定规则见 `../generationSources.ts`——它镜像后端
 * `generation_engine.rs` 的 provider 谓词与各 adapter 的 capabilities()。
 */
const filteredCredentials = computed(() =>
  (props.credentials ?? []).filter((credential) =>
    isGenerationSourceFor(credential, requiredMediaKind.value),
  ),
);

/** 资源账号同理：即梦账号图片/视频都支持，但判定仍走同一套规则。 */
const filteredAccounts = computed(() =>
  (props.resourceAccounts ?? []).filter((account) =>
    isGenerationSourceFor(
      { providerName: account.providerId, displayName: account.displayName },
      requiredMediaKind.value,
    ),
  ),
);

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

/** 当前选中项的显示信息。不属于当前模式的来源（如视频模式下选中的 LLM 凭据）按未选中处理。 */
const selectedDisplay = computed(() => {
  const cred = selectedCredential();
  if (cred && filteredCredentials.value.some((c) => c.id === cred.id))
    return {
      name: cred.modelName,
      provider: cred.providerName,
      status: cred.status,
      isAccount: false,
    };
  const acc = selectedAccount();
  if (acc && filteredAccounts.value.some((a) => a.id === acc.id))
    return {
      name: acc.displayName || acc.providerId,
      provider: "即梦",
      status: acc.status,
      isAccount: true,
    };
  return null;
});

/**
 * 账号剩余积分的展示文本。
 *
 * 积分由 `account_health_worker` 每轮从代理 `/token/points` 取回并写进
 * `extra_json`（见 `bridge/resourceAccounts.ts` 的 `parseAccountCredits`）。
 * 没有积分信息（老记录 / 代理不可达 / 非即梦账号）时返回空串，不显示。
 */
function accountCreditsLabel(account: ResourceAccountRecord): string {
  const credits = parseAccountCredits(account);
  return credits ? `剩余 ${credits.total} 积分` : "";
}

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
      return "var(--color-success)";
    case "need_login":
      return "var(--color-warning)";
    case "blocked":
      return "var(--color-danger)";
    default:
      return "var(--color-text-tertiary)";
  }
}

const showStatus = (): boolean => props.showStatusIndicator !== false;

/**
 * 无可用来源时的提示。区分两种情况——「一个都没配」与「配了但没有能出这种媒体的」；
 * 后者若提示「请先配置模型」会让人以为配置丢了（2026-10-10 同类反馈）。
 */
const emptySourceHint = computed(() => {
  if (hasAnySource.value) return "切换模型";
  const hasAnyConfigured =
    (props.credentials?.length ?? 0) > 0 || (props.resourceAccounts?.length ?? 0) > 0;
  if (requiredMediaKind.value && hasAnyConfigured) {
    const media = requiredMediaKind.value === "video" ? "视频" : "图片";
    return `没有可用的生成来源：已配置的模型/账号都不支持生成${media}`;
  }
  return "请先配置模型或连接 AI 账号";
});
</script>

<template>
  <div ref="modelMenuRef" class="model-picker">
    <button
      type="button"
      class="model-btn"
      :disabled="!hasAnySource || disabled"
      :title="emptySourceHint"
      @click="toggleMenu"
    >
      <Sparkles :size="11" />
      <span class="model-btn-label">
        <template v-if="selectedDisplay">
          <span class="model-btn-name">{{ selectedDisplay.name }}</span>
          <span class="model-btn-provider">{{ selectedDisplay.provider }}</span>
        </template>
        <template v-else-if="!hasAnySource">
          {{ requiredMediaKind ? "无可用生成来源" : "未配置模型" }}
        </template>
        <template v-else>选择模型</template>
      </span>
      <span
        v-if="selectedDisplay && showStatus()"
        class="model-status-dot"
        :style="{
          background: selectedDisplay.isAccount
            ? accountStatusColor(selectedDisplay.status ?? '')
            : (statusColor(selectedDisplay.status as ProviderAccountStatus) ??
              'var(--color-text-tertiary)'),
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
          <div class="model-pop-group-label">API Key 模型（多模态可生成）</div>
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
          <div class="model-pop-group-label">AI 账号（平台直连）</div>
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
                        ? 'var(--color-success)'
                        : acc.status === 'need_login'
                          ? 'var(--color-warning)'
                          : 'var(--color-danger)',
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
                <span v-if="accountCreditsLabel(acc)" class="model-pop-credits">
                  · {{ accountCreditsLabel(acc) }}
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
  border: 1px solid var(--color-border);
  border-radius: 999px;
  background: var(--color-fg-soft);
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  max-width: 200px;
}

.model-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  border-color: var(--color-border-strong);
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

/* 生成来源标识：与模型名之间的浅色前缀 */
.model-pop-credits {
  margin-left: 2px;
  color: var(--color-text-tertiary);
  font-size: 10px;
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

.model-pop {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 220px;
  max-width: 280px;
  padding: 4px;
  border-radius: 12px;
  background: var(--material-sheet);
  border: 1px solid var(--color-border);
  box-shadow: var(--shadow-sheet);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

/* 向下弹出的变体（欢迎页输入卡内使用） */
.model-pop--bottom {
  bottom: auto;
  top: calc(100% + 6px);
}

.model-pop-group-label {
  padding: 6px 10px 2px;
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-text-tertiary);
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
  color: var(--color-text-secondary);
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.model-pop-item:hover {
  background: var(--color-surface-hover);
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
  color: var(--color-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-pop-provider {
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.model-pop-check {
  color: var(--color-accent-text);
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
  background: var(--color-warning-soft);
  border: 1px solid color-mix(in oklab, var(--color-warning) 30%, transparent);
  color: var(--color-warning);
  font-size: 10px;
  font-weight: 500;
  cursor: pointer;
  flex-shrink: 0;
  transition: background var(--duration-fast) var(--ease-out);
}

.model-pop-reauth:hover {
  background: color-mix(in oklab, var(--color-warning) 25%, transparent);
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
