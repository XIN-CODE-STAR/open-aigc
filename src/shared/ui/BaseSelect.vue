<script setup lang="ts">
/**
 * BaseSelect：统一样式的原生 select 原语。
 * options 为 { value, label } 数组；原生语义免费获得键盘/无障碍支持。
 */
const model = defineModel<string>({ default: "" });

defineProps<{
  options: { value: string; label: string }[];
  disabled?: boolean;
  title?: string;
}>();
</script>

<template>
  <span class="base-select" :title="title">
    <select v-model="model" class="base-select__field" :disabled="disabled">
      <option v-for="opt in options" :key="opt.value" :value="opt.value">
        {{ opt.label }}
      </option>
    </select>
    <svg class="base-select__caret" viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2.5 4.5L6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.4" />
    </svg>
  </span>
</template>

<style scoped>
.base-select {
  position: relative;
  display: inline-flex;
}

.base-select__field {
  appearance: none;
  height: var(--control-height);
  padding: 0 30px 0 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-family: var(--font-sans);
  font-size: 12px;
  cursor: pointer;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.base-select__field:hover:not(:disabled) {
  border-color: var(--color-border-strong);
}

.base-select__field:focus {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 3px var(--color-accent-soft);
}

.base-select__field:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.base-select__caret {
  position: absolute;
  right: 10px;
  top: 50%;
  width: 12px;
  height: 12px;
  transform: translateY(-50%);
  color: var(--color-text-tertiary);
  pointer-events: none;
}
</style>
