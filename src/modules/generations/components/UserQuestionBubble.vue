<script setup lang="ts">
/**
 * UserQuestionBubble：Agent 提问气泡。
 *
 * 当 Agent 通过 ask_user_question 工具向学生提问时显示。
 * 包含问题文本和内联回答输入框。
 */
import { ref } from "vue";
import { MessageCircleQuestion, Send } from "@lucide/vue";

const props = defineProps<{
  question: string;
}>();

void props;

const emit = defineEmits<{
  answer: [answer: string];
}>();

const answerText = ref("");

function onSubmit(): void {
  const trimmed = answerText.value.trim();
  if (!trimmed) return;
  emit("answer", trimmed);
  answerText.value = "";
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    onSubmit();
  }
}
</script>

<template>
  <div class="question-bubble">
    <div class="question-icon">
      <MessageCircleQuestion :size="16" />
    </div>
    <div class="question-content">
      <p class="question-text">{{ question }}</p>
      <div class="question-input-row">
        <input
          v-model="answerText"
          class="question-input"
          type="text"
          placeholder="输入你的回答…"
          @keydown="onKeydown"
        />
        <button
          type="button"
          class="question-send"
          :disabled="!answerText.trim()"
          @click="onSubmit"
        >
          <Send :size="14" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.question-bubble {
  display: flex;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--color-accent-soft);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

.question-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.question-content {
  flex: 1;
  min-width: 0;
}

.question-text {
  margin: 0 0 8px;
  font-size: 14px;
  color: var(--color-text);
  line-height: 1.5;
}

.question-input-row {
  display: flex;
  gap: 6px;
}

.question-input {
  flex: 1;
  min-width: 0;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.question-input:focus {
  border-color: var(--color-border-strong);
}

.question-input::placeholder {
  color: var(--color-text-tertiary);
}

.question-send {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  cursor: pointer;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.question-send:hover:not(:disabled) {
  opacity: 0.85;
}

.question-send:disabled {
  background: var(--color-surface-hover);
  color: var(--color-text-disabled);
  cursor: not-allowed;
}
</style>
