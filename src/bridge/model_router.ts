/**
 * Model Router — 前端桥接层。
 *
 * 封装智能模型路由调度，支持根据任务类型、成本、速度、质量选择最优模型。
 */
import { z } from "zod";
import { invokeNative, isNativeRuntimeAvailable, NativeCommandError } from "./native";

// ═══════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════

export const routingTaskTypeSchema = z.enum([
  "text_generation",
  "image_generation",
  "video_generation",
  "text_to_speech",
  "vision_evaluation",
  "content_guard",
  "feedback_parsing",
  "character_consistency",
  "style_consistency",
]);

const routingStrategySchema = z.enum([
  "cost_optimized",
  "quality_first",
  "speed_first",
  "balanced",
  "user_specified",
]);

const approvalPolicySchema = z.enum(["auto", "confirm_cost", "confirm_final"]);

const modelCandidateSchema = z
  .object({
    providerId: z.string().min(1),
    modelName: z.string().min(1),
    displayName: z.string().min(1),
    capabilityScore: z.number().min(0).max(1),
    costScore: z.number().min(0).max(1),
    speedScore: z.number().min(0).max(1),
    qualityScore: z.number().min(0).max(1),
    overallScore: z.number().min(0).max(1),
    available: z.boolean(),
    remainingQuota: z.number().int(),
    healthStatus: z.string().min(1),
  })
  .strict();

const routingDecisionSchema = z
  .object({
    selected: modelCandidateSchema,
    alternatives: z.array(modelCandidateSchema),
    reason: z.string().min(1),
    strategy: routingStrategySchema,
    requiresConfirmation: z.boolean(),
    approvalPolicy: approvalPolicySchema,
    estimatedCost: z.number().nullable(),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type RoutingTaskType = z.infer<typeof routingTaskTypeSchema>;
export type RoutingStrategy = z.infer<typeof routingStrategySchema>;
export type ApprovalPolicy = z.infer<typeof approvalPolicySchema>;
export type ModelCandidate = z.infer<typeof modelCandidateSchema>;
export type RoutingDecision = z.infer<typeof routingDecisionSchema>;

export { NativeCommandError as RouterCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isRouterRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 根据任务类型和策略选择最优模型。
 */
export async function routeModel(request: {
  taskType: RoutingTaskType;
  strategy?: RoutingStrategy;
  preferredProvider?: string;
  preferredModel?: string;
  budgetLimit?: number;
  requiresReference?: boolean;
  context?: string;
}): Promise<RoutingDecision> {
  return invokeNative("model_router_v1_route", routingDecisionSchema, { request });
}

/**
 * 列出可用模型（按任务类型筛选）。
 */
export async function listAvailableModels(taskType?: RoutingTaskType): Promise<ModelCandidate[]> {
  return invokeNative("model_router_v1_list_models", z.array(modelCandidateSchema), {
    request: { taskType },
  });
}

/**
 * 记录模型使用结果（用于运行时学习）。
 */
export async function recordModelOutcome(request: {
  providerId: string;
  modelName: string;
  taskType: RoutingTaskType;
  success: boolean;
}): Promise<{ recorded: boolean }> {
  return invokeNative("model_router_v1_record_outcome", z.object({ recorded: z.boolean() }), {
    request,
  });
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/**
 * 将任务类型翻译为用户可读的中文标签。
 */
export function routingTaskTypeLabel(type: RoutingTaskType): string {
  const labels: Record<RoutingTaskType, string> = {
    text_generation: "文本生成",
    image_generation: "图片生成",
    video_generation: "视频生成",
    text_to_speech: "语音合成",
    vision_evaluation: "视觉评价",
    content_guard: "内容安全",
    feedback_parsing: "反馈解析",
    character_consistency: "角色一致性",
    style_consistency: "风格一致性",
  };
  return labels[type] ?? type;
}

/**
 * 将路由策略翻译为用户可读的中文标签。
 */
export function routingStrategyLabel(strategy: RoutingStrategy): string {
  const labels: Record<RoutingStrategy, string> = {
    cost_optimized: "最低成本",
    quality_first: "最高质量",
    speed_first: "最快速度",
    balanced: "平衡模式",
    user_specified: "用户指定",
  };
  return labels[strategy] ?? strategy;
}

/**
 * 根据前端任务场景推断路由任务类型。
 */
export function inferTaskType(context: {
  isImageGeneration?: boolean;
  isVideoGeneration?: boolean;
  isVisionEvaluation?: boolean;
  isContentGuard?: boolean;
  isFeedbackParsing?: boolean;
}): RoutingTaskType {
  if (context.isVideoGeneration) return "video_generation";
  if (context.isImageGeneration) return "image_generation";
  if (context.isVisionEvaluation) return "vision_evaluation";
  if (context.isContentGuard) return "content_guard";
  if (context.isFeedbackParsing) return "feedback_parsing";
  return "text_generation";
}

/**
 * 快速路由（便捷函数）。
 * 自动推断任务类型，使用平衡策略。
 */
export async function quickRoute(
  taskType: RoutingTaskType,
  preferredModel?: string,
): Promise<RoutingDecision> {
  return routeModel({
    taskType,
    strategy: "balanced",
    preferredModel,
  });
}
