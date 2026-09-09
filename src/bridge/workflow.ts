/**
 * Workflow — 前端桥接层。
 *
 * 封装创作工作流的生命周期管理，支持生成→评价→反馈→修改→重新生成的完整闭环。
 */
import { z } from "zod";
import { invokeNative, isNativeRuntimeAvailable, NativeCommandError } from "./native";

// ═══════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════

const workflowStageSchema = z.enum([
  "generating",
  "evaluating",
  "waiting_feedback",
  "parsing_feedback",
  "planning_modification",
  "applying_modification",
  "regenerating",
  "completed",
  "failed",
]);

const workflowEventTypeSchema = z.enum([
  "stage_entered",
  "stage_completed",
  "stage_failed",
  "auto_evaluation_completed",
  "feedback_received",
  "modification_plan_created",
  "modification_applied",
  "regeneration_triggered",
  "workflow_completed",
  "workflow_failed",
]);

const workflowEventSchema = z
  .object({
    eventType: workflowEventTypeSchema,
    stage: workflowStageSchema,
    detail: z.string().nullable(),
    timestamp: z.string().min(1),
    reviewReportId: z.string().nullable(),
    editRequestId: z.string().nullable(),
    editPlanId: z.string().nullable(),
  })
  .strict();

const workflowStateSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    assetId: z.string().min(1),
    shotId: z.string().nullable(),
    currentStage: workflowStageSchema,
    iteration: z.number().int().nonnegative(),
    maxIterations: z.number().int().positive(),
    currentReviewId: z.string().nullable(),
    currentPlanId: z.string().nullable(),
    events: z.array(workflowEventSchema),
    isPaused: z.boolean(),
    error: z.string().nullable(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type WorkflowStage = z.infer<typeof workflowStageSchema>;
export type WorkflowEventType = z.infer<typeof workflowEventTypeSchema>;
export type WorkflowEvent = z.infer<typeof workflowEventSchema>;
export type WorkflowState = z.infer<typeof workflowStateSchema>;

export { NativeCommandError as WorkflowCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isWorkflowRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 启动新的创作工作流。
 */
export async function startWorkflow(request: {
  projectId: string;
  assetId: string;
  shotId?: string;
  maxIterations?: number;
}): Promise<WorkflowState> {
  return invokeNative("workflow_v1_start", workflowStateSchema, { request });
}

/**
 * 推进工作流到下一阶段。
 */
export async function advanceWorkflow(
  workflowId: string,
  stage: WorkflowStage,
  detail?: string,
): Promise<WorkflowState> {
  return invokeNative("workflow_v1_advance", workflowStateSchema, {
    request: { workflowId, stage, detail },
  });
}

/**
 * 暂停工作流（等待用户反馈）。
 */
export async function pauseWorkflow(workflowId: string, reviewId?: string): Promise<WorkflowState> {
  return invokeNative("workflow_v1_pause", workflowStateSchema, {
    request: { workflowId, reviewId },
  });
}

/**
 * 用户提交反馈后恢复工作流。
 */
export async function resumeWorkflow(
  workflowId: string,
  editRequestId: string,
): Promise<WorkflowState> {
  return invokeNative("workflow_v1_resume", workflowStateSchema, {
    request: { workflowId, editRequestId },
  });
}

/**
 * 标记修改计划已创建。
 */
export async function markPlanCreated(workflowId: string, planId: string): Promise<WorkflowState> {
  return invokeNative("workflow_v1_mark_plan_created", workflowStateSchema, {
    request: { workflowId, planId },
  });
}

/**
 * 标记工作流失败。
 */
export async function failWorkflow(workflowId: string, error: string): Promise<WorkflowState> {
  return invokeNative("workflow_v1_fail", workflowStateSchema, {
    request: { workflowId, error },
  });
}

/**
 * 获取工作流状态。
 */
export async function getWorkflow(workflowId: string): Promise<WorkflowState | null> {
  return invokeNative("workflow_v1_get", workflowStateSchema.nullable(), {
    request: { workflowId },
  });
}

/**
 * 列出项目的工作流。
 */
export async function listWorkflows(projectId: string): Promise<WorkflowState[]> {
  return invokeNative("workflow_v1_list", z.array(workflowStateSchema), {
    request: { projectId },
  });
}

/**
 * 列出等待反馈的工作流。
 */
export async function listWaitingWorkflows(): Promise<WorkflowState[]> {
  return invokeNative("workflow_v1_list_waiting", z.array(workflowStateSchema));
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/**
 * 将工作流阶段翻译为用户可读的中文标签。
 */
export function workflowStageLabel(stage: WorkflowStage): string {
  const labels: Record<WorkflowStage, string> = {
    generating: "生成中",
    evaluating: "自动评价中",
    waiting_feedback: "等待反馈",
    parsing_feedback: "解析反馈中",
    planning_modification: "生成修改计划",
    applying_modification: "执行修改",
    regenerating: "重新生成中",
    completed: "已完成",
    failed: "失败",
  };
  return labels[stage] ?? stage;
}

/**
 * 工作流阶段是否为终态。
 */
export function isWorkflowTerminal(stage: WorkflowStage): boolean {
  return stage === "completed" || stage === "failed";
}

/**
 * 工作流阶段是否等待用户输入。
 */
export function isWorkflowWaiting(stage: WorkflowStage): boolean {
  return stage === "waiting_feedback";
}

/**
 * 一键启动完整的创作闭环：创建 → 暂停等评价 → 等待反馈。
 * 返回创建工作流的状态。
 */
export async function startCreativeLoop(request: {
  projectId: string;
  assetId: string;
  shotId?: string;
}): Promise<WorkflowState> {
  const workflow = await startWorkflow({
    ...request,
    maxIterations: 3,
  });

  // 立即推进到评价阶段
  const evaluating = await advanceWorkflow(workflow.id, "evaluating", "自动评价已触发");

  return evaluating;
}
