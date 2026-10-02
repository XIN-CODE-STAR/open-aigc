<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";

import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

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
      "点击上方按钮登录即梦网页版，然后 F12 → Network → 随便一个请求 → 复制整段 Cookie 粘贴到这里（自动识别 sessionid）；也可以只粘贴 sessionid 的值。",
  },
];

/**
 * 从粘贴内容中容错提取 sessionid：支持整段 Cookie、`Cookie:` 前缀、
 * 换行分隔与纯 token。解析规则与 jimeng-free-api 账号池保持一致，
 * 用户复制什么格式都能识别。
 */
function extractSessionId(raw: string): string {
  const normalized = raw
    .trim()
    .replace(/^\s*cookie\s*:\s*/i, "")
    .replace(/[\r\n]+/g, ";");
  const pairs = new Map<string, string>();
  for (const item of normalized.split(";")) {
    const separator = item.indexOf("=");
    if (separator < 0) continue;
    const key = item.slice(0, separator).trim().toLowerCase();
    const value = item.slice(separator + 1).trim();
    if (key && value) pairs.set(key, value);
  }
  const value =
    pairs.get("sessionid") ||
    pairs.get("sessionid_ss") ||
    pairs.get("sid_tt") ||
    pairs.get("sid_guard")?.split("%7C")[0] ||
    pairs.get("sid_guard")?.split("|")[0] ||
    "";
  if (value) return value;
  // 无键值对时接受纯 token（从接口或他人转发中直接复制的场景）
  if (!normalized.includes("=") && /^[^\s;]+$/.test(normalized)) return normalized;
  return "";
}

function maskSessionId(value: string): string {
  if (value.length <= 8) return `${value.length} 位`;
  return `${value.slice(0, 4)}…${value.slice(-4)}（${value.length} 位）`;
}

const selectedPreset = ref(0);
const sessionCookie = ref("");
const busy = ref(false);

const isEditMode = computed(() => !!props.editing);

const preset = computed(() => ACCOUNT_PRESETS[selectedPreset.value]);

const form = reactive({
  displayName: "",
});

// 实时解析粘贴内容，识别到 sessionid 时给出脱敏预览
const detectedSessionId = computed(() => extractSessionId(sessionCookie.value));

function openLoginPage(): void {
  void invoke("open_file_with_system_viewer", { path: preset.value.loginUrl }).catch((err) => {
    console.error("[account-dialog] 打开登录页失败:", err);
  });
}

/**
 * 打开即梦账号池管理（内嵌 Webview 窗口，承载 jimeng-free-api 的
 * 账号池管理页）：多账号 Cookie 添加/启停/轮询/失败冷却与状态刷新
 * 都在这个页面完成，无需单独开浏览器。代理未运行时回退系统浏览器。
 */
async function openPoolManager(): Promise<void> {
  const poolUrl = "http://127.0.0.1:5100/account-pool/";
  try {
    const probe = await fetch("http://127.0.0.1:5100/ping", {
      signal: AbortSignal.timeout(3000),
    });
    if (!probe.ok) throw new Error(`HTTP ${probe.status}`);
  } catch {
    // 代理未运行：回退到系统浏览器（至少能看到错误页与启动指引）
    await invoke("open_file_with_system_viewer", { path: poolUrl }).catch(() => undefined);
    return;
  }
  try {
    const win = new WebviewWindow("jimeng-pool-manager", {
      url: poolUrl,
      title: "即梦账号池管理",
      width: 1100,
      height: 820,
      center: true,
    });
    win.once("tauri://error", (event) => {
      console.error("[account-dialog] 账号池窗口创建失败:", event);
      void invoke("open_file_with_system_viewer", { path: poolUrl }).catch(() => undefined);
    });
  } catch (err) {
    console.error("[account-dialog] 账号池窗口异常:", err);
    await invoke("open_file_with_system_viewer", { path: poolUrl }).catch(() => undefined);
  }
}

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
    return (
      form.displayName.trim().length > 0 &&
      (sessionCookie.value.trim().length === 0 || !!detectedSessionId.value) &&
      !busy.value
    );
  }
  return !!detectedSessionId.value && !busy.value;
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
    sessionCookie: detectedSessionId.value,
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
          即梦 Cookie / Session ID
          <span v-if="isEditMode" class="field__optional">（留空则不更新）</span>
        </span>
        <textarea
          v-model="sessionCookie"
          class="field__textarea"
          rows="3"
          :placeholder="
            isEditMode
              ? '粘贴新的 Cookie 或 sessionid 以更新...'
              : '粘贴整段 Cookie 或仅 sessionid 值...'
          "
        />
        <p v-if="detectedSessionId" class="field__detected">
          已识别 sessionid：{{ maskSessionId(detectedSessionId) }}
        </p>
        <p v-else-if="sessionCookie.trim()" class="field__detected field__detected--warn">
          未识别到 sessionid，请检查粘贴内容
        </p>
        <p class="field__help">{{ preset.helpText }}</p>
        <div class="quick-links">
          <button class="login-link" type="button" @click="openLoginPage">打开即梦登录页 ↗</button>
          <span class="quick-links__sep">·</span>
          <button class="login-link" type="button" @click="openPoolManager">
            账号池管理（多账号轮询）↗
          </button>
        </div>
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

.field__detected {
  margin: 0;
  color: var(--color-accent);
  font-family: var(--font-mono);
  font-size: var(--text-caption);
}

.field__detected--warn {
  color: var(--color-text-tertiary);
  font-family: inherit;
}

.quick-links {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.quick-links__sep {
  color: var(--color-text-tertiary);
}

.login-link {
  align-self: flex-start;
  padding: 0;
  border: none;
  background: none;
  color: var(--color-accent);
  font-size: var(--text-caption);
  cursor: pointer;
  text-decoration: underline;
}

.login-link:hover {
  opacity: 0.85;
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
  background: var(--color-accent-soft);
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
