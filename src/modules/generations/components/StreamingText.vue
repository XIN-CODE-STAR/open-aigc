<script setup lang="ts">
/**
 * StreamingText：流式文本显示组件。
 *
 * 支持打字机效果，逐字显示 AI 响应。
 * 当 isStreaming 为 true 时显示光标闪烁动画。
 */
import { computed, ref, watch } from "vue";

const props = defineProps<{
  text: string;
  isStreaming: boolean;
  speed?: number; // 毫秒/字符
}>();

const displayedLength = ref(0);
const showCursor = computed(() => props.isStreaming || displayedLength.value < props.text.length);

const displayedText = computed(() => {
  return props.text.slice(0, displayedLength.value);
});

// Typewriter effect
watch(
  () => props.text,
  (newText, oldText) => {
    if (!oldText || newText.length < oldText.length) {
      // New response or reset
      displayedLength.value = 0;
    }

    // If streaming, animate character by character
    if (props.isStreaming && displayedLength.value < newText.length) {
      const interval = setInterval(() => {
        if (displayedLength.value < newText.length) {
          displayedLength.value++;
        } else {
          clearInterval(interval);
        }
      }, props.speed ?? 20);

      // Cleanup on next text change
      watch(
        () => props.text,
        () => clearInterval(interval),
        { once: true },
      );
    } else if (!props.isStreaming) {
      // Show all text immediately when not streaming
      displayedLength.value = newText.length;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="streaming-text">
    <span class="streaming-content">{{ displayedText }}</span>
    <span v-if="showCursor" class="streaming-cursor">|</span>
  </div>
</template>

<style scoped>
.streaming-text {
  position: relative;
  display: inline;
}

.streaming-content {
  white-space: pre-wrap;
  word-break: break-word;
}

.streaming-cursor {
  display: inline-block;
  margin-left: 1px;
  color: var(--color-accent);
  font-weight: 300;
  animation: cursor-blink 1s step-end infinite;
}

@keyframes cursor-blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .streaming-cursor {
    animation: none;
    opacity: 1;
  }
}
</style>
