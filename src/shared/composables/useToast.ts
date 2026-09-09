/**
 * useToast — 全局通知 composable。
 *
 * 替代分散在各组件中的 alert() 和内联错误提示。
 * 支持 success / warning / error / info 四种类型。
 */
import { ref } from "vue";

export type ToastType = "success" | "warning" | "error" | "info";

export interface ToastMessage {
  id: string;
  type: ToastType;
  title: string;
  message?: string;
  duration: number;
  timestamp: number;
}

const toasts = ref<ToastMessage[]>([]);
let nextId = 0;

function addToast(type: ToastType, title: string, message?: string, duration = 4000): string {
  const id = `toast-${++nextId}`;
  toasts.value = [...toasts.value, { id, type, title, message, duration, timestamp: Date.now() }];

  // 自动移除
  if (duration > 0) {
    setTimeout(() => {
      removeToast(id);
    }, duration);
  }

  return id;
}

function removeToast(id: string): void {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

export function useToast() {
  return {
    toasts,
    success: (title: string, message?: string) => addToast("success", title, message),
    warning: (title: string, message?: string) => addToast("warning", title, message, 6000),
    error: (title: string, message?: string) => addToast("error", title, message, 8000),
    info: (title: string, message?: string) => addToast("info", title, message),
    remove: removeToast,
    clear: () => {
      toasts.value = [];
    },
  };
}
