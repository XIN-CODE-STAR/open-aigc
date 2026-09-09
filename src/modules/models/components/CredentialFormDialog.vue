<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";

import ModalDialog from "../../../shared/ui/ModalDialog.vue";
import ProviderLogo from "./ProviderLogo.vue";
import type { CredentialRecord } from "../../../bridge/credentials";

const props = defineProps<{
  open: boolean;
  editing: CredentialRecord | null;
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  submit: [
    payload: {
      providerName: string;
      displayName: string;
      baseUrl: string;
      modelName: string;
      apiKey: string;
    },
  ];
}>();

interface ProviderPreset {
  label: string;
  abbr: string;
  providerName: string;
  baseUrl: string;
  defaultModel: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    label: "DeepSeek",
    abbr: "DS",
    providerName: "deepseek",
    baseUrl: "https://api.deepseek.com",
    defaultModel: "deepseek-chat",
  },
  {
    label: "OpenAI",
    abbr: "AI",
    providerName: "openai",
    baseUrl: "https://api.openai.com/v1",
    defaultModel: "gpt-4o",
  },
  {
    label: "Claude",
    abbr: "CL",
    providerName: "anthropic",
    baseUrl: "https://api.anthropic.com",
    defaultModel: "claude-sonnet-4-20250514",
  },
  {
    label: "Gemini",
    abbr: "GE",
    providerName: "gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    defaultModel: "gemini-2.5-flash",
  },
  {
    label: "Kimi (月之暗面)",
    abbr: "K",
    providerName: "moonshot",
    baseUrl: "https://api.moonshot.cn/v1",
    defaultModel: "moonshot-v1-128k",
  },
  {
    label: "智谱 GLM",
    abbr: "Z",
    providerName: "zhipu",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    defaultModel: "glm-4-plus",
  },
  {
    label: "百度千帆",
    abbr: "BD",
    providerName: "qianfan",
    baseUrl: "https://qianfan.baidubce.com/v2",
    defaultModel: "ernie-4.5-turbo-128k",
  },
  {
    label: "阿里百炼",
    abbr: "AL",
    providerName: "dashscope",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    defaultModel: "qwen-max",
  },
  {
    label: "MiniMax",
    abbr: "MM",
    providerName: "minimax",
    baseUrl: "https://api.minimax.chat/v1",
    defaultModel: "MiniMax-M1",
  },
  {
    label: "阶跃星辰",
    abbr: "SF",
    providerName: "stepfun",
    baseUrl: "https://api.stepfun.com/v1",
    defaultModel: "step-2-16k",
  },
  {
    label: "豆包 (火山引擎)",
    abbr: "DB",
    providerName: "doubao",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    defaultModel: "doubao-1-5-pro-256k-250115",
  },
  {
    label: "SiliconFlow",
    abbr: "Si",
    providerName: "siliconflow",
    baseUrl: "https://api.siliconflow.cn/v1",
    defaultModel: "deepseek-ai/DeepSeek-V3",
  },
  {
    label: "OpenRouter",
    abbr: "OR",
    providerName: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    defaultModel: "deepseek/deepseek-chat",
  },
  {
    label: "小米 MiMo",
    abbr: "Mi",
    providerName: "xiaomi",
    baseUrl: "https://api.xiaomimimo.com/v1",
    defaultModel: "mimo-v2.5-pro",
  },
  {
    label: "Nvidia NIM",
    abbr: "NV",
    providerName: "nvidia",
    baseUrl: "https://integrate.api.nvidia.com/v1",
    defaultModel: "deepseek-ai/deepseek-r1",
  },
  {
    label: "Groq",
    abbr: "GQ",
    providerName: "groq",
    baseUrl: "https://api.groq.com/openai/v1",
    defaultModel: "llama-3.3-70b-versatile",
  },
  {
    label: "Mistral",
    abbr: "MS",
    providerName: "mistral",
    baseUrl: "https://api.mistral.ai/v1",
    defaultModel: "mistral-large-latest",
  },
  {
    label: "Cohere",
    abbr: "CO",
    providerName: "cohere",
    baseUrl: "https://api.cohere.com/compatibility/v1",
    defaultModel: "command-r-plus",
  },
  {
    label: "火山 Seedance",
    abbr: "SD",
    providerName: "seedance",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    defaultModel: "seedance-2-0-250601",
  },
  {
    label: "ModelScope",
    abbr: "MS",
    providerName: "modelscope",
    baseUrl: "https://api-inference.modelscope.cn/v1",
    defaultModel: "Qwen/Qwen3-235B-A22B",
  },
];

