<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";

import type { ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import ModalDialog from "../../../shared/ui/ModalDialog.vue";

const props = defineProps<{
  open: boolean;
  /** 传入已有账号时为编辑模式，null 为新建模式。 */
  editing?: ResourceAccountRecord | null;
}>();

const emit = defineEmits<{
  close: [];
  submit: [payload: AccountConnectPayload];
}>();

interface AccountConnectPayload {
  providerName: string;
  displayName: string;
  baseUrl: string;
  modelName: string;
  sessionCookie: string;
}

const ACCOUNT_PRESETS = [
  {
    label: "即梦AI",
    providerName: "jimeng",
    baseUrl: "https://jimeng.jianying.com",
    modelName: "jimeng-4.5",
    loginUrl: "https://jimeng.jianying.com/ai-tool/image/generate",
    helpText:
      "登录即梦网页版后，按 F12 打开开发者工具 → Application → Cookies → 复制 sessionid 的值。",
  },
];

const selectedPreset = ref(0);
const sessionCookie = ref("");
const busy = ref(false);

const isEditMode = computed(() => !!props.editing);

const preset = computed(() => ACCOUNT_PRESETS[selectedPreset.value]);

const form = reactive({
  displayName: "",
});

// 编辑模式下预填表单
watch(
  () => props.editing,
  (account) => {
    if (account) {
      form.displayName = account.displayName;
      sessionCookie.value = "";
      // 定位到对应预设
      const idx = ACCOUNT_PRESETS.findIndex((p) => p.providerName === account.providerId);
      if (idx >= 0) selectedPreset.value = idx;
    } else {
      form.displayName = "";
      sessionCookie.value = "";
      selectedPreset.value = 0;
    }
  },
  { immediate: true },
);

const canSubmit = computed(() => {
  if (isEditMode.value) {
    // 编辑模式：显示名称非空即可（Session 可选更新）
    return form.displayName.trim().length > 0 && !busy.value;
  }
  return sessionCookie.value.trim().length > 0 && !busy.value;
});

function handleSubmit(): void {
  if (!canSubmit.value) return;
  const p = preset.value;
  const displayName = form.displayName.trim() || `${p.label}账号${Date.now().toString().slice(-4)}`;
  emit("submit", {
    providerName: p.providerName,
    displayName: displayName,
    baseUrl: p.baseUrl,
    modelName: p.modelName,
    sessionCookie: sessionCookie.value.trim(),
  });
}

function handleClose(): void {
  emit("close");
}
</script>

<template>
  <ModalDialog
    :open="props.open"
    :title="isEditMode ? '更新账号' : '连接AI账号'"
    :description="
      isEditMode
        ? '更新账号信息。Session 过期时可粘贴新的 Session ID。'
        : '登录你的AI平台账号，OPEN AIGC 将使用你的会员权益进行创作。'
    "
    :busy="busy"
    @close="handleClose"
  >
    <div class="account-form">
      <label v-if="!isEditMode" class="field">
        <span class="field__label">选择平台</span>
        <select v-model="selectedPreset" class="field__input">
          <option v-for="(p, idx) in ACCOUNT_PRESETS" :key="p.providerName" :value="idx">
            {{ p.label }}
          </option>
        </select>
      </label>

      <label class="field">
        <span class="field__label">显示名称</span>
        <input
          v-model="form.displayName"
          class="field__input"
          type="text"
          :placeholder="`${preset.label}账号`"
          maxlength="60"
        />
      </label>

      <div class="field">
        <span class="field__label">
          Session ID
          <span v-if="isEditMode" class="field__optional">（留空则不更新）</span>
        </span>
        <textarea
          v-model="sessionCookie"
          class="field__textarea"
          rows="3"
          :placeholder="isEditMode ? '粘贴新的 Session ID 以更新...' : '粘贴 sessionid 值...'"
        />
        <p v-if="!isEditMode" class="field__help">{{ preset.helpText }}</p>
      </div>

      <div class="security-note">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        <span>Session 将加密存储在系统钥匙串中，不会明文保存在数据库或上传到任何服务器。</span>
      </div>
    </div>

    <template #footer>
      <button class="btn btn--ghost" type="button" @click="handleClose">取消</button>
      <button class="btn btn--primary" type="button" :disabled="!canSubmit" @click="handleSubmit">
        {{ isEditMode ? "保存更新" : "连接账号" }}
      </button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.account-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field__label {
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  font-weight: 500;
}

.field__input {
  height: var(--control-height);
  padding: 0 var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: var(--text-subhead);
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.field__input:focus {
  border-color: var(--color-accent);
}

.field__textarea {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-family: var(--font-mono);
  font-size: var(--text-footnote);
  line-height: 1.5;
  resize: vertical;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.field__textarea:focus {
  border-color: var(--color-accent);
}

.field__help {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  line-height: 1.4;
}

.field__optional {
  color: var(--color-text-tertiary);
  font-weight: 400;
}

.security-note {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-3);
  border-radius: var(--radius-control);
  background: var(--color-accent-soft, rgba(99, 102, 241, 0.08));
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  line-height: 1.4;
}

.security-note svg {
  flex: 0 0 14px;
  margin-top: 1px;
  color: var(--color-accent);
}
</style>
