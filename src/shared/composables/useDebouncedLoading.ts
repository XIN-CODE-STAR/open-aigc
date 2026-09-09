/**
 * useDebouncedLoading — 防抖加载 composable。
 *
 * 用于避免频繁的 IPC 调用（如切换镜头时的评价报告加载）。
 * 支持防抖、取消、错误重试。
 */
import { ref, onUnmounted } from "vue";

export interface UseDebouncedLoadingOptions {
  /** 防抖延迟（毫秒），默认 300 */
  delay?: number;
  /** 最大重试次数，默认 2 */
  maxRetries?: number;
  /** 重试间隔（毫秒），默认 1000 */
  retryDelay?: number;
}

export function useDebouncedLoading(options: UseDebouncedLoadingOptions = {}) {
  const { delay = 300, maxRetries = 2, retryDelay = 1000 } = options;

  const isLoading = ref(false);
  const error = ref<string | null>(null);
  const retryCount = ref(0);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let abortController: AbortController | null = null;

  /**
   * 防抖执行异步操作。
   * 连续调用时只执行最后一次。
   */
  function debounced<T>(fn: () => Promise<T>): Promise<T | null> {
    return new Promise((resolve) => {
      if (debounceTimer) clearTimeout(debounceTimer);

      debounceTimer = setTimeout(async () => {
        isLoading.value = true;
        error.value = null;
        retryCount.value = 0;

        try {
          const result = await fn();
          resolve(result);
        } catch (e) {
          const msg = e instanceof Error ? e.message : "操作失败";
          error.value = msg;

          // 自动重试
          if (retryCount.value < maxRetries) {
            retryCount.value++;
            retryTimer = setTimeout(() => {
              void debounced(fn);
            }, retryDelay);
          }

          resolve(null);
        } finally {
          isLoading.value = false;
        }
      }, delay);
    });
  }

  /**
   * 立即执行（不防抖）。
   */
  async function immediate<T>(fn: () => Promise<T>): Promise<T | null> {
    isLoading.value = true;
    error.value = null;

    try {
      return await fn();
    } catch (e) {
      error.value = e instanceof Error ? e.message : "操作失败";
      return null;
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * 手动重试。
   */
  async function retry<T>(fn: () => Promise<T>): Promise<T | null> {
    retryCount.value = 0;
    return immediate(fn);
  }

  /**
   * 清除错误状态。
   */
  function clearError(): void {
    error.value = null;
  }

  /**
   * 取消待执行的防抖操作。
   */
  function cancel(): void {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (retryTimer) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    if (abortController) {
      abortController.abort();
      abortController = null;
    }
  }

  // 组件卸载时自动清理
  onUnmounted(() => {
    cancel();
  });

  return {
    isLoading,
    error,
    retryCount,
    debounced,
    immediate,
    retry,
    clearError,
    cancel,
  };
}
