<script setup lang="ts">
/**
 * ExecutionPlanPanel：执行计划面板。
 *
 * 显示 Agent 的 Plan-and-Execute 计划：
 * - 标题 + 进度百分比 + 进度条（始终可见）
 * - 目标与步骤清单（默认折叠，点击展开按钮后显示）
 */
import { computed, ref, watch } from "vue";
import { CheckCircle2, ChevronDown, Circle, CircleDot, LoaderCircle, XCircle } from "@lucide/vue";

import type { PlanRecord, PlanStepStatus } from "../../../bridge/agent";

const props = defineProps<{
  plan: PlanRecord;
}>();

/** 默认折叠：仅显示标题与进度条，点击展开按钮后显示目标与步骤明细。 */
const expanded = ref(false);

function toggleExpanded(): void {
  expanded.value = !expanded.value;
}

// 新计划到来时重置为折叠态，避免沿用上一份计划的展开状态。
watch(
  () => props.plan.id,
  () => {
    expanded.value = false;
  },
);

const progress = computed(() => {
  const total = props.plan.steps.length;
  if (total === 0) return 0;
  const done = props.plan.steps.filter(
    (s) => s.status === "completed" || s.status === "skipped",
  ).length;
  return Math.round((done / total) * 100);
});

function stepIcon(status: PlanStepStatus) {
  switch (status) {
    case "completed":
      return CheckCircle2;
    case "in-progress":
      return LoaderCircle;
    case "failed":
      return XCircle;
    case "skipped":
      return CircleDot;
    default:
      return Circle;
  }
}

function stepClass(status: PlanStepStatus): string {
  return `plan-step--${status}`;
}
</script>

<template>
  <div class="plan-panel">
    <div class="plan-header">
      <span class="plan-label">执行计划</span>
      <div class="plan-header-right">
        <span class="plan-progress-text">{{ progress }}%</span>
        <button
          type="button"
          class="plan-toggle"
          :aria-expanded="expanded"
          :title="expanded ? '收起计划细节' : '展开计划细节'"
          @click="toggleExpanded"
        >
          <ChevronDown :size="14" class="plan-toggle-icon" :class="{ 'is-open': expanded }" />
        </button>
      </div>
    </div>

    <div class="plan-progress-bar" :class="{ 'is-collapsed': !expanded }">
      <div class="plan-progress-fill" :style="{ width: `${progress}%` }" />
    </div>

    <div v-if="expanded" class="plan-details">
      <p class="plan-goal">{{ plan.goal }}</p>

      <ul class="plan-steps">
        <li
          v-for="step in plan.steps"
          :key="step.index"
          class="plan-step"
          :class="stepClass(step.status)"
        >
          <component :is="stepIcon(step.status)" :size="14" class="plan-step-icon" />
          <span class="plan-step-desc">{{ step.description }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.plan-panel {
  padding: 12px 16px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface-subtle);
}

.plan-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.plan-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  letter-spacing: 0.02em;
}

.plan-progress-text {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-accent);
  font-variant-numeric: tabular-nums;
}

.plan-header-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.plan-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition:
    background var(--duration-base) var(--ease-out),
    color var(--duration-base) var(--ease-out);
}

.plan-toggle:hover {
  background: color-mix(in srgb, var(--color-text-secondary) 16%, transparent);
  color: var(--color-text);
}

.plan-toggle-icon {
  transition: transform var(--duration-base) var(--ease-out);
}

.plan-toggle-icon.is-open {
  transform: rotate(180deg);
}

.plan-progress-bar {
  height: 3px;
  border-radius: 2px;
  background: var(--color-border-subtle);
  overflow: hidden;
}

.plan-progress-fill {
  height: 100%;
  border-radius: 2px;
  background: var(--color-accent);
  transition: width var(--duration-base) var(--ease-out);
}

.plan-details {
  margin-top: 10px;
}

.plan-goal {
  margin: 0 0 10px;
  font-size: 13px;
  color: var(--color-text);
  line-height: 1.4;
}

.plan-steps {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.plan-step {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 4px 0;
}

.plan-step-icon {
  flex-shrink: 0;
  margin-top: 1px;
  color: var(--color-text-tertiary);
}

.plan-step--completed .plan-step-icon {
  color: var(--color-success);
}

.plan-step--in-progress .plan-step-icon {
  color: var(--color-accent);
  animation: spin 1s linear infinite;
}

.plan-step--failed .plan-step-icon {
  color: var(--color-danger);
}

.plan-step-desc {
  font-size: 13px;
  color: var(--color-text-secondary);
  line-height: 1.4;
}

.plan-step--completed .plan-step-desc {
  color: var(--color-text-tertiary);
  text-decoration: line-through;
}

.plan-step--in-progress .plan-step-desc {
  color: var(--color-text);
  font-weight: 500;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
