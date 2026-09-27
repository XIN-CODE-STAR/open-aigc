<script setup lang="ts">
/**
 * BaseBadge：状态徽章原语。tone 决定语义色，点可选。
 */
withDefaults(
  defineProps<{
    tone?: "neutral" | "success" | "warning" | "danger" | "accent";
    dot?: boolean;
  }>(),
  { tone: "neutral", dot: false },
);
</script>

<template>
  <span class="base-badge" :class="`base-badge--${tone}`">
    <span v-if="dot" class="base-badge__dot" />
    <slot />
  </span>
</template>

<style scoped>
.base-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 10px;
  font-weight: var(--font-weight-medium);
  line-height: var(--line-height-tight);
  white-space: nowrap;
}

.base-badge__dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentcolor;
}

.base-badge--neutral {
  background: var(--color-surface-hover);
  color: var(--color-text-secondary);
}

.base-badge--success {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.base-badge--warning {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.base-badge--danger {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.base-badge--accent {
  background: var(--color-accent-soft);
  color: var(--color-accent);
}
</style>
