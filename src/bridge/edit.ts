/**
 * Edit Understanding Agent — 前端桥接层。

 * 按照 2026-07-20-creative-agent-critical-capability-decisions.md §5 设计。
 * 将用户自然语言反馈转换为结构化修改计划、Prompt Patch 和参数补丁。
 */

import { z } from "zod";
import {
  hasControlCharacters,
  invokeNative,
  isNativeRuntimeAvailable,
  NativeCommandError,
} from "./native";

// ═══════════════════════════════════════════════════
// 枚举
// ═══════════════════════════════════════════════════

const editRequestStatusSchema = z.enum([
  "received",
  "analyzing",
  "plan_ready",
  "applied",
  "rejected",
  "ambiguous",
]);
const editContextTypeSchema = z.enum([
  "generation_result",
  "storyboard",
  "character_design",
  "visual_spec",
  "script",
  "deliverable",
]);
const editOperationTypeSchema = z.enum([
  "style_adjustment",
  "character_adjustment",
  "composition_change",
  "lighting_adjustment",
  "color_grading",
  "camera_change",
  "content_revision",
  "quality_improvement",
  "regenerate",
]);
const editPlanStatusSchema = z.enum(["draft", "ready", "executing", "executed", "rejected"]);
const impactLevelSchema = z.enum(["low", "medium", "high"]);

// ═══════════════════════════════════════════════════
// 子结构
// ═══════════════════════════════════════════════════

const editTargetSchema = z
  .object({
    type: z.string().min(1).max(40),
    id: z.string().min(1),
    operation: z.string().min(1).max(80),
  })
  .strict();

const promptPatchSchema = z
  .object({
    add: z.array(z.string().min(1).max(500)).optional(),
    remove: z.array(z.string().min(1).max(500)).optional(),
    replace: z.record(z.string(), z.string().min(1).max(500)).optional(),
  })
  .strict();

const parameterPatchSchema = z
  .object({
    model: z.string().min(1).max(120).optional(),
    steps: z.number().int().positive().optional(),
    guidance: z.number().positive().optional(),
    seed: z.number().int().optional(),
  })
  .passthrough(); // 允许扩展字段 (flatten)

const editUnderstandingResultSchema = z
  .object({
    feedback: z.string().min(1).max(2_000),
    intent: z.string().min(1).max(80),
    subIntent: z.string().min(1).max(80).optional(),
    confidence: z.number().min(0).max(1),
    targets: z.array(editTargetSchema),
    promptPatch: promptPatchSchema.optional(),
    parameterPatch: parameterPatchSchema.optional(),
    requiresCriticRerun: z.boolean(),
    risk: impactLevelSchema,
  })
  .strict();

const editRequestRecordSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    runId: z.string().min(1).nullable(),
    feedbackText: z.string().min(1).max(2_000),
    contextType: editContextTypeSchema,
    contextRefId: z.string().min(1).nullable(),
    sourceReviewId: z.string().min(1).nullable(),
    intentJson: z.string().min(1).nullable(),
    status: editRequestStatusSchema,
    ambiguousReason: z.string().min(1).max(1_000).nullable(),
    createdBy: z.string().min(1),
    createdAt: z.string().min(1),
    resolvedAt: z.string().min(1).nullable(),
  })
  .strict();

