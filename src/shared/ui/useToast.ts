import { ref } from "vue";

export interface ToastMessage {
  id: string;
  type: "info" | "success" | "warning" | "error";
  message: string;
  duration: number;
}

const toasts = ref<ToastMessage[]>([]);

let nextId = 0;

function addToast(type: ToastMessage["type"], message: string, duration = 4000): string {
  const id = `toast-${++nextId}`;
  toasts.value = [...toasts.value, { id, type, message, duration }];

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
    info: (msg: string, dur?: number) => addToast("info", msg, dur),
    success: (msg: string, dur?: number) => addToast("success", msg, dur),
    warning: (msg: string, dur?: number) => addToast("warning", msg, dur),
    error: (msg: string, dur?: number) => addToast("error", msg, dur ?? 6000),
    remove: removeToast,
  };
}
