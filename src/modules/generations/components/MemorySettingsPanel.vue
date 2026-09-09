<script setup lang="ts">
/**
 * MemorySettingsPanel：记忆服务设置面板。
 *
 * 管理 EverOS 记忆服务的配置和状态。
 */
import { ref } from "vue";
import { Brain, Database, LoaderCircle, RefreshCw, Zap } from "@lucide/vue";

interface MemoryConfig {
  enabled: boolean;
  searchMethod: "keyword" | "vector" | "hybrid";
  maxContextMessages: number;
  enableLongTerm: boolean;
}

const props = defineProps<{
  config: MemoryConfig;
  status: "active" | "inactive" | "loading";
  errorMessage?: string;
}>();

const emit = defineEmits<{
  "update:config": [config: MemoryConfig];
  "test-connection": [];
  "clear-memory": [];
}>();

const localConfig = ref({ ...props.config });

function saveConfig(): void {
  emit("update:config", { ...localConfig.value });
}

function toggleEnabled(): void {
  localConfig.value.enabled = !localConfig.value.enabled;
  saveConfig();
}

function setSearchMethod(method: MemoryConfig["searchMethod"]): void {
  localConfig.value.searchMethod = method;
  saveConfig();
}
</script>

<template>
  <div class="memory-settings">
    <div class="memory-header">
      <Brain :size="18" />
      <h3 class="memory-title">记忆服务</h3>
      <div class="memory-status" :class="`memory-status--${status}`">
        <span class="memory-status-dot" />
        <span>{{
          status === "active" ? "运行中" : status === "loading" ? "加载中…" : "未连接"
        }}</span>
      </div>
    </div>

    <p class="memory-description">
      记忆服务让 Agent 能够记住您的偏好和历史对话，在后续创作中提供更个性化的建议。
    </p>

    <!-- 启用/禁用开关 -->
    <div class="memory-toggle">
      <label class="toggle-label">
        <span class="toggle-text">启用长期记忆</span>
        <button
          type="button"
          class="toggle-switch"
          :class="{ 'is-on': localConfig.enabled }"
          @click="toggleEnabled"
        >
          <span class="toggle-knob" />
        </button>
      </label>
      <p class="toggle-hint">
        {{ localConfig.enabled ? "Agent 会记住您的创作偏好和历史" : "仅使用当前对话的上下文" }}
      </p>
    </div>

    <!-- 搜索方法 -->
    <div v-if="localConfig.enabled" class="memory-option">
      <label class="option-label">检索方式</label>
      <div class="option-buttons">
        <button
          type="button"
          class="option-btn"
          :class="{ 'is-active': localConfig.searchMethod === 'keyword' }"
          @click="setSearchMethod('keyword')"
        >
          <Zap :size="12" />
          关键词
        </button>
        <button
          type="button"
          class="option-btn"
          :class="{ 'is-active': localConfig.searchMethod === 'vector' }"
          @click="setSearchMethod('vector')"
        >
          <Brain :size="12" />
          语义
        </button>
        <button
          type="button"
          class="option-btn"
          :class="{ 'is-active': localConfig.searchMethod === 'hybrid' }"
          @click="setSearchMethod('hybrid')"
        >
          <Database :size="12" />
          混合
        </button>
      </div>
      <p class="option-hint">
        {{
          localConfig.searchMethod === "keyword"
            ? "基于关键词匹配，速度快"
            : localConfig.searchMethod === "vector"
              ? "基于语义相似度，需要 Embedding API"
              : "结合关键词和语义，效果最佳"
        }}
      </p>
    </div>

    <!-- 上下文窗口 -->
    <div v-if="localConfig.enabled" class="memory-option">
      <label class="option-label">上下文窗口</label>
      <div class="option-slider">
        <input
          v-model.number="localConfig.maxContextMessages"
          type="range"
          min="10"
          max="50"
          step="5"
          class="slider-input"
          @change="saveConfig"
        />
        <span class="slider-value">{{ localConfig.maxContextMessages }} 条</span>
      </div>
      <p class="option-hint">Agent 单次对话中保留的历史消息数量</p>
    </div>

    <!-- 操作按钮 -->
    <div class="memory-actions">
      <button
        type="button"
        class="memory-btn"
        :disabled="status === 'loading'"
        @click="emit('test-connection')"
      >
        <LoaderCircle v-if="status === 'loading'" :size="14" class="is-spinning" />
        <RefreshCw v-else :size="14" />
        测试连接
      </button>
      <button type="button" class="memory-btn memory-btn--danger" @click="emit('clear-memory')">
        清除记忆
      </button>
    </div>

    <!-- 错误信息 -->
    <p v-if="errorMessage" class="memory-error">
      {{ errorMessage }}
    </p>
  </div>
</template>

<style scoped>
.memory-settings {
  padding: 20px;
}

.memory-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  color: var(--color-text);
}

.memory-title {
  flex: 1;
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}

.memory-status {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: var(--radius-pill);
  font-size: 11px;
  font-weight: 500;
}

.memory-status--active {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.memory-status--inactive {
  background: var(--color-surface-subtle);
  color: var(--color-text-tertiary);
}

.memory-status--loading {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.memory-status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.memory-description {
  margin: 0 0 20px;
  font-size: 13px;
  color: var(--color-text-secondary);
  line-height: 1.5;
}

/* —— 开关 —— */
.memory-toggle {
  margin-bottom: 20px;
  padding: 14px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

.toggle-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
}

.toggle-text {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-text);
}

.toggle-switch {
  position: relative;
  width: 44px;
  height: 24px;
  border: none;
  border-radius: 12px;
  background: var(--color-surface-hover);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.toggle-switch.is-on {
  background: var(--color-accent);
}

.toggle-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: white;
  transition: transform var(--duration-fast) var(--ease-out);
}

.toggle-switch.is-on .toggle-knob {
  transform: translateX(20px);
}

.toggle-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--color-text-tertiary);
}

/* —— 选项 —— */
.memory-option {
  margin-bottom: 20px;
}

.option-label {
  display: block;
  margin-bottom: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.option-buttons {
  display: flex;
  gap: 8px;
}

.option-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 14px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.option-btn:hover {
  border-color: var(--color-border);
  color: var(--color-text);
}

.option-btn.is-active {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.option-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--color-text-tertiary);
}

/* —— 滑块 —— */
.option-slider {
  display: flex;
  align-items: center;
  gap: 12px;
}

.slider-input {
  flex: 1;
  height: 4px;
  border-radius: 2px;
  background: var(--color-surface-hover);
  outline: none;
  -webkit-appearance: none;
}

.slider-input::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--color-accent);
  cursor: pointer;
}

.slider-value {
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text);
  min-width: 50px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

/* —— 按钮 —— */
.memory-actions {
  display: flex;
  gap: 10px;
  margin-top: 24px;
}

.memory-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.memory-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.memory-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.memory-btn--danger {
  color: var(--color-danger);
  border-color: var(--color-danger-soft);
}

.memory-btn--danger:hover {
  background: var(--color-danger-soft);
}

/* —— 错误 —— */
.memory-error {
  margin: 12px 0 0;
  padding: 10px 14px;
  border-radius: var(--radius-control);
  background: var(--color-danger-soft);
  color: var(--color-danger);
  font-size: 13px;
}

/* —— 动画 —— */
.is-spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
