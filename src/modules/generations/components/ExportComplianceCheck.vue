<script setup lang="ts">
/**
 * ExportComplianceCheck：导出前合规检查组件。
 *
 * 汇总所有资产的版权和内容安全状态，
 * 在导出前进行合规检查，确保可以安全导出。
 */
import { computed } from "vue";
import { CheckCircle, AlertTriangle, XCircle, Shield, Download, LoaderCircle } from "@lucide/vue";

import type { ExportComplianceSummary } from "../../../bridge/content_guard";

const props = defineProps<{
  summary: ExportComplianceSummary;
  isChecking?: boolean;
}>();

const emit = defineEmits<{
  export: [];
  "view-blocked": [];
}>();

// ═══════════════════════════════════════════════════
// 状态计算
// ═══════════════════════════════════════════════════

const statusConfig = computed(() => {
  if (props.isChecking) {
    return {
      label: "检查中...",
      color: "var(--color-info)",
      bgColor: "var(--color-info-soft)",
      icon: LoaderCircle,
    };
  }

  if (props.summary.canExport) {
    return {
      label: "合规通过",
      color: "var(--color-success)",
      bgColor: "var(--color-success-soft)",
      icon: CheckCircle,
    };
  }

  if (props.summary.blockedCount > 0) {
    return {
      label: "存在阻断",
      color: "var(--color-danger)",
      bgColor: "var(--color-danger-soft)",
      icon: XCircle,
    };
  }

  if (props.summary.needsReviewCount > 0) {
    return {
      label: "需人工复核",
      color: "var(--color-warning)",
      bgColor: "var(--color-warning-soft)",
      icon: AlertTriangle,
    };
  }

  return {
    label: "检查完成",
    color: "var(--color-info)",
    bgColor: "var(--color-info-soft)",
    icon: Shield,
  };
});

const canExport = computed(() => props.summary.canExport && !props.isChecking);
</script>

<template>
  <div class="compliance-check" :style="{ borderColor: statusConfig.color }">
    <!-- 头部状态 -->
    <div class="compliance-header">
      <div class="compliance-status" :style="{ color: statusConfig.color }">
        <component :is="statusConfig.icon" :size="20" :class="{ 'is-spinning': isChecking }" />
        <span class="compliance-status__label">{{ statusConfig.label }}</span>
      </div>
      <button
        type="button"
        class="compliance-export-btn"
        :disabled="!canExport"
        @click="emit('export')"
      >
        <Download :size="14" />
        <span>导出</span>
      </button>
    </div>

    <!-- 汇总统计 -->
    <div class="compliance-stats">
      <div class="compliance-stat">
        <span class="compliance-stat__value" style="color: var(--color-success)">
          {{ summary.blockedCount === 0 && summary.needsReviewCount === 0 ? "✓" : "-" }}
        </span>
        <span class="compliance-stat__label">合规</span>
      </div>
      <div v-if="summary.needsReviewCount > 0" class="compliance-stat">
        <span class="compliance-stat__value" style="color: var(--color-warning)">
          {{ summary.needsReviewCount }}
        </span>
        <span class="compliance-stat__label">待复核</span>
      </div>
      <div v-if="summary.flaggedCount > 0" class="compliance-stat">
        <span class="compliance-stat__value" style="color: var(--color-orange)">
          {{ summary.flaggedCount }}
        </span>
        <span class="compliance-stat__label">已标记</span>
      </div>
      <div v-if="summary.blockedCount > 0" class="compliance-stat">
        <span class="compliance-stat__value" style="color: var(--color-danger)">
          {{ summary.blockedCount }}
        </span>
        <span class="compliance-stat__label">已阻断</span>
      </div>
    </div>

    <!-- 阻断原因列表 -->
    <div v-if="summary.blockReasons.length > 0" class="compliance-blocks">
      <div class="compliance-blocks__header">
        <XCircle :size="14" />
        <span>阻断原因</span>
        <button type="button" class="compliance-blocks__toggle" @click="emit('view-blocked')">
          查看全部
        </button>
      </div>
      <ul class="compliance-blocks__list">
        <li v-for="(reason, i) in summary.blockReasons.slice(0, 3)" :key="i">
          {{ reason }}
        </li>
        <li v-if="summary.blockReasons.length > 3" class="compliance-blocks__more">
          还有 {{ summary.blockReasons.length - 3 }} 条...
        </li>
      </ul>
    </div>

    <!-- 合规通过提示 -->
    <div v-if="canExport" class="compliance-pass">
      <CheckCircle :size="14" />
      <span>所有资产已通过合规检查，可以安全导出。</span>
    </div>
  </div>
</template>

<style scoped>
.compliance-check {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border-radius: var(--radius-surface);
  border: 1px solid;
  background: var(--color-surface);
}

/* 头部 */
.compliance-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.compliance-status {
  display: flex;
  align-items: center;
  gap: 10px;
}

.compliance-status__label {
  font-size: 15px;
  font-weight: 700;
}

.compliance-export-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.compliance-export-btn:hover:not(:disabled) {
  opacity: 0.92;
}

.compliance-export-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 统计 */
.compliance-stats {
  display: flex;
  gap: 20px;
}

.compliance-stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.compliance-stat__value {
  font-size: 20px;
  font-weight: 700;
}

.compliance-stat__label {
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* 阻断原因 */
.compliance-blocks {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border-radius: var(--radius-control);
  background: var(--color-danger-soft);
  border: 1px solid var(--color-danger);
}

.compliance-blocks__header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-danger);
}

.compliance-blocks__toggle {
  margin-left: auto;
  padding: 2px 8px;
  border: 1px solid var(--color-danger);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-danger);
  font-size: 10px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.compliance-blocks__toggle:hover {
  background: var(--color-danger-soft);
}

.compliance-blocks__list {
  margin: 0;
  padding-left: 16px;
  font-size: 11px;
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.compliance-blocks__more {
  color: var(--color-text-tertiary);
  font-style: italic;
}

/* 合规通过 */
.compliance-pass {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: var(--radius-control);
  background: var(--color-success-soft);
  border: 1px solid var(--color-success);
  color: var(--color-success);
  font-size: 12px;
  font-weight: 500;
}

/* 旋转动画 */
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
