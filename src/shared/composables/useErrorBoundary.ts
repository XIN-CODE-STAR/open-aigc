/**
 * useErrorBoundary — 错误边界 composable。
 *
 * 集中管理组件级错误状态，支持错误分类、自动重试、错误上报。
 * 用于替代分散在各组件中的 error ref。
 */
import { ref, computed } from "vue";

export interface ErrorInfo {
  message: string;
  code?: string;
  timestamp: number;
  retryable: boolean;
  context?: string;
}

export interface UseErrorBoundaryOptions {
  /** 最大自动重试次数 */
  maxAutoRetries?: number;
  /** 自动重试间隔（毫秒） */
  retryDelay?: number;
  /** 错误回调（用于日志上报） */
  onError?: (error: ErrorInfo) => void;
}

export function useErrorBoundary(options: UseErrorBoundaryOptions = {}) {
  const { maxAutoRetries = 2, retryDelay = 1000, onError } = options;

  const errors = ref<ErrorInfo[]>([]);
  const isRetrying = ref(false);

  const hasErrors = computed(() => errors.value.length > 0);
  const latestError = computed(() => errors.value[errors.value.length - 1] ?? null);
  const retryableErrors = computed(() => errors.value.filter((e) => e.retryable));

  /**
   * 捕获并记录错误。
   */
  function capture(error: unknown, context?: string): ErrorInfo {
    const message = error instanceof Error ? error.message : String(error);
    const code =
      error instanceof Error &&
      "code" in error &&
      typeof (error as { code?: unknown }).code === "string"
        ? (error as { code: string }).code
        : undefined;

    const info: ErrorInfo = {
      message,
      code,
      timestamp: Date.now(),
      retryable: isRetryable(error),
      context,
    };

    errors.value = [...errors.value, info];
    onError?.(info);

    return info;
  }

  /**
   * 清除所有错误。
   */
  function clearAll(): void {
    errors.value = [];
  }

  /**
   * 清除指定上下文的错误。
   */
  function clearContext(context: string): void {
    errors.value = errors.value.filter((e) => e.context !== context);
  }

  /**
   * 带错误捕获的异步操作。
   */
  async function withErrorHandling<T>(fn: () => Promise<T>, context?: string): Promise<T | null> {
    try {
      return await fn();
    } catch (e) {
      capture(e, context);
      return null;
    }
  }

  /**
   * 带自动重试的异步操作。
   */
  async function withRetry<T>(fn: () => Promise<T>, context?: string): Promise<T | null> {
    let lastError: unknown;

    for (let attempt = 0; attempt <= maxAutoRetries; attempt++) {
      try {
        if (attempt > 0) {
          isRetrying.value = true;
          await new Promise((r) => setTimeout(r, retryDelay * attempt));
        }
        return await fn();
      } catch (e) {
        lastError = e;
        if (!isRetryable(e) || attempt === maxAutoRetries) {
          capture(e, context);
          return null;
        }
      } finally {
        isRetrying.value = false;
      }
    }

    capture(lastError, context);
    return null;
  }

  /**
   * 判断错误是否可重试。
   */
  function isRetryable(error: unknown): boolean {
    if (error instanceof Error) {
      const msg = error.message.toLowerCase();
      // 网络错误、超时、限流可重试
      if (msg.includes("network") || msg.includes("timeout") || msg.includes("rate limit")) {
        return true;
      }
      // 验证错误、权限错误不可重试
      if (msg.includes("validation") || msg.includes("permission") || msg.includes("auth")) {
        return false;
      }
    }
    // 默认不可重试
    return false;
  }

  return {
    errors,
    hasErrors,
    latestError,
    retryableErrors,
    isRetrying,
    capture,
    clearAll,
    clearContext,
    withErrorHandling,
    withRetry,
  };
}