interface FormState {
  providerName: string;
  displayName: string;
  baseUrl: string;
  modelName: string;
  apiKey: string;
}

const form = reactive<FormState>({
  providerName: "",
  displayName: "",
  baseUrl: "",
  modelName: "",
  apiKey: "",
});

/** step: "grid" = 预设网格选择, "form" = 填写表单 */
const step = ref<"grid" | "form">("grid");
const search = ref("");

const filteredPresets = computed(() => {
  const keyword = search.value.trim().toLowerCase();
  if (!keyword) return PROVIDER_PRESETS;
  return PROVIDER_PRESETS.filter(
    (preset) =>
      preset.label.toLowerCase().includes(keyword) ||
      preset.providerName.toLowerCase().includes(keyword),
  );
});
const selectedPreset = ref<ProviderPreset | null>(null);

const isEdit = computed(() => props.editing !== null);
const title = computed(() => {
  if (isEdit.value) return "编辑模型";
  return step.value === "grid" ? "选择供应商" : "配置模型";
});

const canSubmit = computed(
  () =>
    form.providerName &&
    form.displayName &&
    form.baseUrl &&
    form.modelName &&
    (isEdit.value || form.apiKey),
);

function selectPreset(preset: ProviderPreset): void {
  selectedPreset.value = preset;
  form.providerName = preset.providerName;
  form.displayName = preset.label;
  form.baseUrl = preset.baseUrl;
  form.modelName = preset.defaultModel;
  form.apiKey = "";
  step.value = "form";
}

function openCustom(): void {
  selectedPreset.value = null;
  form.providerName = "";
  form.displayName = "";
  form.baseUrl = "";
  form.modelName = "";
  form.apiKey = "";
  step.value = "form";
}

function backToGrid(): void {
  step.value = "grid";
}

function handleSubmit(): void {
  emit("submit", { ...form });
}

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    if (props.editing) {
      step.value = "form";
      selectedPreset.value = null;
      form.providerName = props.editing.providerName;
      form.displayName = props.editing.displayName;
      form.baseUrl = props.editing.baseUrl;
      form.modelName = props.editing.modelName;
      form.apiKey = "";
    } else {
      step.value = "grid";
      selectedPreset.value = null;
      search.value = "";
      form.providerName = "";
      form.displayName = "";
      form.baseUrl = "";
      form.modelName = "";
      form.apiKey = "";
    }
  },
);
</script>

