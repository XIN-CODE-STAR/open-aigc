<script setup lang="ts">
/**
 * ShotDetail：镜头详情面板。
 *
 * 展示选中镜头的预览、Prompt、参数、状态和生成任务。
 * 集成 AI Critic 评价、修改反馈和内容安全检查。
 */
import { ref, watch, computed } from "vue";
import {
  CheckCircle,
  Clapperboard,
  Clock,
  Film,
  LoaderCircle,
  RefreshCw,
  Save,
  Wand2,
  XCircle,
} from "@lucide/vue";

import type { Shot } from "../../../bridge/manga";
import type { GenerationAttempt } from "../../../bridge/queue";
import { useReviewFeedback } from "../composables/useReviewFeedback";
import { useAssetLicense } from "../composables/useAssetLicense";
import ReviewScoreCard from "./ReviewScoreCard.vue";
import EditFeedbackPanel from "./EditFeedbackPanel.vue";
import AssetVersionPanel from "./AssetVersionPanel.vue";
import LicenseBadge from "./LicenseBadge.vue";

const props = defineProps<{
  shot: Shot;
  attempts: GenerationAttempt[];
  generating: boolean;
  projectId: string;
}>();

const emit = defineEmits<{
  "update-prompt": [shotId: string, prompt: string];
  generate: [shotId: string, prompt: string];
  cancel: [attemptId: string];
  retry: [attemptId: string];
}>();

const editingPrompt = ref(props.shot.prompt ?? "");

// AI Critic & Edit Understanding 工作流
const reviewFeedback = useReviewFeedback({
  projectId: props.projectId,
  shotId: props.shot.id,
});

// 版权管理（当有成功的生成尝试时加载）
const successfulAttemptId = computed(
  () => props.attempts.find((a) => a.status === "succeeded")?.id ?? "",
);
const assetLicense = useAssetLicense({
  assetId: successfulAttemptId.value,
  autoLoad: !!successfulAttemptId.value,
});

// Sync prompt when shot changes
watch(
  () => props.shot.id,
  () => {
    editingPrompt.value = props.shot.prompt ?? "";
  },
);

// 切换镜头时重新加载评价和反馈数据
watch(
  () => props.shot.id,
  () => {
    void reviewFeedback.loadReviews();
    void reviewFeedback.loadEditRequests();
  },
);

function onSave(): void {
  emit("update-prompt", props.shot.id, editingPrompt.value);
}

function onGenerate(): void {
  if (!editingPrompt.value.trim()) return;
  emit("generate", props.shot.id, editingPrompt.value);
}

