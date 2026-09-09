<script setup lang="ts">
/**
 * LicenseBadge：版权状态标签组件。
 *
 * 显示资产的版权状态、商用权限和风险标记。
 * 用于资产卡片、导出按钮旁显示版权合规状态。
 */
import { computed } from "vue";
import { CheckCircle, AlertTriangle, XCircle, Shield, FileText } from "@lucide/vue";

import type { AssetLicenseRecord } from "../../../bridge/review";

const props = defineProps<{
  license: AssetLicenseRecord | null;
  compact?: boolean;
}>();

const emit = defineEmits<{
  "view-details": [];
}>();

const statusConfig = computed(() => {
  if (!props.license) {
    return {
      label: "版权未知",
      color: "var(--color-text-disabled)",
      bgColor: "var(--color-surface-subtle)",
      icon: Shield,
    };
  }

  switch (props.license.commercialUseStatus) {
    case "clear":
      return {
        label: "可商用",
        color: "var(--color-success)",
        bgColor: "var(--color-success-soft)",
        icon: CheckCircle,
      };
    case "needs_review":
      return {
        label: "需审核",
        color: "var(--color-warning)",
        bgColor: "var(--color-warning-soft)",
        icon: AlertTriangle,
      };
    case "restricted":
      return {
        label: "受限",
        color: "var(--color-orange)",
        bgColor: "var(--color-orange-soft)",
        icon: AlertTriangle,
      };
    case "blocked":
      return {
        label: "禁止",
        color: "var(--color-danger)",
        bgColor: "var(--color-danger-soft)",
        icon: XCircle,
      };
    default:
      return {
        label: "未知",
        color: "var(--color-text-disabled)",
        bgColor: "var(--color-surface-subtle)",
        icon: Shield,
      };
  }
});

const riskFlags = computed(() => {
  if (!props.license?.riskFlagsJson) return [];
  try {
    return JSON.parse(props.license.riskFlagsJson) as string[];
  } catch {
    return [];
  }
});

const sourceTypeLabel = computed(() => {
  if (!props.license) return "";
  switch (props.license.sourceType) {
    case "ai_generated":
      return "AI 生成";
    case "user_uploaded":
      return "用户上传";
    case "third_party":
      return "第三方素材";
    case "derived":
      return "衍生作品";
    default:
      return props.license.sourceType;
  }
});

function riskFlagLabel(flag: string): string {
  const labels: Record<string, string> = {
    third_party_reference: "第三方参考图",
    person_likeness: "人物肖像",
    brand_logo: "品牌标识",
    copyrighted_material: "版权素材",
    music_copyright: "音乐版权",
    font_license: "字体授权",
  };
  return labels[flag] ?? flag;
}
</script>

<template>
  <!-- 紧凑模式 -->
  <div
    v-if="compact"
    class="license-badge"
    :style="{ color: statusConfig.color, backgroundColor: statusConfig.bgColor }"
    @click="emit('view-details')"
  >
    <component :is="statusConfig.icon" :size="12" />
    <span class="license-badge__label">{{ statusConfig.label }}</span>
  </div>

  <!-- 详细模式 -->
  <div v-else class="license-card" :style="{ borderColor: statusConfig.color }">
    <div class="license-card__header">
      <div class="license-card__status" :style="{ color: statusConfig.color }">
        <component :is="statusConfig.icon" :size="16" />
        <span>{{ statusConfig.label }}</span>
      </div>
      <span class="license-card__source">{{ sourceTypeLabel }}</span>
    </div>

    <!-- 导出权限 -->
    <div class="license-card__export">
      <div v-if="license?.exportAllowed" class="license-export--allowed">
        <CheckCircle :size="12" />
        <span>允许导出</span>
      </div>
      <div v-else class="license-export--blocked">
        <XCircle :size="12" />
        <span>{{ license?.exportBlockReason || "不允许导出" }}</span>
      </div>
    </div>

    <!-- Provider 信息 -->
    <div v-if="license?.providerId" class="license-card__provider">
      <span class="license-card__provider-label">Provider:</span>
      <span class="license-card__provider-value">{{ license.providerId }}</span>
      <span v-if="license.modelName" class="license-card__model">({{ license.modelName }})</span>
    </div>

    <!-- 版权声明 -->
    <div v-if="license?.copyrightStatement" class="license-card__copyright">
      <FileText :size="12" />
      <span>{{ license.copyrightStatement }}</span>
    </div>

    <!-- 风险标记 -->
    <div v-if="riskFlags.length > 0" class="license-card__risks">
      <AlertTriangle :size="12" />
      <div class="license-risk-list">
        <span v-for="flag in riskFlags" :key="flag" class="license-risk-tag">
          {{ riskFlagLabel(flag) }}
        </span>
      </div>
    </div>

    <!-- 审核备注 -->
    <div v-if="license?.reviewNotes" class="license-card__notes">
      <span class="license-card__notes-label">审核备注:</span>
      <span>{{ license.reviewNotes }}</span>
    </div>
  </div>
</template>

<style scoped>
/* 紧凑标签 */
.license-badge {
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

.license-badge:hover {
  opacity: 0.85;
}

.license-badge__label {
  white-space: nowrap;
}

/* 详细卡片 */
.license-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-radius: var(--radius-surface);
  border: 1px solid;
  background: var(--color-surface);
}

.license-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.license-card__status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}

.license-card__source {
  font-size: 11px;
  color: var(--color-text-secondary);
  padding: 2px 6px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

/* 导出权限 */
.license-card__export {
  display: flex;
  align-items: center;
}

.license-export--allowed {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--color-success);
}

.license-export--blocked {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--color-danger);
}

/* Provider */
.license-card__provider {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}

.license-card__provider-label {
  color: var(--color-text-tertiary);
}

.license-card__provider-value {
  color: var(--color-text);
  font-weight: 500;
}

.license-card__model {
  color: var(--color-text-secondary);
}

/* 版权声明 */
.license-card__copyright {
  display: flex;
  align-items: flex-start;
  gap: 4px;
  font-size: 11px;
  color: var(--color-text-secondary);
  padding: 6px 8px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

/* 风险标记 */
.license-card__risks {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  color: var(--color-warning);
}

.license-risk-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.license-risk-tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 6px;
  border-radius: var(--radius-control);
  background: var(--color-warning-soft);
  color: var(--color-warning);
  font-size: 10px;
  font-weight: 500;
}

/* 审核备注 */
.license-card__notes {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 11px;
}

.license-card__notes-label {
  color: var(--color-text-tertiary);
  font-weight: 600;
}
</style>
