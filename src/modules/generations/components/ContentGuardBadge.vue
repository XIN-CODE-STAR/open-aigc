<script setup lang="ts">
/**
 * ContentGuardBadge：内容安全状态标签组件。
 *
 * 显示内容安全检查的状态（通过/警告/标记/阻断），
 * 用于在资产卡片、导出按钮等位置显示合规状态。
 */
import { computed } from "vue";
import { CheckCircle, AlertTriangle, XCircle, Shield } from "@lucide/vue";

import type { ContentGuardReport } from "../../../bridge/content_guard";

const props = defineProps<{
  report: ContentGuardReport | null;
  compact?: boolean;
}>();

const emit = defineEmits<{
  "view-details": [];
}>();

// ═══════════════════════════════════════════════════
// 状态配置
// ═══════════════════════════════════════════════════

interface StatusConfig {
  label: string;
  color: string;
  bgColor: string;
  icon: typeof CheckCircle;
}

const statusConfig = computed<StatusConfig>(() => {
  if (!props.report) {
    return {
      label: "未检查",
      color: "var(--color-text-disabled)",
      bgColor: "var(--color-surface-subtle)",
      icon: Shield,
    };
  }

  switch (props.report.status) {
    case "passed":
      return {
        label: "安全通过",
        color: "var(--color-success)",
        bgColor: "var(--color-success-soft)",
        icon: CheckCircle,
      };
    case "needs_review":
      return {
        label: "需人工复核",
        color: "var(--color-warning)",
        bgColor: "var(--color-warning-soft)",
        icon: AlertTriangle,
      };
    case "flagged":
      return {
        label: "已标记风险",
        color: "var(--color-orange)",
        bgColor: "var(--color-orange-soft)",
        icon: AlertTriangle,
      };
    case "blocked":
      return {
        label: "已阻断",
        color: "var(--color-danger)",
        bgColor: "var(--color-danger-soft)",
        icon: XCircle,
      };
    default:
      return {
        label: "未知状态",
        color: "var(--color-text-disabled)",
        bgColor: "var(--color-surface-subtle)",
        icon: Shield,
      };
  }
});

const riskLevelLabel = computed(() => {
  if (!props.report) return null;
  switch (props.report.riskLevel) {
    case "low":
      return "低风险";
    case "medium":
      return "中风险";
    case "high":
      return "高风险";
    case "critical":
      return "严重风险";
    default:
      return null;
  }
});

const checks = computed(() => {
  if (!props.report) return [];
  try {
    const parsed = JSON.parse(props.report.checksJson);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
});

const actions = computed(() => {
  if (!props.report) return { allowed: [], blocked: [] };
  try {
    return JSON.parse(props.report.actionsJson);
  } catch {
    return { allowed: [], blocked: [] };
  }
});
</script>

<template>
  <!-- 紧凑模式：仅显示标签 -->
  <div
    v-if="compact"
    class="guard-badge"
    :style="{ color: statusConfig.color, backgroundColor: statusConfig.bgColor }"
    @click="emit('view-details')"
  >
    <component :is="statusConfig.icon" :size="12" />
    <span class="guard-badge__label">{{ statusConfig.label }}</span>
  </div>

  <!-- 详细模式 -->
  <div v-else class="guard-card" :style="{ borderColor: statusConfig.color }">
    <div class="guard-card__header">
      <div class="guard-card__status" :style="{ color: statusConfig.color }">
        <component :is="statusConfig.icon" :size="16" />
        <span>{{ statusConfig.label }}</span>
      </div>
      <span v-if="riskLevelLabel" class="guard-card__risk" :style="{ color: statusConfig.color }">
        {{ riskLevelLabel }}
      </span>
    </div>

    <!-- 检查详情 -->
    <div v-if="checks.length > 0" class="guard-checks">
      <div v-for="(check, i) in checks" :key="i" class="guard-check">
        <div class="guard-check__header">
          <span class="guard-check__category">{{ check.category }}</span>
          <span class="guard-check__status" :class="`guard-check__status--${check.status}`">
            {{
              check.status === "pass"
                ? "通过"
                : check.status === "warn"
                  ? "警告"
                  : check.status === "flag"
                    ? "标记"
                    : "阻断"
            }}
          </span>
        </div>
        <p class="guard-check__message">{{ check.message }}</p>
      </div>
    </div>

    <!-- 操作权限 -->
    <div v-if="actions.blocked && actions.blocked.length > 0" class="guard-actions">
      <div class="guard-actions__blocked">
        <XCircle :size="12" />
        <span>已阻止的操作: {{ actions.blocked.join(", ") }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 紧凑标签 */
.guard-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: var(--radius-control);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.guard-badge:hover {
  opacity: 0.85;
}

.guard-badge__label {
  white-space: nowrap;
}

/* 详细卡片 */
.guard-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border-radius: var(--radius-surface);
  border: 1px solid;
  background: var(--color-surface);
}

.guard-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.guard-card__status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}

.guard-card__risk {
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

/* 检查详情 */
.guard-checks {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.guard-check {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 6px 8px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.guard-check__header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
}

.guard-check__category {
  font-weight: 600;
  color: var(--color-text);
}

.guard-check__status {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: var(--radius-control);
}

.guard-check__status--pass {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.guard-check__status--warn {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.guard-check__status--flag {
  background: var(--color-orange-soft);
  color: var(--color-orange);
}

.guard-check__status--block {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.guard-check__message {
  margin: 0;
  font-size: 11px;
  color: var(--color-text-secondary);
  line-height: 1.3;
}

/* 操作权限 */
.guard-actions {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.guard-actions__blocked {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--color-danger);
}
</style>