function statusLabel(status: string): string {
  switch (status) {
    case "draft":
      return "草稿";
    case "ready":
      return "就绪";
    case "generating":
      return "生成中…";
    case "completed":
      return "已完成";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

function attemptStatusLabel(status: string): string {
  switch (status) {
    case "pending":
      return "排队中";
    case "submitted":
      return "已提交";
    case "polling":
      return "生成中";
    case "downloading":
      return "下载中";
    case "succeeded":
      return "成功";
    case "failed":
      return "失败";
    case "cancelled":
      return "已取消";
    case "timed-out":
      return "超时";
    default:
      return status;
  }
}

function attemptStatusIcon(status: string) {
  switch (status) {
    case "succeeded":
      return CheckCircle;
    case "failed":
    case "timed-out":
      return XCircle;
    case "polling":
    case "submitted":
    case "downloading":
      return LoaderCircle;
    default:
      return Clock;
  }
}

const isTerminal = (status: string) =>
  ["succeeded", "failed", "cancelled", "timed-out"].includes(status);
</script>

<template>
  <div class="shot-detail">
    <!-- 镜头头部 -->
    <div class="shot-detail__header">
      <div class="shot-detail__meta">
        <Clapperboard :size="16" />
        <span class="shot-detail__title">镜头 {{ shot.index + 1 }}</span>
        <span v-if="shot.shotType" class="shot-detail__badge">{{ shot.shotType }}</span>
        <span class="shot-detail__status" :class="`shot-status--${shot.status}`">
          {{ statusLabel(shot.status) }}
        </span>
      </div>
    </div>

    <!-- 预览区 -->
    <div class="shot-detail__preview">
      <div class="shot-preview__placeholder">
        <Film :size="32" />
        <span>镜头预览区</span>
        <span class="shot-preview__hint">生成后将在此显示结果</span>
      </div>
    </div>

    <!-- 参数信息 -->
    <div v-if="shot.cameraMotion || shot.duration" class="shot-detail__params">
      <div v-if="shot.cameraMotion" class="shot-param">
        <Wand2 :size="12" />
        <span>{{ shot.cameraMotion }}</span>
      </div>
      <div v-if="shot.duration" class="shot-param">
        <Clock :size="12" />
        <span>{{ shot.duration }}</span>
      </div>
    </div>

    <!-- Prompt 编辑 -->
    <div class="shot-detail__prompt">
      <label class="shot-prompt__label">Prompt</label>
      <textarea
        v-model="editingPrompt"
        class="shot-prompt__input"
        placeholder="描述这个镜头的画面内容…"
        rows="4"
      />
      <div class="shot-prompt__actions">
        <button type="button" class="shot-prompt__save" @click="onSave">
          <Save :size="12" />
          保存
        </button>
        <button
          type="button"
          class="shot-prompt__generate"
          :disabled="generating || !editingPrompt.trim()"
          @click="onGenerate"
        >
          <Wand2 v-if="!generating" :size="12" />
          <LoaderCircle v-else :size="12" class="is-spinning" />
          {{ generating ? "生成中…" : "生成" }}
        </button>
      </div>
    </div>

    <!-- 生成任务列表 -->
    <div v-if="attempts.length > 0" class="shot-detail__attempts">
      <label class="shot-attempts__label">生成记录</label>
      <div class="shot-attempts__list">
        <div
          v-for="attempt in attempts"
          :key="attempt.id"
          class="attempt-item"
          :class="`attempt--${attempt.status}`"
        >
          <component
            :is="attemptStatusIcon(attempt.status)"
            :size="14"
            class="attempt-icon"
            :class="{ 'is-spinning': !isTerminal(attempt.status) }"
          />
          <span class="attempt-status">{{ attemptStatusLabel(attempt.status) }}</span>
          <span v-if="attempt.progress > 0" class="attempt-progress">
            {{ attempt.progress }}%
          </span>
          <span v-if="attempt.errorMessage" class="attempt-error" :title="attempt.errorMessage">
            {{ attempt.errorMessage }}
          </span>
          <div class="attempt-actions">
            <button
              v-if="attempt.status === 'failed' || attempt.status === 'timed-out'"
              type="button"
              class="attempt-retry"
              title="重试"
              @click="emit('retry', attempt.id)"
            >
              <RefreshCw :size="12" />
            </button>
            <button
              v-if="!isTerminal(attempt.status)"
              type="button"
              class="attempt-cancel"
              title="取消"
              @click="emit('cancel', attempt.id)"
            >
              <XCircle :size="12" />
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- AI Critic 评价报告 -->
    <ReviewScoreCard
      v-if="reviewFeedback.latestReview.value"
      :report="reviewFeedback.latestReview.value"
      :compact="false"
    />

    <!-- 视频生成质量门槛警告 -->
    <div
      v-if="!reviewFeedback.passesVideoGate.value && reviewFeedback.latestReview.value"
      class="shot-detail__gate-warning"
    >
      <XCircle :size="14" />
      <span>关键帧未通过视频生成质量门槛</span>
      <ul v-if="reviewFeedback.videoGateWarnings.value.length > 0">
        <li v-for="(w, i) in reviewFeedback.videoGateWarnings.value" :key="i">{{ w }}</li>
      </ul>
    </div>

    <!-- 用户修改反馈面板 -->
    <EditFeedbackPanel
      :project-id="projectId"
      :context-type="'generation_result'"
      :context-ref-id="shot.id"
      :source-review-id="reviewFeedback.latestReview.value?.id"
      :is-submitting="reviewFeedback.submittingFeedback.value"
      :recent-requests="reviewFeedback.editRequests.value"
      :current-plan="reviewFeedback.currentPlan.value"
      @submit-feedback="reviewFeedback.handleFeedbackSubmit"
      @apply-plan="reviewFeedback.handleApplyPlan"
      @skip-request="reviewFeedback.handleSkipRequest"
    />

    <!-- 资产版本历史（当有成功的生成尝试时显示） -->
    <div
      v-if="props.attempts.some((a) => a.status === 'succeeded')"
      class="shot-detail__asset-info"
    >
      <LicenseBadge :license="assetLicense.license.value" :compact="true" />
      <AssetVersionPanel :asset-id="successfulAttemptId" :auto-load="true" />
    </div>
  </div>
</template>

<style scoped>
.shot-detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 20px;
  height: 100%;
  overflow-y: auto;
}

.shot-detail__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.shot-detail__meta {
  display: flex;
  align-items: center;
  gap: 8px;
}

.shot-detail__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text);
}

