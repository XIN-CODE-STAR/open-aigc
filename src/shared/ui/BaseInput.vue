<script setup lang="ts">
/**
 * BaseInput：文本输入原语。label/hint/error 可选，error 优先展示。
 */
const model = defineModel<string>({ default: "" });

defineProps<{
  label?: string;
  placeholder?: string;
  hint?: string;
  error?: string;
  disabled?: boolean;
  type?: "text" | "password" | "url";
  maxlength?: number;
}>();
</script>

<template>
  <label class="base-input">
    <span v-if="label" class="base-input__label">{{ label }}</span>
    <input
      v-model="model"
      class="base-input__field"
      :class="{ 'base-input__field--error': error }"
      :type="type ?? 'text'"
      :placeholder="placeholder"
      :disabled="disabled"
      :maxlength="maxlength"
    />
    <span v-if="error" class="base-input__error">{{ error }}</span>
    <span v-else-if="hint" class="base-input__hint">{{ hint }}</span>
  </label>
</template>

<style scoped>
.base-input {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.base-input__label {
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: var(--font-weight-medium);
}

.base-input__field {
  height: var(--control-height);
  padding: 0 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-family: var(--font-sans);
  font-size: 13px;
  outline: none;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.base-input__field::placeholder {
  color: var(--color-text-tertiary);
}

.base-input__field:focus {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 3px var(--color-accent-soft);
}

.base-input__field--error {
  border-color: var(--color-danger);
}

.base-input__field:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.base-input__hint,
.base-input__error {
  font-size: 11px;
  line-height: var(--line-height-snug);
}

.base-input__hint {
  color: var(--color-text-tertiary);
}

.base-input__error {
  color: var(--color-danger);
}
</style>
