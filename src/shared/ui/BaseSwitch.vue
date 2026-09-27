<script setup lang="ts">
/**
 * BaseSwitch：开关原语。统一替代各页面手搓的 toggle-dot / 自制开关。
 */
const model = defineModel<boolean>({ required: true });

withDefaults(defineProps<{ disabled?: boolean; title?: string }>(), {
  disabled: false,
  title: undefined,
});
</script>

<template>
  <button
    type="button"
    role="switch"
    class="base-switch"
    :class="{ 'base-switch--on': model }"
    :disabled="disabled"
    :title="title"
    :aria-checked="model"
    @click="model = !model"
  >
    <span class="base-switch__knob" />
  </button>
</template>

<style scoped>
.base-switch {
  position: relative;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
  border: none;
  border-radius: 10px;
  background: var(--color-surface-hover);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.base-switch--on {
  background: var(--color-accent);
}

.base-switch:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.base-switch__knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: var(--shadow-sm);
  transition: transform var(--duration-fast) var(--ease-out);
}

.base-switch--on .base-switch__knob {
  transform: translateX(16px);
}
</style>
