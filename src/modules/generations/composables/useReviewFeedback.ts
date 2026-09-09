/**
 * useReviewFeedback — AI Critic 评价与编辑反馈工作流 composable。
 *
 * 管理镜头/资产的评价报告、用户反馈提交、修改计划执行的完整生命周期。
 */
import { ref, computed, watch } from "vue";
import type { ReviewReportRecord } from "../../../bridge/review";
import {
  listReviewReports,
  passesVideoGenerationGate,
  VIDEO_GENERATION_GATE,
} from "../../../bridge/review";
import type { EditRequestRecord, EditPlanRecord, EditContextType } from "../../../bridge/edit";
import {
  listEditRequests,
  submitEditFeedback,
  getEditPlanByRequest,
  applyEditPlan,
  skipEditRequest,
} from "../../../bridge/edit";

export interface UseReviewFeedbackOptions {
  projectId: string;
  assetId?: string;
  shotId?: string;
  autoLoad?: boolean;
}

const EDIT_CONTEXT_TYPES: readonly EditContextType[] = [
  "generation_result",
  "storyboard",
  "character_design",
  "visual_spec",
  "script",
  "deliverable",
];

export function useReviewFeedback(options: UseReviewFeedbackOptions) {
  const { projectId, assetId, shotId, autoLoad = true } = options;

  // ─── 评价报告 ────────────────────────────────
  const reviewReports = ref<ReviewReportRecord[]>([]);
  const loadingReviews = ref(false);
  const reviewError = ref<string | null>(null);

  const latestReview = computed(() => reviewReports.value[0] ?? null);
  const passesVideoGate = computed(() =>
    latestReview.value ? passesVideoGenerationGate(latestReview.value) : false,
  );
  const videoGateWarnings = computed(() => {
    if (!latestReview.value || passesVideoGate.value) return [];
    const warnings: string[] = [];
    if (latestReview.value.overallScore < VIDEO_GENERATION_GATE.overallMin) {
      warnings.push(
        `综合评分 ${latestReview.value.overallScore.toFixed(0)} < ${VIDEO_GENERATION_GATE.overallMin}`,
      );
    }
    return warnings;
  });

  // ─── 编辑请求 ────────────────────────────────
  const editRequests = ref<EditRequestRecord[]>([]);
  const loadingEdits = ref(false);
  const editError = ref<string | null>(null);
  const submittingFeedback = ref(false);

  const latestEditRequest = computed(() => editRequests.value[0] ?? null);
  const currentPlan = ref<EditPlanRecord | null>(null);
  const loadingPlan = ref(false);

  // ─── 加载评价报告 ────────────────────────────
  async function loadReviews(): Promise<void> {
    loadingReviews.value = true;
    reviewError.value = null;
    try {
      if (assetId) {
        // 如果有 assetId，从项目维度加载（后续可扩展为按资产筛选）
      }
      reviewReports.value = await listReviewReports(projectId, {
        shotId,
        limit: 10,
      });
    } catch (e) {
      reviewError.value = e instanceof Error ? e.message : "加载评价报告失败";
    } finally {
      loadingReviews.value = false;
    }
  }

  // ─── 加载编辑请求 ────────────────────────────
  async function loadEditRequests(): Promise<void> {
    loadingEdits.value = true;
    editError.value = null;
    try {
      editRequests.value = await listEditRequests(projectId, { limit: 10 });
    } catch (e) {
      editError.value = e instanceof Error ? e.message : "加载编辑请求失败";
    } finally {
      loadingEdits.value = false;
    }
  }

  // ─── 加载修改计划 ────────────────────────────
  async function loadPlan(requestId: string): Promise<void> {
    loadingPlan.value = true;
    try {
      currentPlan.value = await getEditPlanByRequest(requestId);
    } catch {
      currentPlan.value = null;
    } finally {
      loadingPlan.value = false;
    }
  }

  // ─── 提交反馈 ────────────────────────────────
  async function handleFeedbackSubmit(
    feedback: string,
    contextType: string,
    contextRefId?: string,
  ): Promise<boolean> {
    submittingFeedback.value = true;
    editError.value = null;
    try {
      const request = await submitEditFeedback({
        projectId,
        feedbackText: feedback,
        contextType: normalizeEditContextType(contextType),
        contextRefId,
        sourceReviewId: latestReview.value?.id,
      });
      // 刷新编辑请求列表
      await loadEditRequests();
      // 尝试加载新生成的计划
      await loadPlan(request.id);
      return true;
    } catch (e) {
      editError.value = e instanceof Error ? e.message : "提交反馈失败";
      return false;
    } finally {
      submittingFeedback.value = false;
    }
  }

  function normalizeEditContextType(contextType: string): EditContextType {
    return EDIT_CONTEXT_TYPES.includes(contextType as EditContextType)
      ? (contextType as EditContextType)
      : "generation_result";
  }

  // ─── 执行修改计划 ────────────────────────────
  async function handleApplyPlan(planId: string): Promise<boolean> {
    try {
      await applyEditPlan({ planId });
      currentPlan.value = null;
      await loadEditRequests();
      return true;
    } catch (e) {
      editError.value = e instanceof Error ? e.message : "执行计划失败";
      return false;
    }
  }

  // ─── 跳过修改请求 ────────────────────────────
  async function handleSkipRequest(requestId: string): Promise<boolean> {
    try {
      await skipEditRequest({ requestId });
      currentPlan.value = null;
      await loadEditRequests();
      return true;
    } catch (e) {
      editError.value = e instanceof Error ? e.message : "跳过请求失败";
      return false;
    }
  }

  // ─── 自动加载 ────────────────────────────────
  if (autoLoad) {
    void loadReviews();
    void loadEditRequests();
  }

  // 当最新编辑请求变化时，自动加载对应的修改计划
  watch(latestEditRequest, async (req) => {
    if (req && req.status === "plan_ready") {
      await loadPlan(req.id);
    } else {
      currentPlan.value = null;
    }
  });

  return {
    // 评价报告
    reviewReports,
    latestReview,
    loadingReviews,
    reviewError,
    passesVideoGate,
    videoGateWarnings,
    loadReviews,

    // 编辑请求
    editRequests,
    latestEditRequest,
    loadingEdits,
    editError,
    submittingFeedback,
    loadEditRequests,
    handleFeedbackSubmit,

    // 修改计划
    currentPlan,
    loadingPlan,
    loadPlan,
    handleApplyPlan,
    handleSkipRequest,
  };
}
