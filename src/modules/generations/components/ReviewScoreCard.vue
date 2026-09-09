<script setup lang="ts">
/**
 * ReviewScoreCard：AI Critic 评分展示组件。
 *
 * 展示五层评分体系（需求层、视觉层、内容层、商业层、技术层）
 * 和综合评分、决策建议。
 */
import { computed } from "vue";
import {
  CheckCircle,
  AlertTriangle,
  XCircle,
  Target,
  Palette,
  Lightbulb,
  TrendingUp,
  Cpu,
} from "@lucide/vue";

import type { ReviewReportRecord, IssueSeverity } from "../../../bridge/review";
import {
  issueSeverityLabel,
  passesVideoGenerationGate,
  VIDEO_GENERATION_GATE,
} from "../../../bridge/review";

const props = defineProps<{
  report: ReviewReportRecord;
  compact?: boolean;
}>();

const emit = defineEmits<{
  "view-details": [];
}>();

// ═══════════════════════════════════════════════════
// 解析五层评分
// ═══════════════════════════════════════════════════

interface ScoreLayer {
  label: string;
  icon: typeof Target;
  scores: Record<string, number | undefined>;
}

const scoreLayers = computed<ScoreLayer[]>(() => {
  const req = tryParse(props.report.requirementScoresJson);
  const vis = tryParse(props.report.visualScoresJson);
  const con = tryParse(props.report.contentScoresJson);
  const com = tryParse(props.report.commercialScoresJson);
  const tec = tryParse(props.report.technicalScoresJson);

  return [
    {
      label: "需求层",
      icon: Target,
      scores: {
        匹配度: req?.match,
        完整度: req?.completeness,
        清晰度: req?.clarity,
      },
    },
    {
      label: "视觉层",
      icon: Palette,
      scores: {
        构图: vis?.composition,
        色彩: vis?.color,
        光线: vis?.lighting,
        质感: vis?.texture,
        镜头语言: vis?.lensLanguage,
      },
    },
    {
      label: "内容层",
      icon: Lightbulb,
      scores: {
        主题匹配: con?.themeMatch,
        情绪表达: con?.emotionExpression,
        叙事目的: con?.narrativePurpose,
      },
    },
    {
      label: "商业层",
      icon: TrendingUp,
      scores: {
        平台适配: com?.platformFit,
        受众匹配: com?.audienceFit,
        转化潜力: com?.conversionPotential,
      },
    },
    {
      label: "技术层",
      icon: Cpu,
      scores: {
        清晰度: tec?.clarity,
        畸变: tec?.distortion,
        角色一致性: tec?.characterConsistency,
        运动质量: tec?.motionQuality,
      },
    },
  ];
});