<template>
  <ModalDialog :open="open" :title="title" :busy="busy" width="wide" @close="emit('close')">
    <!-- Step 1: 预设网格 -->
    <div v-if="step === 'grid' && !isEdit" class="preset-step">
      <div class="preset-toolbar">
        <button class="btn btn--accent" type="button" @click="openCustom">自定义配置</button>
        <input
          v-model.trim="search"
          class="preset-search"
          type="search"
          placeholder="搜索供应商…"
          maxlength="40"
        />
        <span class="preset-count">{{ filteredPresets.length }}/{{ PROVIDER_PRESETS.length }}</span>
      </div>
      <div class="preset-grid">
        <button
          v-for="preset in filteredPresets"
          :key="preset.providerName"
          class="preset-card"
          type="button"
          :title="`${preset.label}（${preset.providerName}）`"
          @click="selectPreset(preset)"
        >
          <ProviderLogo
            class="preset-card__logo"
            :provider="preset.providerName"
            :label="preset.label"
            :size="30"
          />
          <span class="preset-card__body">
            <span class="preset-card__label">{{ preset.label }}</span>
            <span class="preset-card__provider">{{ preset.providerName }}</span>
          </span>
        </button>
        <div v-if="filteredPresets.length === 0" class="preset-empty">
          没有匹配「{{ search }}」的供应商，试试「自定义配置」。
        </div>
      </div>
    </div>

    <!-- Step 2: 表单 -->
    <form v-else class="credential-form" @submit.prevent="handleSubmit">
      <button v-if="!isEdit" class="back-link" type="button" @click="backToGrid">
        ← 返回供应商列表
      </button>

      <div class="form-field">
        <label class="form-label" for="provider-name">供应商标识 *</label>
        <input
          id="provider-name"
          v-model.trim="form.providerName"
          class="form-input"
          type="text"
          placeholder="例如：deepseek、openai、zhipu"
          required
          maxlength="80"
          autofocus
        />
        <span class="form-hint">用于内部路由匹配，建议使用英文小写</span>
      </div>

      <div class="form-field">
        <label class="form-label" for="display-name">显示名称 *</label>
        <input
          id="display-name"
          v-model.trim="form.displayName"
          class="form-input"
          type="text"
          placeholder="例如：DeepSeek Chat"
          required
          maxlength="120"
        />
      </div>

      <div class="form-field">
        <label class="form-label" for="base-url">服务地址 *</label>
        <input
          id="base-url"
          v-model.trim="form.baseUrl"
          class="form-input"
          type="url"
          placeholder="https://api.deepseek.com"
          required
          maxlength="500"
        />
      </div>

      <div class="form-field">
        <label class="form-label" for="model-name">模型名称 *</label>
        <input
          id="model-name"
          v-model.trim="form.modelName"
          class="form-input"
          type="text"
          placeholder="例如：deepseek-chat、gpt-4o"
          required
          maxlength="120"
        />
      </div>

      <div class="form-field">
        <label class="form-label" for="api-key">
          API 密钥 {{ isEdit ? "（留空保留原有）" : "*" }}
        </label>
        <input
          id="api-key"
          v-model.trim="form.apiKey"
          class="form-input form-input--mono"
          type="password"
          :placeholder="isEdit ? '留空则不修改密钥' : 'sk-...'"
          :required="!isEdit"
          maxlength="500"
        />
        <span class="form-hint">密钥存储在系统密钥链中，不会明文写入数据库</span>
      </div>
    </form>

    <template #footer>
      <button class="btn btn--secondary" type="button" :disabled="busy" @click="emit('close')">
        取消
      </button>
      <button
        v-if="step === 'form' || isEdit"
        class="btn btn--primary"
        type="button"
        :disabled="busy || !canSubmit"
        @click="handleSubmit"
      >
        {{ busy ? "保存中..." : isEdit ? "保存修改" : "添加模型" }}
      </button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.preset-step {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: var(--space-4) var(--space-5);
}

.preset-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.preset-search {
  flex: 1;
  max-width: 240px;
  height: 32px;
  padding: 0 var(--space-3);
  margin-left: auto;
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  font-size: var(--text-footnote);
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.preset-search:focus {
  border-color: var(--color-accent);
}

.preset-count {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  font-variant-numeric: tabular-nums;
}

.preset-empty {
  grid-column: 1 / -1;
  padding: var(--space-5);
  color: var(--color-text-tertiary);
  font-size: var(--text-footnote);
  text-align: center;
}

.btn--accent {
  height: 32px;
  padding: 0 var(--space-4);
  color: #fff;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  font-size: var(--text-subhead);
  font-weight: 500;
  cursor: pointer;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.btn--accent:hover {
  opacity: 0.9;
}

.preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--space-2);
  max-height: 400px;
  overflow-y: auto;
  padding-right: var(--space-1);
}

.preset-card {
  display: flex;
  padding: var(--space-2) var(--space-3);
  align-items: center;
  gap: var(--space-2);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  cursor: pointer;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    background var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.preset-card:hover {
  border-color: var(--color-accent);
  background: var(--color-surface-hover);
  transform: translateY(-1px);
}

.preset-card__body {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.preset-card__label {
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-footnote);
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preset-card__provider {
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.credential-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: var(--space-5);
}

.back-link {
  padding: 0;
  color: var(--color-accent);
  border: none;
  background: none;
  font-size: var(--text-footnote);
  cursor: pointer;
  align-self: flex-start;
}

.back-link:hover {
  text-decoration: underline;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.form-label {
  color: var(--color-text);
  font-size: var(--text-subhead);
  font-weight: 500;
  line-height: 20px;
}

.form-input {
  height: 38px;
  padding: 0 var(--space-3);
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  font-size: var(--text-subhead);
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.form-input:focus {
  border-color: var(--color-accent);
}

.form-input--mono {
  font-family: var(--font-mono, monospace);
  font-size: var(--text-footnote);
}

.form-hint {
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  line-height: 16px;
}

.btn {
  height: 36px;
  padding: 0 var(--space-4);
  border: none;
  border-radius: var(--radius-control);
  font-size: var(--text-subhead);
  font-weight: 500;
  cursor: pointer;
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
</style>
