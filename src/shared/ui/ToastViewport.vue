<script setup lang="ts">
import { useToast } from "./useToast";

const { toasts, remove } = useToast();
</script>

<template>
  <Teleport to="body">
    <div class="toast-viewport" aria-live="polite" aria-atomic="false">
      <TransitionGroup name="toast">
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="toast-card"
          :class="`toast-card--${toast.type}`"
          role="status"
          @click="remove(toast.id)"
        >
          <span class="toast-card__message">{{ toast.message }}</span>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-viewport {
  position: fixed;
  z-index: 2000;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 8px);
  bottom: var(--space-5, 20px);
  left: 50%;
  transform: translateX(-50%);
  pointer-events: none;
  max-width: min(480px, calc(100vw - 32px));
}

.toast-card {
  display: flex;
  align-items: center;
  gap: var(--space-2, 8px);
  padding: var(--space-3, 12px) var(--space-4, 16px);
  border-radius: var(--radius-control, 8px);
  background: var(--color-surface, #1e1e2e);
  border: 1px solid var(--color-border-subtle, rgba(255, 255, 255, 0.08));
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  pointer-events: auto;
  cursor: pointer;
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

.toast-card__message {
  color: var(--color-text, #e0e0e0);
  font-size: var(--text-subhead, 13px);
  line-height: 20px;
}

.toast-card--success {
  border-color: rgba(34, 197, 94, 0.3);
}

.toast-card--success .toast-card__message {
  color: var(--color-success, #22c55e);
}

.toast-card--error {
  border-color: rgba(239, 68, 68, 0.3);
}

.toast-card--error .toast-card__message {
  color: var(--color-danger, #ef4444);
}

.toast-card--warning {
  border-color: rgba(245, 158, 11, 0.3);
}

.toast-card--warning .toast-card__message {
  color: var(--color-warning, #f59e0b);
}

.toast-card--info {
  border-color: rgba(99, 102, 241, 0.3);
}

.toast-card--info .toast-card__message {
  color: var(--color-accent, #6366f1);
}

.toast-enter-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.toast-leave-active {
  transition: all 0.2s cubic-bezier(0.4, 0, 1, 1);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(12px) scale(0.96);
}

.toast-leave-to {
  opacity: 0;
  transform: translateY(-8px) scale(0.96);
}

.toast-move {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
</style>
