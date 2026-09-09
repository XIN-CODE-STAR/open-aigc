<script setup lang="ts">
/**
 * ModelRouterSelector：模型路由选择器组件。
 *
 * 展示可用模型列表、综合评分、路由策略选择。
 * 支持用户手动选择模型或使用智能路由。
 */
import { ref, onMounted, watch } from "vue";
import {
  Zap,
  DollarSign,
  Gauge,
  Star,
  CheckCircle,
  AlertTriangle,
  LoaderCircle,
  Shuffle,
  Settings,
} from "@lucide/vue";

import {
  routeModel,
  listAvailableModels,
  routingTaskTypeLabel,
  routingStrategyLabel,
  type RoutingTaskType,
  type RoutingStrategy,
  type ModelCandidate,
  type RoutingDecision,
} from "../../../bridge/model_router";

const props = defineProps<{
  taskType: RoutingTaskType;
  preferredModel?: string;
  autoRoute?: boolean;
}>();

const emit = defineEmits<{
  "select-model": [providerId: string, modelName: string];
  "route-decision": [decision: RoutingDecision];
}>();

const models = ref<ModelCandidate[]>([]);
const selectedModel = ref<ModelCandidate | null>(null);
const routingDecision = ref<RoutingDecision | null>(null);
const strategy = ref<RoutingStrategy>("balanced");
const loading = ref(false);
const routing = ref(false);
const showAdvanced = ref(false);
const error = ref<string | null>(null);

const strategies: RoutingStrategy[] = [
  "balanced",
  "cost_optimized",
  "quality_first",
  "speed_first",
  "user_specified",
];

async function loadModels() {
  loading.value = true;
  error.value = null;
  try {
    models.value = await listAvailableModels(props.taskType);
    // 按综合评分排序
    models.value.sort((a, b) => b.overallScore - a.overallScore);
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载模型列表失败";
  } finally {
    loading.value = false;
  }
}

async function handleRoute() {
  routing.value = true;
  error.value = null;
  try {
    routingDecision.value = await routeModel({
      taskType: props.taskType,
      strategy: strategy.value,
      preferredModel: props.preferredModel,
    });
    selectedModel.value = routingDecision.value.selected;
    emit("route-decision", routingDecision.value);
    emit("select-model", selectedModel.value.providerId, selectedModel.value.modelName);
  } catch (e) {
    error.value = e instanceof Error ? e.message : "模型路由失败";
  } finally {
    routing.value = false;
  }
}

function selectModel(model: ModelCandidate) {
  selectedModel.value = model;
  emit("select-model", model.providerId, model.modelName);
}

function scoreColor(score: number): string {
  if (score >= 0.85) return "var(--color-success)";
  if (score >= 0.7) return "var(--color-info)";
  if (score >= 0.5) return "var(--color-warning)";
  return "var(--color-text-disabled)";
}

function scoreBarWidth(score: number): string {
  return `${Math.round(score * 100)}%`;
}

onMounted(() => {
  void loadModels();
  if (props.autoRoute) {
    void handleRoute();
  }
});

watch(
  () => props.taskType,
  () => {
    void loadModels();
    selectedModel.value = null;
    routingDecision.value = null;
  },
);
</script>

