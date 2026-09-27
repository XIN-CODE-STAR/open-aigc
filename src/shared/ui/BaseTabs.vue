<script setup lang="ts">
/**
 * BaseTabs：下划线式标签页原语。
 * tabs 为 { value, label }；v-model 绑定当前 value。
 */
const model = defineModel<string>({ required: true });

defineProps<{
  tabs: { value: string; label: string; count?: number }[];
}>();
</script>

<template>
  <div class="base-tabs" role="tablist">
    <button
      v-for="tab in tabs"
      :key="tab.value"
      type="button"
      role="tab"
      class="base-tabs__tab"
      :class="{ 'base-tabs__tab--active': model === tab.value }"
      :aria-selected="model === tab.value"
      @click="model = tab.value"
    >
      {{ tab.label }}
      <span v-if="tab.count !== undefined" class="base-tabs__count">{{ tab.count }}</span>
    </button>
  </div>
</template>

<style scoped>
.base-tabs {
  display: inline-flex;
  gap: var(--space-1);
  border-bottom: 1px solid var(--color-border-subtle);
}

.base-tabs__tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--color-text-tertiary);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: var(--font-weight-medium);
  cursor: pointer;
  transition:
    color var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}

.base-tabs__tab:hover {
  color: var(--color-text);
}

.base-tabs__tab--active {
  color: var(--color-accent);
  border-bottom-color: var(--color-accent);
}

.base-tabs__count {
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-hover);
  color: var(--color-text-tertiary);
  font-size: 10px;
  line-height: 16px;
}
</style>
