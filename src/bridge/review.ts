/**
 * AI Critic Agent — 前端桥接层。
 *
 * 按照 2026-07-20-creative-agent-critical-capability-decisions.md §4 设计。
 * 定义多维评价体系的 Zod Schema、TypeScript 类型和 IPC 调用函数。
 */

import { z } from "zod";
import { invokeNative, isNativeRuntimeAvailable, NativeCommandError } from "./native";

// ═══════════════════════════════════════════════════
// 枚举
// ═══════════════════════════════════════════════════

const reviewerTypeSchema = z.enum(["auto", "manual", "hybrid"]);
const reviewDecisionSchema = z.enum([
  "needs_review",
  "accept",
  "accept_with_suggestions",
  "revise",
  "regenerate",
  "block",
]);
const issueSeveritySchema = z.enum(["low", "medium", "high", "critical"]);
const dimensionLayerSchema = z.enum([
  "requirement",
  "visual",
  "content",
  "commercial",
  "technical",
]);
const assetVersionStatusSchema = z.enum([
  "draft",
  "reviewing",
  "approved",
  "selected",
  "used_in_deliverable",
  "archived",
  "deprecated",
  "rejected",
]);
const commercialUseStatusSchema = z.enum([
  "clear",
  "needs_review",
  "restricted",
  "blocked",
  "unknown",
]);
const contentGuardStatusSchema = z.enum(["passed", "needs_review", "flagged", "blocked"]);
const contentRiskLevelSchema = z.enum(["low", "medium", "high", "critical"]);

// ═══════════════════════════════════════════════════
// 子结构
// ═══════════════════════════════════════════════════

export const reviewIssueSchema = z
  .object({
    dimension: z.string().min(1).max(80),
    severity: issueSeveritySchema,
    message: z.string().min(1).max(1_000),
    suggestedFix: z.string().min(1).max(1_000),
  })
  .strict();

export const requirementScoresSchema = z
  .object({
    match: z.number().min(0).max(100).optional(),
    completeness: z.number().min(0).max(100).optional(),
    clarity: z.number().min(0).max(100).optional(),
  })
  .strict();

export const visualScoresSchema = z
  .object({
    composition: z.number().min(0).max(100).optional(),
    color: z.number().min(0).max(100).optional(),
    lighting: z.number().min(0).max(100).optional(),
    texture: z.number().min(0).max(100).optional(),
    lensLanguage: z.number().min(0).max(100).optional(),
  })
  .strict();

export const contentScoresSchema = z
  .object({
    themeMatch: z.number().min(0).max(100).optional(),
    emotionExpression: z.number().min(0).max(100).optional(),
    narrativePurpose: z.number().min(0).max(100).optional(),
  })
  .strict();

export const commercialScoresSchema = z
  .object({
    platformFit: z.number().min(0).max(100).optional(),
    audienceFit: z.number().min(0).max(100).optional(),
    conversionPotential: z.number().min(0).max(100).optional(),
  })
  .strict();

export const technicalScoresSchema = z
  .object({
    clarity: z.number().min(0).max(100).optional(),
    distortion: z.number().min(0).max(100).optional(),
    characterConsistency: z.number().min(0).max(100).optional(),
    motionQuality: z.number().min(0).max(100).optional(),
  })
  .strict();

const reviewDimensionRecordSchema = z
  .object({
    id: z.string().min(1),
    reviewId: z.string().min(1),
    dimensionLayer: dimensionLayerSchema,
    dimensionName: z.string().min(1).max(60),
    score: z.number().min(0).max(100),
    weight: z.number().positive().max(5),
    confidence: z.number().min(0).max(1).nullable(),
    reasoning: z.string().min(1).max(2_000).nullable(),
    referenceContextJson: z.string().min(1),
    createdAt: z.string().min(1),
  })
  .strict();

const reviewReportRecordSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    runId: z.string().min(1).nullable(),
    shotId: z.string().min(1).nullable(),
    assetId: z.string().min(1).nullable(),
    generationAttemptId: z.string().min(1).nullable(),
    reviewerType: reviewerTypeSchema,
    reviewerAgentVersion: z.string().min(1).max(80).nullable(),
    reviewerProvider: z.string().min(1).max(80).nullable(),

    // 五层评分（JSON 字符串）
    requirementScoresJson: z.string().min(1),
    visualScoresJson: z.string().min(1),
    contentScoresJson: z.string().min(1),
    commercialScoresJson: z.string().min(1),
    technicalScoresJson: z.string().min(1),

    overallScore: z.number().min(0).max(100),
    weightedScore: z.number().min(0).max(100).nullable(),

    issuesJson: z.string().min(1),
    decision: reviewDecisionSchema,
    confidence: z.number().min(0).max(1).nullable(),
    sourceTaskId: z.string().min(1).nullable(),

    reviewVersion: z.number().int().positive(),
    createdAt: z.string().min(1),
  })
  .strict();

