<script setup lang="ts">
import type { Component } from "vue";
import { useId } from "vue";

withDefaults(
  defineProps<{
    compact?: boolean;
    description: string;
    icon: Component;
    title: string;
  }>(),
  {
    compact: false,
  },
);

const titleId = useId();
</script>

<template>
  <section class="empty-state" :class="{ 'is-compact': compact }" :aria-labelledby="titleId">
    <div class="empty-state__icon-wrap">
      <component :is="icon" class="empty-state__icon" :size="28" :stroke-width="1.6" />
    </div>
    <h2 :id="titleId" class="empty-state__title">{{ title }}</h2>
    <p class="empty-state__description">{{ description }}</p>
    <div v-if="$slots.default" class="empty-state__actions"><slot /></div>
  </section>
</template>

<style scoped>
/* —— iOS 风格 empty state：圆角图标圆环 + 大标题 + 次要色描述 —— */
.empty-state {
  display: grid;
  min-height: 280px;
  padding: var(--space-8) var(--space-6);
  place-content: center;
  justify-items: center;
  text-align: center;
}

.empty-state.is-compact {
  min-height: 200px;
  padding-block: var(--space-6);
}

/* —— iOS 风格图标圆环：surface hover 背景 + 次要色 icon —— */
.empty-state__icon-wrap {
  display: grid;
  width: 72px;
  height: 72px;
  margin-bottom: var(--space-4);
  place-items: center;
  color: var(--color-text-secondary);
  border-radius: var(--radius-surface);
  background: var(--color-surface-hover);
}

.empty-state__icon {
  color: var(--color-text-secondary);
}

/* —— iOS title：headline 17pt semibold + 紧字距 —— */
.empty-state__title {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-headline);
  font-weight: 600;
  line-height: 24px;
  letter-spacing: -0.01em;
  color: var(--color-text);
}

.empty-state__description {
  max-width: 420px;
  margin: var(--space-2) 0 0;
  color: var(--color-text-secondary);
  font-size: var(--text-subhead);
  font-weight: 400;
  line-height: 20px;
}

.empty-state__actions {
  display: flex;
  margin-top: var(--space-5);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
}
</style>
