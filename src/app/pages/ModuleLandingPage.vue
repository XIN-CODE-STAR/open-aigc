<script setup lang="ts">
import type { Component } from "vue";

import EmptyState from "../../shared/ui/EmptyState.vue";

defineProps<{
  description: string;
  emptyDescription: string;
  emptyTitle: string;
  icon: Component;
}>();
</script>

<template>
  <div class="module-page">
    <p class="module-page__description">{{ description }}</p>
    <EmptyState :description="emptyDescription" :icon="icon" :title="emptyTitle" />
  </div>
</template>

<style scoped>
.module-page {
  max-width: 1180px;
  min-height: 100%;
}

.module-page__description {
  margin: 0;
  padding-bottom: var(--space-5);
  color: var(--color-text-secondary);
  border-bottom: 1px solid var(--color-border-subtle);
  font-size: 14px;
  line-height: 20px;
}
</style>
