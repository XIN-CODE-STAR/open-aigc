<script setup lang="ts">
/**
 * CreativeRunProgress：创意运行进度条。
 *
 * 展示当前运行在哪个阶段，以及各阶段的完成状态。
 */
import { computed } from "vue";
import {
  CheckCircle,
  Clapperboard,
  Eye,
  Film,
  Palette,
  PenTool,
  Scissors,
  Upload,
  UserCircle,
  Wand2,
} from "@lucide/vue";

interface StageInfo {
  id: string;
  label: string;
  icon: unknown;
}

const stages: StageInfo[] = [
  { id: "requirement_analysis", label: "需求", icon: Wand2 },
  { id: "visual_spec", label: "视觉", icon: Palette },
  { id: "story_planning", label: "剧本", icon: PenTool },
  { id: "character_planning", label: "角色", icon: UserCircle },
  { id: "image_generation", label: "图片", icon: Eye },
  { id: "video_generation", label: "视频", icon: Film },
  { id: "review", label: "审核", icon: CheckCircle },
  { id: "editing", label: "剪辑", icon: Scissors },
  { id: "export", label: "导出", icon: Upload },
];

const props = defineProps<{
  currentStage: string;
  status: "pending" | "running" | "waiting_approval" | "completed" | "failed" | "cancelled";
}>();

const currentIndex = computed(() => stages.findIndex((s) => s.id === props.currentStage));

function stageStatus(index: number): "completed" | "current" | "pending" {
  if (index < currentIndex.value) return "completed";
  if (index === currentIndex.value) return "current";
  return "pending";
}

const progressPercent = computed(() => {
  if (props.status === "completed") return 100;
  if (currentIndex.value < 0) return 0;
  return Math.round((currentIndex.value / (stages.length - 1)) * 100);
});
</script>

<template>
  <div class="run-progress">
    <div class="run-progress__header">
      <Clapperboard :size="14" />
      <span class="run-progress__label">创作进度</span>
      <span class="run-progress__percent">{{ progressPercent }}%</span>
    </div>

    <div class="run-progress__bar">
      <div class="run-progress__fill" :style="{ width: `${progressPercent}%` }" />
    </div>

    <div class="run-progress__stages">
      <div
        v-for="(stage, idx) in stages"
        :key="stage.id"
        class="run-stage"
        :class="`run-stage--${stageStatus(idx)}`"
      >
        <div class="run-stage__icon">
          <component :is="stage.icon" :size="14" />
        </div>
        <span class="run-stage__label">{{ stage.label }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.run-progress {
  padding: 14px 16px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

.run-progress__header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 10px;
  color: var(--color-text-secondary);
}

.run-progress__label {
  flex: 1;
  font-size: 12px;
  font-weight: 600;
}

.run-progress__percent {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-accent);
  font-variant-numeric: tabular-nums;
}

.run-progress__bar {
  height: 3px;
  border-radius: 2px;
  background: var(--color-border-subtle);
  margin-bottom: 12px;
  overflow: hidden;
}

.run-progress__fill {
  height: 100%;
  border-radius: 2px;
  background: linear-gradient(90deg, var(--color-accent), var(--aurora-cyan));
  transition: width var(--duration-slow) var(--ease-out);
}

.run-progress__stages {
  display: flex;
  justify-content: space-between;
  gap: 2px;
}

.run-stage {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  flex: 1;
}

.run-stage__icon {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  transition: all var(--duration-fast) var(--ease-out);
}

.run-stage--completed .run-stage__icon {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.run-stage--current .run-stage__icon {
  background: var(--color-accent-soft);
  color: var(--color-accent);
  animation: pulse 2s ease-in-out infinite;
}

.run-stage--pending .run-stage__icon {
  background: var(--color-surface-subtle);
  color: var(--color-text-disabled);
}

.run-stage__label {
  font-size: 10px;
  color: var(--color-text-tertiary);
  text-align: center;
}

.run-stage--current .run-stage__label {
  color: var(--color-accent);
  font-weight: 500;
}

.run-stage--completed .run-stage__label {
  color: var(--color-text-secondary);
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.6;
  }
}

@media (prefers-reduced-motion: reduce) {
  .run-stage--current .run-stage__icon {
    animation: none;
  }
}
</style>
