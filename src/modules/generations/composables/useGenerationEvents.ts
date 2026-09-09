/**
 * useGenerationEvents — 生成事件实时监听 composable。
 *
 * 替代 2.5s 轮询，通过 Tauri 事件推送实时更新生成状态。
 *
 * 事件类型：
 * - generation://stage: 生成阶段变化（downloading → hashing → importing → evaluating → completed）
 * - generation://progress: 进度更新（0-100）
 * - generation://completed: 生成完成
 * - generation://failed: 生成失败
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ref, onMounted, onUnmounted } from "vue";

export interface GenerationStageEvent {
  attemptId: string;
  stage: string;
  detail?: string;
}

export interface GenerationCompletedEvent {
  attemptId: string;
  taskId: string;
  resultUrl?: string;
}

export interface GenerationProgressEvent {
  attemptId: string;
  taskId: string;
  progress: number;
}

export interface GenerationFailedEvent {
  attemptId: string;
  taskId: string;
  error?: string;
}

export interface UseGenerationEventsOptions {
  onStage?: (e: GenerationStageEvent) => void;
  onCompleted?: (e: GenerationCompletedEvent) => void;
  onProgress?: (e: GenerationProgressEvent) => void;
  onFailed?: (e: GenerationFailedEvent) => void;
}

export function useGenerationEvents(options: UseGenerationEventsOptions = {}) {
  const unlisteners = ref<UnlistenFn[]>([]);
  const latestStage = ref<string>("");
  const latestProgress = ref<number>(0);
  const isGenerating = ref(false);
  const lastCompleted = ref<GenerationCompletedEvent | null>(null);
  const lastFailed = ref<GenerationFailedEvent | null>(null);

  async function startListening(): Promise<void> {
    try {
      const u1 = await listen<GenerationStageEvent>("generation://stage", (event) => {
        latestStage.value = event.payload.stage;
        isGenerating.value = true;
        options.onStage?.(event.payload);
      });

      const u2 = await listen<GenerationCompletedEvent>("generation://completed", (event) => {
        isGenerating.value = false;
        latestProgress.value = 100;
        lastCompleted.value = event.payload;
        options.onCompleted?.(event.payload);
      });

      const u3 = await listen<GenerationProgressEvent>("generation://progress", (event) => {
        latestProgress.value = event.payload.progress;
        options.onProgress?.(event.payload);
      });

      const u4 = await listen<GenerationFailedEvent>("generation://failed", (event) => {
        isGenerating.value = false;
        lastFailed.value = event.payload;
        options.onFailed?.(event.payload);
      });

      unlisteners.value = [u1, u2, u3, u4];
    } catch (error) {
      console.error("[GenerationEvents] Failed to register listeners:", error);
    }
  }

  function stopListening(): void {
    unlisteners.value.forEach((fn) => fn());
    unlisteners.value = [];
  }

  onMounted(startListening);
  onUnmounted(stopListening);

  return {
    latestStage,
    latestProgress,
    isGenerating,
    lastCompleted,
    lastFailed,
    startListening,
    stopListening,
  };
}
