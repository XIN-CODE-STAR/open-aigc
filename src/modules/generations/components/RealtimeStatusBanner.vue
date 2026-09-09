<script setup lang="ts">
/**
 * RealtimeStatusBanner：创意工坊错误/状态浮层。
 *
 * 展示当前会话/创作流程中的错误信息，并在 Agent 模式下提供重试按钮。
 * Phase 1 拆分目标：只通过 props/emit 通信，不直接调 bridge / store。
 */
import { AlertCircle, RotateCw, X } from "@lucide/vue";

defineProps<{
  /** 当前显示的错误文本；空字符串或 null 时整个浮层隐藏。 */
  error: string | null;
  /** 是否允许显示重试按钮（Agent 模式 + 有错误时）。 */
  canRetry: boolean;
  /** Agent 正在发送中时禁用重试按钮。 */
  isRetrying?: boolean;
}>();

const emit = defineEmits<{
  retry: [];
  dismiss: [];
}>();

function onRetry(): void {
  emit("retry");
}

function onDismiss(): void {
  emit("dismiss");
}
</script>

<template>
  <Transition name="fade">
    <div v-if="error" class="grok-error" role="alert">
      <AlertCircle :size="14" />
      <span class="grok-error-text">{{ error }}</span>
      <button
        v-if="canRetry"
        type="button"
        class="grok-error-retry"
        :disabled="isRetrying"
        @click="onRetry"
      >
        <RotateCw :size="11" />
        <span>重试</span>
      </button>
      <button type="button" class="grok-error-close" title="忽略" @click="onDismiss">
        <X :size="11" />
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.grok-error {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 24px 8px;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--color-danger-soft);
  border: 1px solid color-mix(in srgb, var(--color-danger) 25%, transparent);
  color: var(--color-danger);
  font-size: 12px;
  line-height: 1.5;
}

.grok-error-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.grok-error-retry {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--color-danger) 15%, transparent);
  border: 1px solid color-mix(in srgb, var(--color-danger) 30%, transparent);
  color: var(--color-danger);
  font-size: 11px;
  cursor: pointer;
  transition: background 120ms ease;
}

.grok-error-retry:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-danger) 25%, transparent);
}

.grok-error-retry:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.grok-error-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 3px;
  background: transparent;
  border: none;
  color: var(--color-danger);
  cursor: pointer;
  border-radius: 4px;
  transition: background 120ms ease;
}

.grok-error-close:hover {
  background: color-mix(in srgb, var(--color-danger) 15%, transparent);
}

.fade-enter-active,
.fade-leave-active {
  transition:
    opacity 200ms ease,
    transform 200ms ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
