<script setup lang="ts">
/**
 * EditFeedbackPanel：用户反馈提交组件。
 *
 * 支持自然语言反馈输入、客户端快速关键词匹配、
 * 反馈历史查看和修改计划预览。
 */
import { ref, computed, watch } from "vue";
import { Send, Sparkles, CheckCircle, AlertTriangle, Clock, LoaderCircle } from "@lucide/vue";

import type { EditRequestRecord, EditPlanRecord } from "../../../bridge/edit";
import {
  matchFeedbackQuickHint,
  editOperationTypeLabel,
  editRequestStatusLabel,
  editPlanStatusLabel,
  type FeedbackQuickHint,
} from "../../../bridge/edit";

const props = defineProps<{
  projectId: string;
  contextType?: string;
  contextRefId?: string;
  sourceReviewId?: string;
  isSubmitting?: boolean;
  recentRequests?: EditRequestRecord[];
  currentPlan?: EditPlanRecord | null;
}>();

const emit = defineEmits<{
  "submit-feedback": [feedback: string, contextType: string, contextRefId?: string];
  "apply-plan": [planId: string];
  "skip-request": [requestId: string];
}>();

// ═══════════════════════════════════════════════════
// 状态
// ═══════════════════════════════════════════════════

const feedbackText = ref("");
const quickHint = ref<FeedbackQuickHint | null>(null);
const showHistory = ref(false);

// ═══════════════════════════════════════════════════
// 计算属性
// ═══════════════════════════════════════════════════

const canSubmit = computed(() => feedbackText.value.trim().length > 0 && !props.isSubmitting);

const charCount = computed(() => feedbackText.value.length);
const maxChars = 2000;

// ═══════════════════════════════════════════════════
// 监听：实时匹配关键词
// ═══════════════════════════════════════════════════

watch(feedbackText, (text) => {
  if (text.trim().length >= 2) {
    quickHint.value = matchFeedbackQuickHint(text);
  } else {
    quickHint.value = null;
  }
});

// ═══════════════════════════════════════════════════
// 方法
// ═══════════════════════════════════════════════════

function handleSubmit(): void {
  if (!canSubmit.value) return;
  emit(
    "submit-feedback",
    feedbackText.value.trim(),
    props.contextType ?? "generation_result",
    props.contextRefId,
  );
  feedbackText.value = "";
  quickHint.value = null;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    handleSubmit();
  }
}

function requestStatusLabel(status: EditRequestRecord["status"]): string {
  return editRequestStatusLabel(status);
}

function statusIcon(status: string) {
  switch (status) {
    case "plan_ready":
      return CheckCircle;
    case "analyzing":
    case "received":
      return Clock;
    case "applied":
      return CheckCircle;
    case "rejected":
      return AlertTriangle;
    case "ambiguous":
      return AlertTriangle;
    default:
      return Clock;
  }
}

function statusColor(status: string): string {
  switch (status) {
    case "plan_ready":
      return "var(--color-success)";
    case "applied":
      return "var(--color-info)";
    case "analyzing":
    case "received":
      return "var(--color-text-secondary)";
    case "rejected":
      return "var(--color-danger)";
    case "ambiguous":
      return "var(--color-warning)";
    default:
      return "var(--color-text-secondary)";
  }
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}
</script>