<template>
  <div class="model-router-selector">
    <!-- 头部 -->
    <div class="model-router-selector__header">
      <div class="model-router-selector__title">
        <Zap :size="14" />
        <span>模型选择</span>
        <span class="model-router-selector__task-type">
          {{ routingTaskTypeLabel(taskType) }}
        </span>
      </div>
      <div class="model-router-selector__actions">
        <button
          type="button"
          class="model-router-selector__settings"
          @click="showAdvanced = !showAdvanced"
        >
          <Settings :size="12" />
        </button>
        <button
          type="button"
          class="model-router-selector__route"
          :disabled="routing"
          @click="handleRoute"
        >
          <Shuffle v-if="!routing" :size="12" />
          <LoaderCircle v-else :size="12" class="is-spinning" />
          <span>{{ routing ? "路由中..." : "智能路由" }}</span>
        </button>
      </div>
    </div>

    <!-- 策略选择（高级模式） -->
    <Transition name="slide">
      <div v-if="showAdvanced" class="model-router-selector__strategies">
        <label class="strategy-label">路由策略</label>
        <div class="strategy-options">
          <button
            v-for="s in strategies"
            :key="s"
            type="button"
            class="strategy-chip"
            :class="{ 'is-active': strategy === s }"
            @click="strategy = s"
          >
            {{ routingStrategyLabel(s) }}
          </button>
        </div>
      </div>
    </Transition>

    <!-- 路由决策结果 -->
    <div v-if="routingDecision" class="model-router-selector__decision">
      <div class="decision-header">
        <CheckCircle :size="14" />
        <span class="decision-reason">{{ routingDecision.reason }}</span>
      </div>
      <div v-if="routingDecision.requiresConfirmation" class="decision-warning">
        <AlertTriangle :size="12" />
        <span>高成本操作，建议确认后再执行</span>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="model-router-selector__error">
      <AlertTriangle :size="14" />
      <span>{{ error }}</span>
    </div>

    <!-- 模型列表 -->
    <div class="model-router-selector__list">
      <div v-if="loading" class="model-router-selector__loading">
        <LoaderCircle :size="16" class="is-spinning" />
        <span>加载模型...</span>
      </div>

      <div
        v-for="model in models"
        :key="`${model.providerId}-${model.modelName}`"
        class="model-item"
        :class="{ 'is-selected': selectedModel?.modelName === model.modelName }"
        @click="selectModel(model)"
      >
        <div class="model-item__header">
          <span class="model-item__name">{{ model.displayName }}</span>
          <span
            v-if="selectedModel?.modelName === model.modelName"
            class="model-item__selected-badge"
          >
            <CheckCircle :size="12" />
          </span>
        </div>

        <div class="model-item__scores">
          <div class="model-score" title="综合评分">
            <Star :size="10" />
            <div class="model-score__bar">
              <div
                class="model-score__fill"
                :style="{
                  width: scoreBarWidth(model.overallScore),
                  backgroundColor: scoreColor(model.overallScore),
                }"
              />
            </div>
            <span class="model-score__value" :style="{ color: scoreColor(model.overallScore) }">
              {{ (model.overallScore * 100).toFixed(0) }}
            </span>
          </div>

          <div class="model-score" title="质量评分">
            <Zap :size="10" />
            <div class="model-score__bar">
              <div
                class="model-score__fill"
                :style="{
                  width: scoreBarWidth(model.qualityScore),
                  backgroundColor: scoreColor(model.qualityScore),
                }"
              />
            </div>
            <span class="model-score__value">{{ (model.qualityScore * 100).toFixed(0) }}</span>
          </div>

          <div class="model-score" title="成本评分（越高越便宜）">
            <DollarSign :size="10" />
            <div class="model-score__bar">
              <div
                class="model-score__fill"
                :style="{
                  width: scoreBarWidth(model.costScore),
                  backgroundColor: scoreColor(model.costScore),
                }"
              />
            </div>
            <span class="model-score__value">{{ (model.costScore * 100).toFixed(0) }}</span>
          </div>

          <div class="model-score" title="速度评分">
            <Gauge :size="10" />
            <div class="model-score__bar">
              <div
                class="model-score__fill"
                :style="{
                  width: scoreBarWidth(model.speedScore),
                  backgroundColor: scoreColor(model.speedScore),
                }"
              />
            </div>
            <span class="model-score__value">{{ (model.speedScore * 100).toFixed(0) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.model-router-selector {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}

.model-router-selector__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.model-router-selector__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.model-router-selector__task-type {
  font-size: 10px;
  font-weight: 500;
  padding: 2px 6px;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.model-router-selector__actions {
  display: flex;
  gap: 6px;
}

.model-router-selector__settings,
.model-router-selector__route {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
}

.model-router-selector__route {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.model-router-selector__route:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 策略选择 */
.model-router-selector__strategies {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.strategy-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.strategy-options {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.strategy-chip {
  display: inline-flex;
  align-items: center;
  padding: 3px 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 10px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.strategy-chip:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

.strategy-chip.is-active {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

/* 路由决策 */
.model-router-selector__decision {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  background: var(--color-success-soft);
  border: 1px solid var(--color-success);
}

.decision-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--color-success);
}

.decision-reason {
  flex: 1;
  font-weight: 500;
}

.decision-warning {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--color-warning);
}

/* 错误 */
.model-router-selector__error {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  background: var(--color-danger-soft);
  color: var(--color-danger);
  font-size: 12px;
}

/* 模型列表 */
.model-router-selector__list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.model-router-selector__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 16px;
  font-size: 12px;
  color: var(--color-text-tertiary);
}

.model-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.model-item:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

.model-item.is-selected {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
}

.model-item__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.model-item__name {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text);
}

.model-item__selected-badge {
  color: var(--color-accent);
}

/* 评分条 */
.model-item__scores {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.model-score {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.model-score__bar {
  flex: 1;
  height: 4px;
  border-radius: 2px;
  background: var(--color-surface-subtle);
  overflow: hidden;
}

.model-score__fill {
  height: 100%;
  border-radius: 2px;
  transition: width var(--duration-fast) var(--ease-out);
}

.model-score__value {
  font-size: 10px;
  font-weight: 600;
  min-width: 20px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

/* 动画 */
.slide-enter-active,
.slide-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-height var(--duration-fast) var(--ease-out);
  overflow: hidden;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
}

.slide-enter-to,
.slide-leave-from {
  opacity: 1;
  max-height: 100px;
}

.is-spinning {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