.shot-detail__badge {
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
  font-size: 11px;
  font-weight: 500;
}

.shot-detail__status {
  font-size: 12px;
  font-weight: 500;
}

.shot-status--draft {
  color: var(--color-text-tertiary);
}
.shot-status--ready {
  color: var(--color-warning);
}
.shot-status--generating {
  color: var(--color-accent);
}
.shot-status--completed {
  color: var(--color-success);
}
.shot-status--failed {
  color: var(--color-danger);
}

/* —— 预览区 —— */
.shot-detail__preview {
  aspect-ratio: 16 / 9;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
  overflow: hidden;
}

.shot-preview__placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 8px;
  color: var(--color-text-tertiary);
  font-size: 14px;
}

.shot-preview__hint {
  font-size: 12px;
  color: var(--color-text-disabled);
}

/* —— 参数 —— */
.shot-detail__params {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.shot-param {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
  font-size: 12px;
}

/* —— Prompt —— */
.shot-detail__prompt {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.shot-prompt__label,
.shot-attempts__label {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.shot-prompt__input {
  min-height: 80px;
  padding: 10px 12px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: 14px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.shot-prompt__input:focus {
  border-color: var(--color-border);
}

.shot-prompt__input::placeholder {
  color: var(--color-text-tertiary);
}

.shot-prompt__actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.shot-prompt__save {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 14px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.shot-prompt__save:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.shot-prompt__generate {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 14px;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.shot-prompt__generate:hover:not(:disabled) {
  opacity: 0.85;
}

.shot-prompt__generate:disabled {
  background: var(--color-surface-hover);
  color: var(--color-text-disabled);
  cursor: not-allowed;
}

/* —— 生成记录 —— */
.shot-detail__attempts {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.shot-attempts__list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.attempt-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  font-size: 12px;
}

.attempt-icon {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}

.attempt--succeeded .attempt-icon {
  color: var(--color-success);
}
.attempt--failed .attempt-icon,
.attempt--timed-out .attempt-icon {
  color: var(--color-danger);
}
.attempt--polling .attempt-icon,
.attempt--submitted .attempt-icon {
  color: var(--color-accent);
}

.attempt-status {
  font-weight: 500;
  color: var(--color-text-secondary);
}

.attempt-progress {
  font-size: 11px;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.attempt-error {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-danger);
  font-size: 11px;
}

.attempt-actions {
  display: flex;
  gap: 4px;
  margin-left: auto;
}

.attempt-retry,
.attempt-cancel {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.attempt-retry:hover {
  background: var(--color-surface-hover);
  color: var(--color-accent);
}

.attempt-cancel:hover {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

/* —— 视频生成质量门槛警告 —— */
.shot-detail__gate-warning {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: var(--radius-control);
  background: var(--color-warning-soft);
  border: 1px solid var(--color-warning);
  color: var(--color-warning);
  font-size: 12px;
}

.shot-detail__gate-warning ul {
  margin: 0;
  padding-left: 16px;
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* —— 资产信息区域 —— */
.shot-detail__asset-info {
  display: flex;
  flex-direction: column;
  gap: 8px;
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

/* —— 移动端适配 —— */
@media (max-width: 640px) {
  .shot-detail {
    padding: 14px;
    gap: 12px;
  }

  .shot-detail__header {
    flex-wrap: wrap;
    gap: 8px;
  }

  .shot-detail__meta {
    flex-wrap: wrap;
    gap: 6px;
  }

  .shot-detail__preview {
    aspect-ratio: 16 / 9;
  }

  .shot-detail__params {
    gap: 8px;
  }

  .shot-prompt__input {
    min-height: 60px;
  }

  .shot-prompt__actions {
    flex-wrap: wrap;
  }

  .attempt-item {
    flex-wrap: wrap;
    gap: 6px;
  }

  .attempt-actions {
    width: 100%;
    justify-content: flex-end;
  }
}
</style>
