<script setup lang="ts">
/**
 * CreativeRuntimePanel：创作流水线实时面板。
 *
 * 三层结构：
 * 1. Pipeline — 总进度条 + 当前阶段 + 耗时
 * 2. Shot Grid — 每个镜头的状态卡片
 * 3. Log — 滚动事件日志
 *
 * 数据源：useCreativeRuntime composable（监听 runtime://event）。
 */
import {
  CheckCircle,
  Circle,
  CircleDot,
  Clock,
  Download,
  Film,
  Loader2,
  Package,
  XCircle,
} from "@lucide/vue";
import type { ShotState, RuntimeLogEntry } from "../composables/useCreativeRuntime";
import type { RuntimePhase, ShotStatus } from "../../../bridge/runtime";

interface Props {
  isRunning: boolean;
  currentPhase: RuntimePhase | null;
  phaseLabel: string;
  totalShots: number;
  shotSuccessCount: number;
  progressPercent: number;
  durationSecs: number;
  sortedShots: ShotState[];
  logEntries: RuntimeLogEntry[];
  finalStatus: string | null;
  outputAssetId: string | null;
}

defineProps<Props>();

function shotStatusIcon(status: ShotStatus) {
  switch (status) {
    case "pending":
      return Circle;
    case "submitting":
    case "generating":
      return Loader2;
    case "downloaded":
      return Download;
    case "imported":
      return Package;
    case "composed":
      return Film;
    case "failed":
      return XCircle;
    default:
      return Circle;
  }
}

function shotStatusClass(status: ShotStatus): string {
  switch (status) {
    case "submitting":
    case "generating":
      return "shot--active";
    case "downloaded":
    case "imported":
    case "composed":
      return "shot--success";
    case "failed":
      return "shot--failed";
    default:
      return "shot--pending";
  }
}

function formatDuration(secs: number): string {
  if (secs < 1) return "<1s";
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return `${m}m ${s}s`;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
}

function phaseIcon(phase: RuntimePhase | null) {
  switch (phase) {
    case "planning":
      return Clock;
    case "submitting":
      return Download;
    case "polling":
      return Loader2;
    case "importing":
      return Package;
    case "composing":
      return Film;
    case "completed":
      return CheckCircle;
    case "failed":
      return XCircle;
    default:
      return CircleDot;
  }
}
</script>

<template>
  <div class="runtime-panel">
    <!-- ── Layer 1: Pipeline overview ── -->
    <div class="pipeline">
      <div class="pipeline__header">
        <component
          :is="phaseIcon(currentPhase)"
          :size="14"
          :class="{ spin: isRunning && currentPhase === 'polling' }"
        />
        <span class="pipeline__phase">{{ phaseLabel }}</span>
        <span v-if="isRunning" class="pipeline__timer">{{ formatDuration(durationSecs) }}</span>
        <span
          v-else-if="finalStatus"
          class="pipeline__result"
          :class="`pipeline__result--${finalStatus}`"
        >
          {{ shotSuccessCount }}/{{ totalShots }} · {{ formatDuration(durationSecs) }}
        </span>
      </div>

      <div class="pipeline__bar">
        <div
          class="pipeline__fill"
          :style="{ width: `${progressPercent}%` }"
          :class="{ 'pipeline__fill--done': finalStatus === 'completed' }"
        />
      </div>

      <div class="pipeline__meta">
        <span>{{ progressPercent }}%</span>
        <span v-if="outputAssetId">作品已生成</span>
      </div>
    </div>

    <!-- ── Layer 2: Shot Grid ── -->
    <div v-if="sortedShots.length > 0" class="shot-grid">
      <div
        v-for="shot in sortedShots"
        :key="shot.shotIndex"
        class="shot"
        :class="shotStatusClass(shot.status)"
      >
        <div class="shot__icon">
          <component
            :is="shotStatusIcon(shot.status)"
            :size="12"
            :class="{ spin: shot.status === 'submitting' || shot.status === 'generating' }"
          />
        </div>
        <div class="shot__info">
          <span class="shot__label">镜头 {{ shot.shotIndex }}</span>
          <span class="shot__message">{{ shot.message }}</span>
        </div>
      </div>
    </div>

    <!-- ── Layer 3: Log ── -->
    <div v-if="logEntries.length > 0" class="log">
      <div class="log__header">
        <span class="log__title">事件日志</span>
        <span class="log__count">{{ logEntries.length }}</span>
      </div>
      <div class="log__list">
        <div v-for="(entry, idx) in logEntries" :key="idx" class="log__entry">
          <span class="log__time">{{ formatTime(entry.timestamp) }}</span>
          <span class="log__msg">{{ entry.message }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.runtime-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px 16px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

/* ── Pipeline ── */
.pipeline__header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  color: var(--color-text-secondary);
}

.pipeline__phase {
  flex: 1;
  font-size: 12px;
  font-weight: 600;
}

.pipeline__timer {
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--color-text-tertiary);
}

.pipeline__result {
  font-size: 11px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}

.pipeline__result--completed {
  color: var(--color-success);
}

.pipeline__result--failed {
  color: var(--color-error);
}

.pipeline__result--partial {
  color: var(--color-warning);
}

.pipeline__bar {
  height: 3px;
  border-radius: 2px;
  background: var(--color-border-subtle);
  overflow: hidden;
  margin-bottom: 4px;
}

.pipeline__fill {
  height: 100%;
  border-radius: 2px;
  background: linear-gradient(90deg, var(--color-accent), var(--aurora-cyan));
  transition: width var(--duration-slow) var(--ease-out);
}

.pipeline__fill--done {
  background: linear-gradient(90deg, var(--color-success), var(--aurora-cyan));
}

.pipeline__meta {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  color: var(--color-text-tertiary);
}

/* ── Shot Grid ── */
.shot-grid {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.shot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  font-size: 11px;
  transition: background var(--duration-fast) var(--ease-out);
}

.shot--pending {
  color: var(--color-text-disabled);
}

.shot--active {
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.shot--success {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.shot--failed {
  background: var(--color-error-soft);
  color: var(--color-error);
}

.shot__icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.shot__info {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.shot__label {
  font-weight: 600;
  font-size: 11px;
}

.shot__message {
  font-size: 10px;
  opacity: 0.8;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Log ── */
.log__header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}

.log__title {
  font-size: 11px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.log__count {
  font-size: 10px;
  color: var(--color-text-disabled);
  font-variant-numeric: tabular-nums;
}

.log__list {
  max-height: 160px;
  overflow-y: auto;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 10px;
  line-height: 1.5;
  color: var(--color-text-tertiary);
}

.log__entry {
  display: flex;
  gap: 8px;
  padding: 1px 0;
}

.log__time {
  flex-shrink: 0;
  color: var(--color-text-disabled);
  font-variant-numeric: tabular-nums;
}

.log__msg {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Animation ── */
.spin {
  animation: spin 1.5s linear infinite;
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
