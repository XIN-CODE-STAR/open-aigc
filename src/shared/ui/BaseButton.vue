<script setup lang="ts">
/**
 * BaseButton：全局唯一按钮原语。
 *
 * - variant：primary（品牌主操作）/ secondary（表面强调）/ ghost（工具栏次要）/ danger（破坏性）
 * - size：sm（工具栏）/ md（表单与页面）
 * 统一替代各模块手搓的 .btn / .tool-btn 样式，保证焦点环、按压反馈与主题联动一致。
 */
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    type?: "button" | "submit";
    disabled?: boolean;
    block?: boolean;
    title?: string;
  }>(),
  {
    variant: "secondary",
    size: "md",
    type: "button",
    disabled: false,
    block: false,
    title: undefined,
  },
);

const emit = defineEmits<{ click: [event: MouseEvent] }>();

const classes = computed(() => [
  "base-btn",
  `base-btn--${props.variant}`,
  `base-btn--${props.size}`,
  { "base-btn--block": props.block },
]);

function onClick(event: MouseEvent): void {
  if (!props.disabled) emit("click", event);
}
</script>

<template>
  <button :type="type" :class="classes" :disabled="disabled" :title="title" @click="onClick">
    <slot />
  </button>
</template>

<style scoped>
.base-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  font-family: var(--font-sans);
  font-weight: var(--font-weight-medium);
  line-height: var(--line-height-tight);
  cursor: pointer;
  white-space: nowrap;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    transform var(--duration-instant) var(--ease-out);
}

.base-btn:active:not(:disabled) {
  transform: scale(0.98);
}

.base-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* —— 尺寸 —— */
.base-btn--sm {
  height: 26px;
  padding: 0 10px;
  font-size: 11px;
}

.base-btn--md {
  height: var(--control-height);
  padding: 0 16px;
  font-size: 13px;
}

/* —— 变体 —— */
.base-btn--primary {
  background: var(--color-accent);
  color: var(--color-on-accent);
}

.base-btn--primary:hover:not(:disabled) {
  background: var(--color-accent-hover);
}

.base-btn--primary:active:not(:disabled) {
  background: var(--color-accent-pressed);
}

.base-btn--secondary {
  border-color: var(--color-border);
  background: var(--color-surface);
  color: var(--color-text);
}

.base-btn--secondary:hover:not(:disabled) {
  background: var(--color-surface-hover);
  border-color: var(--color-border-strong);
}

.base-btn--ghost {
  background: transparent;
  color: var(--color-text-secondary);
}

.base-btn--ghost:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.base-btn--danger {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.base-btn--danger:hover:not(:disabled) {
  background: var(--color-danger);
  color: #ffffff;
}

.base-btn--block {
  width: 100%;
}
</style>
