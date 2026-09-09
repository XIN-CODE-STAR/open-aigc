<script setup lang="ts">
/**
 * LoadingSkeleton：加载骨架屏。
 *
 * 显示内容占位符，提示用户正在加载。
 */
defineProps<{
  rows?: number;
  variant?: "default" | "card" | "text";
}>();
</script>

<template>
  <div class="skeleton" :class="`skeleton--${variant ?? 'default'}`">
    <div v-for="i in rows ?? 3" :key="i" class="skeleton-row">
      <div class="skeleton-bar" :style="{ width: `${60 + Math.random() * 40}%` }" />
    </div>
  </div>
</template>

<style scoped>
.skeleton {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
}

.skeleton-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.skeleton-bar {
  height: 14px;
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--color-surface-subtle) 25%,
    var(--color-surface-hover) 50%,
    var(--color-surface-subtle) 75%
  );
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.5s ease-in-out infinite;
}

.skeleton--card .skeleton-bar {
  height: 80px;
  border-radius: var(--radius-surface);
}

.skeleton--text .skeleton-bar {
  height: 12px;
  border-radius: 4px;
}

@keyframes skeleton-shimmer {
  0% {
    background-position: 200% 0;
  }
  100% {
    background-position: -200% 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .skeleton-bar {
    animation: none;
    background: var(--color-surface-subtle);
  }
}
</style>