const issues = computed(() => {
  try {
    const parsed = JSON.parse(props.report.issuesJson);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
});

const passesVideoGate = computed(() => passesVideoGenerationGate(props.report));

const videoGateWarnings = computed(() => {
  if (passesVideoGate.value) return [];
  const warnings: string[] = [];
  if (props.report.overallScore < VIDEO_GENERATION_GATE.overallMin) {
    warnings.push(
      `综合评分 ${props.report.overallScore.toFixed(0)} < ${VIDEO_GENERATION_GATE.overallMin}`,
    );
  }
  // 可从 technicalScoresJson 中提取更多警告
  return warnings;
});

// ═══════════════════════════════════════════════════
// 决策显示
// ═══════════════════════════════════════════════════

const decisionConfig = computed(() => {
  switch (props.report.decision) {
    case "needs_review":
      return { label: "待审核", color: "gray", icon: AlertTriangle };
    case "accept":
      return { label: "可直接采纳", color: "green", icon: CheckCircle };
    case "accept_with_suggestions":
      return { label: "可采纳（有优化建议）", color: "blue", icon: CheckCircle };
    case "revise":
      return { label: "建议局部修改", color: "yellow", icon: AlertTriangle };
    case "regenerate":
      return { label: "建议重新生成", color: "orange", icon: AlertTriangle };
    case "block":
      return { label: "阻断进入下一阶段", color: "red", icon: XCircle };
    default:
      return { label: "待评价", color: "gray", icon: AlertTriangle };
  }
});

// ═══════════════════════════════════════════════════
// 评分颜色
// ═══════════════════════════════════════════════════

function scoreColor(score: number | undefined): string {
  if (score === undefined) return "var(--color-text-disabled)";
  if (score >= 90) return "var(--color-success)";
  if (score >= 80) return "var(--color-info)";
  if (score >= 70) return "var(--color-warning)";
  if (score >= 60) return "var(--color-orange)";
  return "var(--color-danger)";
}

function scoreBgColor(score: number | undefined): string {
  if (score === undefined) return "var(--color-surface-subtle)";
  if (score >= 90) return "var(--color-success-soft)";
  if (score >= 80) return "var(--color-info-soft)";
  if (score >= 70) return "var(--color-warning-soft)";
  if (score >= 60) return "var(--color-orange-soft)";
  return "var(--color-danger-soft)";
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

function tryParse(json: string): Record<string, number | undefined> | null {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

function severityColor(severity: IssueSeverity): string {
  switch (severity) {
    case "critical":
      return "var(--color-danger)";
    case "high":
      return "var(--color-orange)";
    case "medium":
      return "var(--color-warning)";
    case "low":
      return "var(--color-text-secondary)";
    default:
      return "var(--color-text-secondary)";
  }
}
</script>

<template>
  <div class="review-score-card" :class="{ 'review-score-card--compact': compact }">
    <!-- 综合评分头部 -->
    <div class="review-header">
      <div
        class="review-overall"
        :style="{
          color: scoreColor(report.overallScore),
          backgroundColor: scoreBgColor(report.overallScore),
        }"
      >
        <span class="review-overall__score">{{ report.overallScore.toFixed(0) }}</span>
        <span class="review-overall__label">综合评分</span>
      </div>

      <div class="review-meta">
        <div class="review-decision" :class="`review-decision--${decisionConfig.color}`">
          <component :is="decisionConfig.icon" :size="14" />
          <span>{{ decisionConfig.label }}</span>
        </div>
        <div v-if="report.reviewerProvider" class="review-provider">
          {{ report.reviewerProvider }}
        </div>
        <div v-if="report.confidence !== null" class="review-confidence">
          置信度: {{ (report.confidence * 100).toFixed(0) }}%
        </div>
      </div>
    </div>

    <!-- 视频生成门槛提示 -->
    <div v-if="!passesVideoGate && !compact" class="review-gate-warning">
      <AlertTriangle :size="14" />
      <span>关键帧未通过视频生成质量门槛（{{ VIDEO_GENERATION_GATE.overallMin }}分）</span>
      <ul v-if="videoGateWarnings.length > 0" class="review-gate-list">
        <li v-for="(warning, i) in videoGateWarnings" :key="i">{{ warning }}</li>
      </ul>
    </div>

    <!-- 五层评分详情（非 compact 模式） -->
    <div v-if="!compact" class="review-layers">
      <div v-for="layer in scoreLayers" :key="layer.label" class="review-layer">
        <div class="review-layer__header">
          <component :is="layer.icon" :size="14" />
          <span class="review-layer__label">{{ layer.label }}</span>
        </div>
        <div class="review-layer__scores">
          <div
            v-for="(score, name) in layer.scores"
            :key="name"
            class="review-score-item"
            :style="{ color: scoreColor(score) }"
          >
            <span class="review-score-item__name">{{ name }}</span>
            <span class="review-score-item__value">{{
              score !== undefined ? score.toFixed(0) : "-"
            }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 问题列表 -->
    <div v-if="issues.length > 0 && !compact" class="review-issues">
      <h4 class="review-issues__title">发现的问题</h4>
      <div
        v-for="(issue, i) in issues"
        :key="i"
        class="review-issue"
        :style="{ borderLeftColor: severityColor(issue.severity) }"
      >
        <div class="review-issue__header">
          <span class="review-issue__dimension">{{ issue.dimension }}</span>
          <span class="review-issue__severity" :style="{ color: severityColor(issue.severity) }">
            {{ issueSeverityLabel(issue.severity) }}
          </span>
        </div>
        <p class="review-issue__message">{{ issue.message }}</p>
        <p v-if="issue.suggestedFix" class="review-issue__fix">
          <Lightbulb :size="12" /> {{ issue.suggestedFix }}
        </p>
      </div>
    </div>

    <!-- 操作按钮 -->
    <div v-if="!compact" class="review-actions">
      <button type="button" class="review-action-btn" @click="emit('view-details')">
        查看详细评分
      </button>
    </div>
  </div>
</template>

<style scoped>
.review-score-card {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}

.review-score-card--compact {
  gap: 8px;
  padding: 12px;
}

/* 头部：综合评分 + 决策 */
.review-header {
  display: flex;
  align-items: center;
  gap: 16px;
}

.review-overall {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 72px;
  height: 72px;
  border-radius: 50%;
  border: 2px solid currentColor;
  flex-shrink: 0;
}

.review-score-card--compact .review-overall {
  width: 48px;
  height: 48px;
}

.review-overall__score {
  font-size: 24px;
  font-weight: 700;
  line-height: 1;
}

.review-score-card--compact .review-overall__score {
  font-size: 16px;
}

.review-overall__label {
  font-size: 10px;
  opacity: 0.7;
  margin-top: 2px;
}

.review-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.review-decision {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}

.review-decision--green {
  color: var(--color-success);
}
.review-decision--blue {
  color: var(--color-info);
}
.review-decision--yellow {
  color: var(--color-warning);
}
.review-decision--orange {
  color: var(--color-orange);
}
.review-decision--red {
  color: var(--color-danger);
}
.review-decision--gray {
  color: var(--color-text-secondary);
}

.review-provider,
.review-confidence {
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* 视频生成门槛警告 */
.review-gate-warning {
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

.review-gate-list {
  margin: 0;
  padding-left: 16px;
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* 五层评分 */
.review-layers {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.review-layer {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.review-layer__header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.review-layer__scores {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.review-score-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 6px 10px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
  min-width: 60px;
}

.review-score-item__name {
  font-size: 10px;
  color: var(--color-text-tertiary);
  white-space: nowrap;
}

.review-score-item__value {
  font-size: 16px;
  font-weight: 700;
  line-height: 1.2;
}

/* 问题列表 */
.review-issues {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.review-issues__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.review-issue {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 12px;
  border-left: 3px solid;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.review-issue__header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.review-issue__dimension {
  font-weight: 600;
  color: var(--color-text);
}

.review-issue__severity {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
}

.review-issue__message {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.4;
}

.review-issue__fix {
  margin: 0;
  font-size: 11px;
  color: var(--color-info);
  display: flex;
  align-items: center;
  gap: 4px;
}

/* 操作按钮 */
.review-actions {
  display: flex;
  justify-content: flex-end;
}

.review-action-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.review-action-btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-accent);
  color: var(--color-accent);
}
</style>
