/**
 * useAutoVisionEvaluation — 图片生成完成后自动触发 AI Critic 评价。
 *
 * 监听生成任务状态，当图片生成成功时自动调用 Vision API 进行多维评价。
 * 支持开关控制、错误处理和评价结果回调。
 */
import { ref, watch, type Ref } from "vue";
import type { GenerationTaskRecord } from "../../../bridge/generations";
import { autoEvaluateImage, listVisionAdapters } from "../../../bridge/vision";
import type { ReviewReportRecord } from "../../../bridge/review";

export interface UseAutoVisionOptions {
  projectId: string;
  /** 是否启用自动评价（默认 true） */
  enabled?: boolean;
  /** 评价完成后的回调 */
  onEvaluated?: (report: ReviewReportRecord) => void;
  /** 评价失败后的回调 */
  onError?: (error: string) => void;
}

export function useAutoVisionEvaluation(options: UseAutoVisionOptions) {
  const { projectId, enabled = true, onEvaluated, onError } = options;

  const isEvaluating = ref(false);
  const lastReport = ref<ReviewReportRecord | null>(null);
  const evaluationError = ref<string | null>(null);
  const availableAdapters = ref<{ id: string; name: string; provider: string; models: string[] }[]>(
    [],
  );

  // 加载可用适配器
  async function loadAdapters(): Promise<void> {
    try {
      availableAdapters.value = await listVisionAdapters();
    } catch {
      availableAdapters.value = [];
    }
  }

  /**
   * 对一张图片触发 Vision 评价。
   * 在图片生成成功后由 watch 自动调用，也可手动调用。
   */
  async function evaluateImage(
    assetId: string,
    imageUrl: string,
    userGoal?: string,
  ): Promise<ReviewReportRecord | null> {
    if (!enabled) return null;

    isEvaluating.value = true;
    evaluationError.value = null;

    try {
      const report = (await autoEvaluateImage(
        projectId,
        assetId,
        imageUrl,
        userGoal,
      )) as unknown as ReviewReportRecord;

      lastReport.value = report;
      onEvaluated?.(report);
      return report;
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Vision 评价失败";
      evaluationError.value = msg;
      onError?.(msg);
      return null;
    } finally {
      isEvaluating.value = false;
    }
  }

  /**
   * 监听生成任务列表，当任务变为 succeeded 且是图片类型时自动触发评价。
   *
   * 使用方式：
   * ```
   * const tasks = ref<GenerationTaskRecord[]>([]);
   * const autoVision = useAutoVisionEvaluation({
   *   projectId: "xxx",
   *   onEvaluated: (report) => console.log("评分:", report.overallScore),
   * });
   * autoVision.watchTasks(tasks, (task) => task.resultAssetUrl);
   * ```
   */
  function watchTasks(
    tasks: Ref<GenerationTaskRecord[]>,
    extractImageUrl: (task: GenerationTaskRecord) => string | undefined,
  ): void {
    // 记录已处理的任务 ID，避免重复评价
    const processedTaskIds = new Set<string>();

    watch(
      () => tasks.value.map((t) => `${t.id}:${t.status}`).join(","),
      () => {
        for (const task of tasks.value) {
          if (task.status === "succeeded" && !processedTaskIds.has(task.id)) {
            processedTaskIds.add(task.id);
            const imageUrl = extractImageUrl(task);
            if (imageUrl) {
              // 异步触发评价，不阻塞 UI
              void evaluateImage(task.id, imageUrl);
            }
          }
        }
      },
      { immediate: true },
    );
  }

  // 初始化
  if (enabled) {
    void loadAdapters();
  }

  return {
    isEvaluating,
    lastReport,
    evaluationError,
    availableAdapters,
    evaluateImage,
    watchTasks,
    loadAdapters,
  };
}
