import { computed, ref } from "vue";

import {
  GenerationCommandError,
  type GenerationResultRecord,
  type GenerationStatus,
  type GenerationTaskRecord,
  listResults,
  listTasks,
  markFailed,
  recordOutput,
  submitTask,
} from "../../../bridge/generations";
import { useGenerationEvents } from "./useGenerationEvents";
import { playErrorSound, playSuccessSound } from "../../../shared/composables/useNotificationSound";

export type GenerationHistoryPhase = "idle" | "loading" | "ready" | "error";

export function useGenerationHistory() {
  const phase = ref<GenerationHistoryPhase>("idle");
  const statusFilter = ref<GenerationStatus | undefined>(undefined);
  const tasks = ref<GenerationTaskRecord[]>([]);
  const selectedTask = ref<GenerationTaskRecord | null>(null);
  const results = ref<GenerationResultRecord[]>([]);
  const errorMessage = ref<string>();
  let listRequest = 0;

  const isBusy = computed(() => phase.value === "loading");

  // 实时事件监听（替代 2.5s 轮询）
  useGenerationEvents({
    onCompleted: () => {
      // 生成完成后自动刷新列表 + 提示音
      playSuccessSound();
      void load();
    },
    onFailed: (e) => {
      console.error("[Generation] failed:", e.error);
      playErrorSound();
      void load();
    },
  });

  async function load(): Promise<void> {
    const request = ++listRequest;
    clearError();
    phase.value = "loading";
    try {
      const records = await listTasks({
        status: statusFilter.value,
      });
      if (request !== listRequest) return;
      tasks.value = records;
      phase.value = "ready";
    } catch (error) {
      if (request !== listRequest) return;
      tasks.value = [];
      setError(error);
    }
  }

  async function selectTask(task: GenerationTaskRecord): Promise<void> {
    selectedTask.value = task;
    results.value = [];
    try {
      results.value = await listResults(task.id);
    } catch (error) {
      setError(error);
    }
  }

  async function refreshSelectedTask(): Promise<void> {
    if (!selectedTask.value) return;
    try {
      results.value = await listResults(selectedTask.value.id);
      const current = tasks.value.find((task) => task.id === selectedTask.value?.id);
      if (current) {
        selectedTask.value = current;
      }
    } catch (error) {
      setError(error);
    }
  }

  async function createTask(input: {
    providerName: string;
    modelName: string;
    promptText: string;
  }): Promise<GenerationTaskRecord> {
    const record = await submitTask(input);
    await load();
    return record;
  }

  async function attachOutput(taskId: string, sourcePath: string): Promise<GenerationResultRecord> {
    const result = await recordOutput(taskId, sourcePath);
    await load();
    if (selectedTask.value?.id === taskId) {
      await refreshSelectedTask();
    }
    return result;
  }

  async function failTask(taskId: string, reason: string): Promise<GenerationTaskRecord> {
    const record = await markFailed(taskId, reason);
    await load();
    if (selectedTask.value?.id === taskId) {
      selectedTask.value = record;
      await refreshSelectedTask();
    }
    return record;
  }

  function clearError(): void {
    errorMessage.value = undefined;
  }

  function setError(error: unknown): void {
    phase.value = "error";
    if (error instanceof GenerationCommandError) {
      errorMessage.value = error.message;
    } else {
      errorMessage.value = "生成任务读取失败，请稍后重试。";
    }
  }

  return {
    phase,
    statusFilter,
    tasks,
    selectedTask,
    results,
    errorMessage,
    isBusy,
    load,
    selectTask,
    refreshSelectedTask,
    createTask,
    attachOutput,
    failTask,
    clearError,
  };
}
