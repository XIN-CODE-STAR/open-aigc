<script setup lang="ts">
/**
 * MemoryRecallPanel：记忆检索结果展示。
 *
 * 当 Agent 在规划阶段从长期记忆中检索到相关内容时显示。
 * 让学生看到 Agent 记住了哪些相关偏好和历史。
 */
import { Brain, Lightbulb, MessageSquare, User } from "@lucide/vue";

import type { MemoryRecallEntry } from "../../../bridge/agent";

defineProps<{
  memories: MemoryRecallEntry[];
}>();

function sourceIcon(type: string) {
  switch (type) {
    case "episode":
      return MessageSquare;
    case "fact":
      return Lightbulb;
    case "profile":
      return User;
    default:
      return Brain;
  }
}

function sourceLabel(type: string): string {
  switch (type) {
    case "episode":
      return "对话";
    case "fact":
      return "事实";
    case "profile":
      return "画像";
    default:
      return "记忆";
  }
}
</script>

<template>
  <div class="memory-panel">
    <div class="memory-header">
      <Brain :size="14" />
      <span>已检索到 {{ memories.length }} 条相关记忆</span>
    </div>
    <ul class="memory-list">
      <li v-for="(mem, i) in memories" :key="i" class="memory-item">
        <component :is="sourceIcon(mem.sourceType)" :size="12" class="memory-icon" />
        <span class="memory-tag">{{ sourceLabel(mem.sourceType) }}</span>
        <span class="memory-content">{{ mem.content }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.memory-panel {
  padding: 10px 14px;
  border: 1px solid var(--color-accent-soft);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

.memory-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-accent);
}

.memory-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.memory-item {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 4px 0;
}

.memory-icon {
  flex-shrink: 0;
  margin-top: 2px;
  color: var(--color-text-tertiary);
}

.memory-tag {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 10px;
  font-weight: 500;
  line-height: 1.4;
}

.memory-content {
  font-size: 13px;
  color: var(--color-text-secondary);
  line-height: 1.4;
}
</style>
