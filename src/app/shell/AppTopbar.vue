<script setup lang="ts">
/**
 * AppTopbar：顶部栏。
 *
 * 曾在此放「记忆画布」浮层开关（仅创意工坊显示）；画布已提升为独立工作区
 * （/canvas，侧栏直达），浮层入口遂成冗余，已移除。
 */
import { computed } from "vue";
import { useRoute } from "vue-router";

const route = useRoute();

const title = computed(() => String(route.meta.title ?? "OPEN AIGC"));
</script>

<template>
  <header class="topbar material-vibrancy">
    <div class="topbar__context">
      <h1 class="topbar__title">{{ title }}</h1>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  position: relative;
  z-index: 1;
  display: flex;
  min-width: 0;
  padding: 0 var(--page-padding);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.topbar__context {
  min-width: 0;
}

.topbar__title {
  margin: 0;
  overflow: hidden;
  font-family: var(--font-display);
  font-size: var(--text-title-3);
  font-weight: var(--font-weight-semibold);
  line-height: 24px;
  letter-spacing: -0.01em;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
