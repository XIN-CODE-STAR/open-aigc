<script setup lang="ts">
/**
 * AppTopbar：顶部栏。页面标题 + 上下文操作（记忆画布开关仅创意工坊显示）。
 */
import { computed } from "vue";
import { Waypoints } from "@lucide/vue";
import { useRoute } from "vue-router";

import { useMemoryCanvasStore } from "../stores/memoryCanvas";

const route = useRoute();
const memoryCanvas = useMemoryCanvasStore();

const isGenerationsRoute = computed(() => route.path === "/generations");
const title = computed(() => String(route.meta.title ?? "OPEN AIGC"));
</script>

<template>
  <header class="topbar material-vibrancy">
    <div class="topbar__context">
      <h1 class="topbar__title">{{ title }}</h1>
    </div>
    <button
      v-if="isGenerationsRoute"
      class="topbar__canvas-btn"
      :class="{ 'is-active': memoryCanvas.visible }"
      title="工作记忆画布"
      @click="memoryCanvas.toggle()"
    >
      <Waypoints :size="16" :stroke-width="1.8" />
    </button>
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

.topbar__canvas-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}

.topbar__canvas-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.topbar__canvas-btn.is-active {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  color: var(--color-accent);
}
</style>
