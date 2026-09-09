<script setup lang="ts">
/**
 * MemoryStatusBadge：长期记忆服务状态指示器。
 *
 * 绿点：EverOS 运行中，记忆可用
 * 黄点：EverOS 未启动，仅短期记忆
 * 灰点：EverOS 未配置
 */
import { computed } from "vue";
import { Database, LoaderCircle } from "@lucide/vue";

const props = defineProps<{
  /** 记忆服务状态。 */
  status: "active" | "starting" | "inactive" | "unconfigured";
}>();

void props;

const statusLabel = computed(() => {
  switch (props.status) {
    case "active":
      return "记忆可用";
    case "starting":
      return "记忆启动中…";
    case "inactive":
      return "仅短期记忆";
    case "unconfigured":
      return "记忆未配置";
    default:
      return "记忆未配置";
  }
});

const statusClass = computed(() => `memory-badge--${props.status}`);
</script>

<template>
  <div class="memory-badge" :class="statusClass">
    <span class="memory-dot" />
    <LoaderCircle v-if="status === 'starting'" :size="12" class="is-spinning" />
    <Database v-else :size="12" />
    <span>{{ statusLabel }}</span>
  </div>
</template>

<style scoped>
.memory-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border-radius: 999px;
  font-size: 11px;
  color: var(--color-text-tertiary);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.memory-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-text-disabled);
}

.memory-badge--active .memory-dot {
  background: var(--color-success);
  box-shadow: 0 0 4px var(--color-success);
}

.memory-badge--active {
  color: var(--color-success);
  border-color: var(--color-success-soft);
}

.memory-badge--starting .memory-dot {
  background: var(--color-warning);
}

.memory-badge--starting {
  color: var(--color-warning);
}

.memory-badge--inactive .memory-dot {
  background: var(--color-warning);
}

.is-spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
