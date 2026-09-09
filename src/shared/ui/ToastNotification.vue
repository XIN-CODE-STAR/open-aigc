<script setup lang="ts">
/**
 * ToastNotification：轻量级通知提示。
 *
 * 支持 info / success / warning / error 四种类型。
 */
import { computed, onMounted, ref } from "vue";
import { AlertCircle, CheckCircle, Info, X, XCircle } from "@lucide/vue";

const props = defineProps<{
  type: "info" | "success" | "warning" | "error";
  message: string;
  duration?: number;
  dismissible?: boolean;
}>();

const emit = defineEmits<{
  dismiss: [];
}>();

const visible = ref(true);

const icon = computed(() => {
  switch (props.type) {
    case "success":
      return CheckCircle;
    case "warning":
      return AlertCircle;
    case "error":
      return XCircle;
    default:
      return Info;
  }
});

onMounted(() => {
  if (props.duration && props.duration > 0) {
    setTimeout(() => {
      visible.value = false;
      setTimeout(() => emit("dismiss"), 300);
    }, props.duration);
  }
});

function onDismiss(): void {
  visible.value = false;
  setTimeout(() => emit("dismiss"), 300);
}
</script>

<template>
  <Transition name="toast">
    <div v-if="visible" class="toast" :class="`toast--${type}`">
      <component :is="icon" :size="16" class="toast-icon" />
      <span class="toast-message">{{ message }}</span>
      <button v-if="dismissible !== false" type="button" class="toast-dismiss" @click="onDismiss">
        <X :size="14" />
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.toast {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
  box-shadow: var(--shadow-lg);
  font-size: 13px;
  color: var(--color-text);
  max-width: 400px;
}

.toast-icon {
  flex-shrink: 0;
}

.toast--info .toast-icon {
  color: var(--color-accent);
}
.toast--success .toast-icon {
  color: var(--color-success);
}
.toast--warning .toast-icon {
  color: var(--color-warning);
}
.toast--error .toast-icon {
  color: var(--color-danger);
}

.toast-message {
  flex: 1;
  min-width: 0;
  line-height: 1.4;
}

.toast-dismiss {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: all var(--duration-fast) var(--ease-out);
}

.toast-dismiss:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.toast-enter-active,
.toast-leave-active {
  transition: all var(--duration-base) var(--ease-out);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(-10px) scale(0.95);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
</style>
