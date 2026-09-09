<script setup lang="ts">
/**
 * ToolInvocationPanel：单次工具调用面板。
 *
 * 状态：pending / running / succeeded / failed / skipped
 * 头部展示工具名 + 状态，可展开查看参数/结果/错误的 JSON 详情。
 * Phase 1 拆分目标：只通过 props/emit 通信，不直接调 bridge / store。
 */
import { AlertCircle, CheckCircle2, ChevronDown, FileUp, LoaderCircle, X } from "@lucide/vue";
import type { ToolInvocationRecord } from "../../../bridge/agent";

defineProps<{
  /** 当前工具调用记录。 */
  invocation: ToolInvocationRecord;
  /** 是否已展开（由父组件维护展开集合）。 */
  isExpanded: boolean;
  /** 工具名 → 中文名映射（来自父组件 toolLabel）。 */
  toolLabel: (name: string) => string;
  /** JSON 美化函数。 */
  prettyJson: (raw: string) => string;
}>();

const emit = defineEmits<{
  toggle: [id: string];
}>();

function onToggle(id: string): void {
  emit("toggle", id);
}
</script>

<template>
  <div
    class="invocation"
    :class="[`invocation--${invocation.status}`, { 'is-expanded': isExpanded }]"
  >
    <button
      type="button"
      class="invocation-head"
      :title="isExpanded ? '收起详情' : '展开详情'"
      @click="onToggle(invocation.id)"
    >
      <LoaderCircle
        v-if="invocation.status === 'pending' || invocation.status === 'running'"
        :size="11"
        class="is-spinning"
      />
      <CheckCircle2 v-else-if="invocation.status === 'succeeded'" :size="11" />
      <AlertCircle v-else-if="invocation.status === 'failed'" :size="11" />
      <X v-else :size="11" />
      <span class="invocation-name">{{ toolLabel(invocation.toolName) }}</span>
      <span class="invocation-state">
        <span v-if="invocation.status === 'running'">执行中…</span>
        <span v-else-if="invocation.status === 'pending'">排队中…</span>
        <span v-else-if="invocation.status === 'succeeded'">完成</span>
        <span v-else-if="invocation.status === 'failed'">失败</span>
        <span v-else>已跳过</span>
      </span>
      <ChevronDown :size="10" class="invocation-caret" :class="{ 'is-open': isExpanded }" />
    </button>

    <div v-if="isExpanded" class="invocation-detail">
      <div class="detail-block">
        <div class="detail-label">参数</div>
        <pre class="detail-code">{{ prettyJson(invocation.argumentsJson) }}</pre>
      </div>
      <div v-if="invocation.resultJson" class="detail-block">
        <div class="detail-label">结果</div>
        <pre class="detail-code">{{ prettyJson(invocation.resultJson) }}</pre>
      </div>
      <div v-if="invocation.errorMessage" class="detail-block detail-block--error">
        <div class="detail-label">错误</div>
        <pre class="detail-code">{{ invocation.errorMessage }}</pre>
      </div>
      <div v-if="invocation.generationTaskId" class="detail-link">
        <FileUp :size="10" />
        <span>关联任务：{{ invocation.generationTaskId.slice(0, 8) }}…</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.invocation {
  border: 1px solid var(--color-border-subtle);
  border-radius: 8px;
  background: var(--color-surface-subtle);
  margin-bottom: 6px;
  transition:
    border-color 120ms ease,
    background 120ms ease;
}

.invocation-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  background: transparent;
  border: none;
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
  text-align: left;
}

.invocation-head:hover {
  background: var(--color-surface-hover);
}

.invocation--running {
  border-color: color-mix(in srgb, var(--color-accent) 30%, transparent);
}

.invocation--succeeded {
  border-color: var(--color-success-soft);
}

.invocation--failed {
  border-color: var(--color-danger-soft);
}

.invocation-name {
  flex: 1;
  font-weight: 500;
}

.invocation-state {
  color: var(--color-text-tertiary);
  font-size: 10px;
}

.invocation-caret {
  opacity: 0.6;
  transition: transform 160ms ease;
}

.invocation-caret.is-open {
  transform: rotate(180deg);
}

.invocation-detail {
  padding: 8px 10px 10px;
  border-top: 1px solid var(--color-border-subtle);
}

.detail-block {
  margin-bottom: 8px;
}

.detail-block:last-child {
  margin-bottom: 0;
}

.detail-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-text-tertiary);
  margin-bottom: 3px;
}

.detail-code {
  margin: 0;
  padding: 6px 8px;
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  border-radius: 4px;
  font-size: 10px;
  line-height: 1.5;
  font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  color: var(--color-text);
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
}

.detail-block--error .detail-code {
  color: var(--color-danger);
}

.detail-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: rgba(59, 130, 246, 0.85);
}

/* 旋转动画：scoped 样式无法从父页面穿透到本组件内部元素，
   必须在本组件内自定义，否则 LoaderCircle 静止不转。 */
.is-spinning {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