const assetVersionRecordSchema = z
  .object({
    id: z.string().min(1),
    assetId: z.string().min(1),
    version: z.number().int().positive(),
    storageKey: z.string().min(1),
    mimeType: z.string().min(1),
    sizeBytes: z.number().int().nonnegative(),
    hash: z.string().min(1).max(64).nullable(),
    width: z.number().int().positive().nullable(),
    height: z.number().int().positive().nullable(),
    durationSeconds: z.number().positive().nullable(),
    sourceType: z.string().min(1),
    sourceTaskId: z.string().min(1).nullable(),
    sourceAttemptId: z.string().min(1).nullable(),
    status: assetVersionStatusSchema,
    statusReason: z.string().min(1).max(1_000).nullable(),
    reviewId: z.string().min(1).nullable(),
    createdBy: z.string().min(1),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

const assetLicenseRecordSchema = z
  .object({
    id: z.string().min(1),
    assetId: z.string().min(1),
    assetVersionId: z.string().min(1).nullable(),
    sourceType: z.enum(["ai_generated", "user_uploaded", "third_party", "derived"]),
    providerId: z.string().min(1).max(80).nullable(),
    modelName: z.string().min(1).max(120).nullable(),
    commercialUseStatus: commercialUseStatusSchema,
    commercialUseDetails: z.string().min(1).max(2_000).nullable(),
    sourceAssetsJson: z.string().min(1).nullable(),
    copyrightStatement: z.string().min(1).max(1_000).nullable(),
    attributionRequired: z.boolean(),
    riskFlagsJson: z.string().min(1).nullable(),
    reviewRequired: z.boolean(),
    reviewNotes: z.string().min(1).max(1_000).nullable(),
    exportAllowed: z.boolean(),
    exportBlockReason: z.string().min(1).max(500).nullable(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

const contentGuardReportRecordSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    targetType: z.enum(["user_prompt", "generated_prompt", "asset", "deliverable", "script"]),
    targetId: z.string().min(1).nullable(),
    taskId: z.string().min(1).nullable(),
    assetId: z.string().min(1).nullable(),
    guardVersion: z.string().min(1).max(80).nullable(),
    guardProvider: z.string().min(1).max(80).nullable(),
    status: contentGuardStatusSchema,
    riskLevel: contentRiskLevelSchema,
    checksJson: z.string().min(1),
    actionsJson: z.string().min(1),
    createdAt: z.string().min(1),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 前端查询请求 Schema
// ═══════════════════════════════════════════════════

export const listReviewReportsRequestSchema = z
  .object({
    projectId: z.string().min(1).uuid(),
    shotId: z.string().min(1).uuid().optional(),
    assetId: z.string().min(1).uuid().optional(),
    decision: reviewDecisionSchema.optional(),
    limit: z.number().int().min(1).max(500).optional(),
  })
  .strict();

export const listAssetVersionsRequestSchema = z
  .object({
    assetId: z.string().min(1).uuid(),
    limit: z.number().int().min(1).max(100).optional(),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type ReviewerType = z.infer<typeof reviewerTypeSchema>;
export type ReviewDecision = z.infer<typeof reviewDecisionSchema>;
export type IssueSeverity = z.infer<typeof issueSeveritySchema>;
export type DimensionLayer = z.infer<typeof dimensionLayerSchema>;
export type AssetVersionStatus = z.infer<typeof assetVersionStatusSchema>;
export type CommercialUseStatus = z.infer<typeof commercialUseStatusSchema>;
export type ContentGuardStatus = z.infer<typeof contentGuardStatusSchema>;
export type ContentRiskLevel = z.infer<typeof contentRiskLevelSchema>;

export type ReviewIssue = z.infer<typeof reviewIssueSchema>;
export type RequirementScores = z.infer<typeof requirementScoresSchema>;
export type VisualScores = z.infer<typeof visualScoresSchema>;
export type ContentScores = z.infer<typeof contentScoresSchema>;
export type CommercialScores = z.infer<typeof commercialScoresSchema>;
export type TechnicalScores = z.infer<typeof technicalScoresSchema>;

export type ReviewDimensionRecord = z.infer<typeof reviewDimensionRecordSchema>;
export type ReviewReportRecord = z.infer<typeof reviewReportRecordSchema>;
export type AssetVersionRecord = z.infer<typeof assetVersionRecordSchema>;
export type AssetLicenseRecord = z.infer<typeof assetLicenseRecordSchema>;
export type ContentGuardReportRecord = z.infer<typeof contentGuardReportRecordSchema>;

export { NativeCommandError as ReviewCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isReviewRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 获取项目的所有评价报告。
 */
export async function listReviewReports(
  projectId: string,
  options: {
    shotId?: string;
    assetId?: string;
    decision?: ReviewDecision;
    limit?: number;
  } = {},
): Promise<ReviewReportRecord[]> {
  const request = {
    projectId,
    ...options,
    limit: options.limit ?? 50,
  };
  return invokeNative("review_v1_list_reports", z.array(reviewReportRecordSchema), { request });
}

/**
 * 获取单条评价报告。
 */
export async function getReviewReport(reportId: string): Promise<ReviewReportRecord> {
  return invokeNative("review_v1_get_report", reviewReportRecordSchema, {
    reportId: z.string().uuid().parse(reportId),
  });
}

/**
 * 获取评价报告的所有维度详情。
 */
export async function listReviewDimensions(reviewId: string): Promise<ReviewDimensionRecord[]> {
  return invokeNative("review_v1_list_dimensions", z.array(reviewDimensionRecordSchema), {
    reviewId: z.string().uuid().parse(reviewId),
  });
}

/**
 * 获取资产的版本历史。
 */
export async function listAssetVersions(
  assetId: string,
  limit?: number,
): Promise<AssetVersionRecord[]> {
  return invokeNative("asset_v1_list_versions", z.array(assetVersionRecordSchema), {
    assetId: z.string().uuid().parse(assetId),
    limit: limit ?? 50,
  });
}

/**
 * 获取资产的版权信息。
 */
export async function getAssetLicense(assetId: string): Promise<AssetLicenseRecord | null> {
  return invokeNative("asset_v1_get_license", assetLicenseRecordSchema.nullable(), {
    assetId: z.string().uuid().parse(assetId),
  });
}

/**
 * 获取内容安全检查报告。
 */
export async function getContentGuardReport(
  targetType: "user_prompt" | "generated_prompt" | "asset" | "deliverable" | "script",
  targetId: string,
): Promise<ContentGuardReportRecord | null> {
  return invokeNative("content_guard_v1_get_report", contentGuardReportRecordSchema.nullable(), {
    targetType: z
      .enum(["user_prompt", "generated_prompt", "asset", "deliverable", "script"])
      .parse(targetType),
    targetId: z.string().uuid().parse(targetId),
  });
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/**
 * 将评价决策翻译为用户可读的中文标签。
 */
export function reviewDecisionLabel(decision: ReviewDecision): string {
  switch (decision) {
    case "needs_review":
      return "待审核";
    case "accept":
      return "可直接采纳";
    case "accept_with_suggestions":
      return "可采纳（有优化建议）";
    case "revise":
      return "建议局部修改";
    case "regenerate":
      return "建议重新生成";
    case "block":
      return "阻断进入下一阶段";
  }
}

/**
 * 将问题严重程度翻译为中文标签。
 */
export function issueSeverityLabel(severity: IssueSeverity): string {
  switch (severity) {
    case "low":
      return "轻微";
    case "medium":
      return "中等";
    case "high":
      return "严重";
    case "critical":
      return "严重阻断";
  }
}

/**
 * 视频生成前的关键帧质量门槛（与 Rust VideoGenerationGate 保持一致）。
 */
export const VIDEO_GENERATION_GATE = {
  overallMin: 80,
  characterConsistencyMin: 80,
  styleConsistencyMin: 80,
  requirementMatchMin: 80,
} as const;

/**
 * 检查评价报告是否通过视频生成前的质量门槛。
 * 在 UI 层做客户端预检查，避免浪费调用。
 */
export function passesVideoGenerationGate(report: ReviewReportRecord): boolean {
  if (report.decision !== "accept" && report.decision !== "accept_with_suggestions") return false;

  if (report.overallScore < VIDEO_GENERATION_GATE.overallMin) return false;

  try {
    const tech = JSON.parse(report.technicalScoresJson) as TechnicalScores;
    if ((tech.characterConsistency ?? 0) < VIDEO_GENERATION_GATE.characterConsistencyMin)
      return false;
  } catch {
    /* JSON 解析失败，保守放行 */
  }

  try {
    const content = JSON.parse(report.contentScoresJson) as ContentScores;
    if ((content.themeMatch ?? 0) < VIDEO_GENERATION_GATE.styleConsistencyMin) return false;
  } catch {
    /* 同上 */
  }

  try {
    const req = JSON.parse(report.requirementScoresJson) as RequirementScores;
    if ((req.match ?? 0) < VIDEO_GENERATION_GATE.requirementMatchMin) return false;
  } catch {
    /* 同上 */
  }

  return true;
}