const editPlanRecordSchema = z
  .object({
    id: z.string().min(1),
    editRequestId: z.string().min(1),
    projectId: z.string().min(1),
    planSummary: z.string().min(1).max(500),
    operationType: editOperationTypeSchema,
    scope: z.enum(["whole", "partial"]),
    targetsJson: z.string().min(1),
    promptPatchJson: z.string().min(1).nullable(),
    parameterPatchJson: z.string().min(1).nullable(),
    referenceAssetPatchJson: z.string().min(1).nullable(),
    requiresRegeneration: z.boolean(),
    requiresCriticRerun: z.boolean(),
    estimatedImpact: impactLevelSchema.nullable(),
    riskLevel: impactLevelSchema.nullable(),
    status: editPlanStatusSchema,
    executionResultJson: z.string().min(1).nullable(),
    createdBy: z.string().min(1),
    createdAt: z.string().min(1),
    executedAt: z.string().min(1).nullable(),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 输入 Schema
// ═══════════════════════════════════════════════════

export const submitEditFeedbackRequestSchema = z
  .object({
    projectId: z.string().min(1).uuid(),
    runId: z.string().min(1).uuid().optional(),
    feedbackText: z
      .string()
      .trim()
      .min(1, "反馈文本不能为空。")
      .max(2_000, "反馈文本不能超过 2000 个字符。")
      .refine((value) => !hasControlCharacters(value), {
        message: "反馈文本包含不支持的控制字符。",
      }),
    contextType: editContextTypeSchema,
    contextRefId: z.string().min(1).uuid().optional(),
    sourceReviewId: z.string().min(1).uuid().optional(),
  })
  .strict();

export const applyEditPlanRequestSchema = z
  .object({
    planId: z.string().min(1).uuid(),
    approveHighCost: z.boolean().optional(),
  })
  .strict();

export const skipEditRequestSchema = z
  .object({
    requestId: z.string().min(1).uuid(),
    reason: z.string().min(1).max(500).optional(),
  })
  .strict();

export const listEditRequestsRequestSchema = z
  .object({
    projectId: z.string().min(1).uuid(),
    status: editRequestStatusSchema.optional(),
    limit: z.number().int().min(1).max(500).optional(),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type EditRequestStatus = z.infer<typeof editRequestStatusSchema>;
export type EditContextType = z.infer<typeof editContextTypeSchema>;
export type EditOperationType = z.infer<typeof editOperationTypeSchema>;
export type EditPlanStatus = z.infer<typeof editPlanStatusSchema>;
export type ImpactLevel = z.infer<typeof impactLevelSchema>;

export type EditTarget = z.infer<typeof editTargetSchema>;
export type PromptPatch = z.infer<typeof promptPatchSchema>;
export type ParameterPatch = z.infer<typeof parameterPatchSchema>;
export type EditUnderstandingResult = z.infer<typeof editUnderstandingResultSchema>;
export type EditRequestRecord = z.infer<typeof editRequestRecordSchema>;
export type EditPlanRecord = z.infer<typeof editPlanRecordSchema>;

export type SubmitEditFeedbackRequest = z.infer<typeof submitEditFeedbackRequestSchema>;
export type ApplyEditPlanRequest = z.infer<typeof applyEditPlanRequestSchema>;
export type SkipEditRequest = z.infer<typeof skipEditRequestSchema>;

export { NativeCommandError as EditCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isEditRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 提交用户自然语言反馈。
 * 返回创建的编辑请求记录（状态为 "received"）。
 * 后端会异步调用 Edit Understanding Agent 解析意图并生成编辑计划。
 */
export async function submitEditFeedback(
  request: SubmitEditFeedbackRequest,
): Promise<EditRequestRecord> {
  return invokeNative("edit_v1_submit_feedback", editRequestRecordSchema, { request });
}

/**
 * 列出项目的所有编辑请求。
 */
export async function listEditRequests(
  projectId: string,
  options: {
    status?: EditRequestStatus;
    limit?: number;
  } = {},
): Promise<EditRequestRecord[]> {
  return invokeNative("edit_v1_list_requests", z.array(editRequestRecordSchema), {
    request: {
      projectId: z.string().uuid().parse(projectId),
      status: options.status,
      limit: options.limit ?? 50,
    },
  });
}

/**
 * 获取单条编辑请求。
 */
export async function getEditRequest(requestId: string): Promise<EditRequestRecord> {
  return invokeNative("edit_v1_get_request", editRequestRecordSchema, {
    requestId: z.string().uuid().parse(requestId),
  });
}

/**
 * 获取编辑请求对应的编辑计划。
 */
export async function getEditPlanByRequest(requestId: string): Promise<EditPlanRecord | null> {
  return invokeNative("edit_v1_get_plan_by_request", editPlanRecordSchema.nullable(), {
    requestId: z.string().uuid().parse(requestId),
  });
}

/**
 * 执行编辑计划。
 * 将 prompt_patch 和 parameter_patch 应用到原 Prompt 并创建新的生成任务。
 * 如果 approveHighCost = true，跳过成本确认。
 */
export async function applyEditPlan(request: ApplyEditPlanRequest): Promise<EditPlanRecord> {
  return invokeNative("edit_v1_apply_plan", editPlanRecordSchema, { request });
}

/**
 * 跳过/拒绝一条编辑请求。
 */
export async function skipEditRequest(request: SkipEditRequest): Promise<EditRequestRecord> {
  return invokeNative("edit_v1_skip_request", editRequestRecordSchema, { request });
}

/**
 * 获取编辑请求的 Edit Understanding Agent 解析结果。
 * 返回语义理解结果（intent、targets、prompt_patch 等）。
 * 如果 Agent 尚未解析完成，返回 null。
 */
export async function getEditUnderstandingResult(
  requestId: string,
): Promise<EditUnderstandingResult | null> {
  return invokeNative(
    "edit_v1_get_understanding_result",
    editUnderstandingResultSchema.nullable(),
    { requestId: z.string().uuid().parse(requestId) },
  );
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/**
 * 将编辑操作类型翻译为用户可读的中文标签。
 */
export function editOperationTypeLabel(operation: EditOperationType): string {
  const labels: Record<EditOperationType, string> = {
    style_adjustment: "风格调整",
    character_adjustment: "人物调整",
    composition_change: "构图变更",
    lighting_adjustment: "光影调整",
    color_grading: "色彩调色",
    camera_change: "镜头变更",
    content_revision: "内容修正",
    quality_improvement: "质量提升",
    regenerate: "重新生成",
  };
  return labels[operation] ?? operation;
}

/**
 * 将编辑请求状态翻译为用户可读的中文标签。
 */
export function editRequestStatusLabel(status: EditRequestStatus): string {
  const labels: Record<EditRequestStatus, string> = {
    received: "已收到",
    analyzing: "分析中",
    plan_ready: "修改计划就绪",
    applied: "已应用",
    rejected: "已拒绝",
    ambiguous: "无法理解",
  };
  return labels[status] ?? status;
}

/**
 * 将编辑计划状态翻译为用户可读的中文标签。
 */
export function editPlanStatusLabel(status: EditPlanStatus): string {
  const labels: Record<EditPlanStatus, string> = {
    draft: "草稿",
    ready: "就绪",
    executing: "执行中",
    executed: "已执行",
    rejected: "已拒绝",
  };
  return labels[status] ?? status;
}

/**
 * 预定义的常见反馈模式匹配（客户端快速提示）。
 * 在调用后端 Agent 前，先做关键词匹配给用户即时反馈。
 * 返回匹配到的意图和推荐操作（如果没有匹配返回 null）。
 */
export interface FeedbackQuickHint {
  intent: EditOperationType;
  label: string;
  promptAdd: string[];
  promptRemove: string[];
}

const FEEDBACK_PATTERNS: {
  keywords: string[];
  hint: FeedbackQuickHint;
}[] = [
  {
    keywords: ["太假", "不真实", "CG感", "像动画", "假人"],
    hint: {
      intent: "style_adjustment",
      label: "增加真实感，减少CG感",
      promptAdd: ["photorealistic", "natural texture", "real-world lighting"],
      promptRemove: ["CG render", "perfect symmetry", "plastic skin"],
    },
  },
  {
    keywords: ["不够大气", "不够高级", "太小气"],
    hint: {
      intent: "composition_change",
      label: "提升空间尺度和质感",
      promptAdd: ["grand scale", "wide angle lens", "negative space"],
      promptRemove: ["close-up framing", "crowded composition"],
    },
  },
  {
    keywords: ["不像纪录片", "不是央视", "太网红", "不像纪实"],
    hint: {
      intent: "style_adjustment",
      label: "加强纪实风格，减少网红感",
      promptAdd: ["documentary cinematography", "natural handheld", "muted color"],
      promptRemove: ["glamour lighting", "influencer style", "beauty filter"],
    },
  },
  {
    keywords: ["不像创业者", "太像模特", "太网红"],
    hint: {
      intent: "character_adjustment",
      label: "调整人物特征，让角色更像目标人物",
      promptAdd: ["ordinary person", "natural expression", "workplace attire"],
      promptRemove: ["fashion model", "perfect skin", "glamour pose"],
    },
  },
  {
    keywords: ["太冷", "太暖", "颜色不对", "色调"],
    hint: {
      intent: "color_grading",
      label: "调整色彩温度平衡",
      promptAdd: ["balanced color temperature", "natural color palette"],
      promptRemove: ["extreme color cast", "oversaturated"],
    },
  },
  {
    keywords: ["节奏", "太慢", "太快", "拖沓"],
    hint: {
      intent: "camera_change",
      label: "调整镜头节奏",
      promptAdd: ["dynamic pacing", "appropriate tempo"],
      promptRemove: ["static camera", "rapid cuts"],
    },
  },
];

/**
 * 在客户端对用户反馈做关键词快速匹配。
 * 用于在调用后端 Agent 前给用户即时视觉反馈。
 */
export function matchFeedbackQuickHint(feedback: string): FeedbackQuickHint | null {
  const lower = feedback.toLowerCase();
  for (const pattern of FEEDBACK_PATTERNS) {
    for (const keyword of pattern.keywords) {
      if (lower.includes(keyword)) {
        return pattern.hint;
      }
    }
  }
  return null;
}