<template>
  <div class="edit-feedback-panel">
    <!-- 头部 -->
    <div class="edit-panel-header">
      <h3 class="edit-panel-title">
        <Sparkles :size="16" />
        <span>修改反馈</span>
      </h3>
      <button
        v-if="recentRequests && recentRequests.length > 0"
        type="button"
        class="edit-history-toggle"
        @click="showHistory = !showHistory"
      >
        {{ showHistory ? "收起历史" : "查看历史" }}
        <span class="edit-history-count">{{ recentRequests.length }}</span>
      </button>
    </div>

    <!-- 快速提示 -->
    <Transition name="fade">
      <div v-if="quickHint" class="edit-quick-hint">
        <div class="edit-quick-hint__header">
          <Sparkles :size="12" />
          <span>智能提示</span>
        </div>
        <p class="edit-quick-hint__label">{{ quickHint.label }}</p>
        <div v-if="quickHint.promptAdd.length > 0" class="edit-quick-hint__tags">
          <span v-for="tag in quickHint.promptAdd" :key="tag" class="edit-tag edit-tag--add">
            + {{ tag }}
          </span>
        </div>
        <div v-if="quickHint.promptRemove.length > 0" class="edit-quick-hint__tags">
          <span v-for="tag in quickHint.promptRemove" :key="tag" class="edit-tag edit-tag--remove">
            - {{ tag }}
          </span>
        </div>
      </div>
    </Transition>

    <!-- 输入区域 -->
    <div class="edit-input-area">
      <textarea
        v-model="feedbackText"
        class="edit-textarea"
        placeholder="输入修改反馈，例如：&#10;• 太假了，增加真实感&#10;• 不够大气，画面太小了&#10;• 色调太冷了，暖一点&#10;• 人物更像创业者，不要像模特"
        :maxlength="maxChars"
        rows="4"
        @keydown="onKeydown"
      />
      <div class="edit-input-footer">
        <span class="edit-char-count" :class="{ 'is-limit': charCount > maxChars * 0.9 }">
          {{ charCount }}/{{ maxChars }}
        </span>
        <button type="button" class="edit-submit-btn" :disabled="!canSubmit" @click="handleSubmit">
          <LoaderCircle v-if="isSubmitting" :size="14" class="is-spinning" />
          <Send v-else :size="14" />
          <span>{{ isSubmitting ? "分析中..." : "提交反馈" }}</span>
        </button>
      </div>
    </div>

    <!-- 当前修改计划 -->
    <Transition name="fade">
      <div v-if="currentPlan" class="edit-plan">
        <div class="edit-plan__header">
          <h4 class="edit-plan__title">修改计划</h4>
          <span class="edit-plan__status" :class="`edit-plan__status--${currentPlan.status}`">
            {{ editPlanStatusLabel(currentPlan.status) }}
          </span>
        </div>
        <p class="edit-plan__summary">{{ currentPlan.planSummary }}</p>
        <div class="edit-plan__meta">
          <span class="edit-plan__op">
            操作类型: {{ editOperationTypeLabel(currentPlan.operationType) }}
          </span>
          <span v-if="currentPlan.requiresRegeneration" class="edit-plan__regen">
            需要重新生成
          </span>
        </div>
        <div v-if="currentPlan.status === 'ready'" class="edit-plan__actions">
          <button
            type="button"
            class="edit-plan-btn edit-plan-btn--apply"
            @click="emit('apply-plan', currentPlan.id)"
          >
            应用修改
          </button>
          <button
            type="button"
            class="edit-plan-btn edit-plan-btn--skip"
            @click="emit('skip-request', currentPlan.editRequestId)"
          >
            跳过
          </button>
        </div>
      </div>
    </Transition>

    <!-- 历史记录 -->
    <Transition name="slide">
      <div v-if="showHistory && recentRequests" class="edit-history">
        <div v-for="request in recentRequests" :key="request.id" class="edit-history-item">
          <div class="edit-history-item__header">
            <component
              :is="statusIcon(request.status)"
              :size="14"
              :style="{ color: statusColor(request.status) }"
            />
            <span class="edit-history-item__status" :style="{ color: statusColor(request.status) }">
              {{ requestStatusLabel(request.status) }}
            </span>
            <span class="edit-history-item__time">
              {{ formatTime(request.createdAt) }}
            </span>
          </div>
          <p class="edit-history-item__feedback">{{ request.feedbackText }}</p>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.edit-feedback-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}

/* 头部 */
.edit-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.edit-panel-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
}

.edit-history-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.edit-history-toggle:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

.edit-history-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 50%;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 10px;
  font-weight: 600;
}

/* 快速提示 */
.edit-quick-hint {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  border: 1px solid var(--color-accent);
}

.edit-quick-hint__header {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  font-weight: 600;
  color: var(--color-accent);
}

.edit-quick-hint__label {
  margin: 0;
  font-size: 12px;
  color: var(--color-text);
  font-weight: 500;
}

.edit-quick-hint__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.edit-tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: var(--radius-control);
  font-size: 10px;
  font-weight: 500;
}

.edit-tag--add {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.edit-tag--remove {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

/* 输入区域 */
.edit-input-area {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.edit-textarea {
  width: 100%;
  min-height: 100px;
  padding: 10px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  color: var(--color-text);
  font-size: 13px;
  font-family: inherit;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.edit-textarea::placeholder {
  color: var(--color-text-tertiary);
}

.edit-textarea:focus {
  border-color: var(--color-accent);
}

.edit-input-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.edit-char-count {
  font-size: 11px;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.edit-char-count.is-limit {
  color: var(--color-warning);
}

.edit-submit-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.edit-submit-btn:hover:not(:disabled) {
  opacity: 0.92;
}

.edit-submit-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 修改计划 */
.edit-plan {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.edit-plan__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.edit-plan__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.edit-plan__status {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: var(--radius-control);
  font-size: 10px;
  font-weight: 600;
}

.edit-plan__status--ready {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.edit-plan__status--executing {
  background: var(--color-info-soft);
  color: var(--color-info);
}

.edit-plan__status--executed {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.edit-plan__summary {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.4;
}

.edit-plan__meta {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 11px;
  color: var(--color-text-tertiary);
}

.edit-plan__regen {
  color: var(--color-warning);
  font-weight: 600;
}

.edit-plan__actions {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

.edit-plan-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: 1px solid;
  border-radius: var(--radius-control);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.edit-plan-btn--apply {
  border-color: var(--color-accent);
  background: var(--color-accent);
  color: var(--color-on-accent);
}

.edit-plan-btn--apply:hover {
  opacity: 0.92;
}

.edit-plan-btn--skip {
  border-color: var(--color-border);
  background: transparent;
  color: var(--color-text-secondary);
}

.edit-plan-btn--skip:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

/* 历史记录 */
.edit-history {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 200px;
  overflow-y: auto;
}

.edit-history-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
}

.edit-history-item__header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
}

.edit-history-item__status {
  font-weight: 600;
}

.edit-history-item__time {
  margin-left: auto;
  color: var(--color-text-tertiary);
}

.edit-history-item__feedback {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-fast) var(--ease-out);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-enter-active,
.slide-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-height var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
  overflow: hidden;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
  transform: translateY(-8px);
}

.slide-enter-to,
.slide-leave-from {
  opacity: 1;
  max-height: 200px;
  transform: translateY(0);
}

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
