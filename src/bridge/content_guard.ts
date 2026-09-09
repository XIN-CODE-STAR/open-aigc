/**
 * AI Critic Agent — 内容安全检查桥接层。
 *
 * 封装 Content Guard Agent 的前端调用，包括：
 * - 预定义规则库的客户端快速检查（节省 IPC 往返）
 * - 提交内容进行完整安全检查
 * - 导出前合规状态汇总
 */

import { z } from "zod";
import { invokeNative } from "./native";

// ═══════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════

const contentGuardStatusSchema = z.enum(["passed", "needs_review", "flagged", "blocked"]);
const contentRiskLevelSchema = z.enum(["low", "medium", "high", "critical"]);

export const guardCheckSchema = z
  .object({
    category: z.string().min(1),
    status: z.enum(["pass", "warn", "flag", "block"]),
    message: z.string().min(1),
    detail: z.string().optional(),
  })
  .strict();

export const guardActionsSchema = z
  .object({
    allowed: z.array(z.string()),
    blocked: z.array(z.string()),
  })
  .strict();

const contentGuardReportSchema = z
  .object({
    id: z.string().min(1),
    projectId: z.string().min(1),
    targetType: z.enum(["user_prompt", "generated_prompt", "asset", "deliverable", "script"]),
    targetId: z.string().min(1).nullable(),
    taskId: z.string().min(1).nullable(),
    assetId: z.string().min(1).nullable(),
    guardVersion: z.string().min(1).nullable(),
    guardProvider: z.string().min(1).nullable(),
    status: contentGuardStatusSchema,
    riskLevel: contentRiskLevelSchema,
    checksJson: z.string().min(1),
    actionsJson: z.string().min(1),
    createdAt: z.string().min(1),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type ContentGuardStatus = z.infer<typeof contentGuardStatusSchema>;
export type ContentRiskLevel = z.infer<typeof contentRiskLevelSchema>;
export type GuardCheck = z.infer<typeof guardCheckSchema>;
export type GuardActions = z.infer<typeof guardActionsSchema>;
export type ContentGuardReport = z.infer<typeof contentGuardReportSchema>;

// ═══════════════════════════════════════════════════
// 预定义关键词规则库（客户端快速检查）
// ═══════════════════════════════════════════════════

interface KeywordRule {
  category: string;
  keywords: string[];
  severity: "warn" | "flag" | "block";
  messageTemplate: string;
}

const KEYWORD_RULES: KeywordRule[] = [
  {
    category: "political_sensitivity",
    keywords: ["革命", "推翻", "暴力", "恐怖", "独裁", "分裂", "颠覆", "叛国", "邪教"],
    severity: "block",
    messageTemplate: "检测到敏感政治关键词：{keywords}",
  },
  {
    category: "nsfw_content",
    keywords: ["色情", "暴力", "血腥", "裸露", "毒品", "赌场"],
    severity: "flag",
    messageTemplate: "检测到敏感内容关键词：{keywords}",
  },
  {
    category: "brand_consistency",
    keywords: ["山寨", "便宜", "劣质", "假冒", "侵权"],
    severity: "warn",
    messageTemplate: "检测到品牌负面词汇：{keywords}，请确认是否合适。",
  },
  {
    category: "educational_safety",
    keywords: ["作弊", "逃课", "打架", "霸凌", "自杀"],
    severity: "warn",
    messageTemplate: "教育场景检测到风险词汇：{keywords}，已警告。",
  },
];

// ═══════════════════════════════════════════════════
// 客户端规则检查
// ═══════════════════════════════════════════════════

/**
 * 在客户端对文本内容做快速关键词安全检查。
 * 用于提交前预检，减少不必要的 IPC 往返。
 * 返回命中的规则列表（空数组表示通过）。
 */
export function quickContentCheck(text: string): GuardCheck[] {
  const lower = text.toLowerCase();
  const checks: GuardCheck[] = [];

  for (const rule of KEYWORD_RULES) {
    const hits = rule.keywords.filter((kw) => lower.includes(kw));
    if (hits.length > 0) {
      checks.push({
        category: rule.category,
        status: rule.severity === "block" ? "block" : rule.severity === "flag" ? "flag" : "warn",
        message: rule.messageTemplate.replace("{keywords}", hits.join(", ")),
        detail: `命中关键词：${hits.join(", ")}`,
      });
    }
  }

  return checks;
}

/**
 * 从客户端检查结果推导整体安全状态。
 */
export function deriveStatusFromChecks(checks: GuardCheck[]): {
  status: ContentGuardStatus;
  riskLevel: ContentRiskLevel;
} {
  if (checks.length === 0) {
    return { status: "passed", riskLevel: "low" };
  }

  const hasBlock = checks.some((c) => c.status === "block");
  const hasFlag = checks.some((c) => c.status === "flag");
  const hasWarn = checks.some((c) => c.status === "warn");

  if (hasBlock) {
    return { status: "blocked", riskLevel: "critical" };
  }
  if (hasFlag) {
    return { status: "flagged", riskLevel: "high" };
  }
  if (hasWarn) {
    return { status: "needs_review", riskLevel: "medium" };
  }
  return { status: "passed", riskLevel: "low" };
}

// ═══════════════════════════════════════════════════
// 导出前合规状态汇总
// ═══════════════════════════════════════════════════

/**
 * 导出前检查结果——汇总所有资产的合规状态。
 */
export interface ExportComplianceSummary {
  /** 是否可以安全导出。 */
  canExport: boolean;
  /** 导出阻塞原因（如果有）。 */
  blockReasons: string[];
  /** 需要人工复核的资产数。 */
  needsReviewCount: number;
  /** 已标记风险的资产数。 */
  flaggedCount: number;
  /** 已阻断的资产数。 */
  blockedCount: number;
}

/**
 * 对资产列表做导出前合规检查。
 * 汇总所有资产的版权和内容安全状态。
 */
export function assessExportReadiness(
  licenses: {
    assetId: string;
    commercialUseStatus: string;
    exportAllowed: boolean;
    exportBlockReason?: string | null;
    reviewRequired: boolean;
  }[],
  guardReports: {
    targetId?: string | null;
    status: ContentGuardStatus;
    riskLevel: ContentRiskLevel;
  }[],
): ExportComplianceSummary {
  const blockReasons: string[] = [];
  let needsReviewCount = 0;
  let flaggedCount = 0;
  let blockedCount = 0;

  // 检查版权
  for (const lic of licenses) {
    if (!lic.exportAllowed) {
      blockReasons.push(`资产 ${lic.assetId}: ${lic.exportBlockReason || "导出被禁止"}`);
      blockedCount++;
    }
    if (lic.commercialUseStatus === "needs_review" || lic.commercialUseStatus === "restricted") {
      needsReviewCount++;
    }
    if (lic.commercialUseStatus === "blocked") {
      blockedCount++;
      blockReasons.push(`资产 ${lic.assetId}: 商用状态为 blocked`);
    }
  }

  // 检查内容安全
  for (const guard of guardReports) {
    if (guard.status === "blocked") {
      blockedCount++;
      blockReasons.push(`内容 (${guard.targetId || "unknown"}): 安全状态为 blocked`);
    } else if (guard.status === "flagged") {
      flaggedCount++;
    } else if (guard.status === "needs_review") {
      needsReviewCount++;
    }
  }

  return {
    canExport: blockReasons.length === 0 && needsReviewCount === 0,
    blockReasons,
    needsReviewCount,
    flaggedCount,
    blockedCount,
  };
}

// ═══════════════════════════════════════════════════
// IPC 调用
// ═══════════════════════════════════════════════════

/**
 * 对目标内容执行完整的内容安全检查（调用后端规则引擎）。
 */
export async function runContentGuardCheck(
  projectId: string,
  targetType: "user_prompt" | "generated_prompt" | "asset" | "deliverable" | "script",
  targetId: string,
  contentText?: string,
): Promise<ContentGuardReport> {
  return invokeNative("content_guard_v1_get_report", contentGuardReportSchema, {
    projectId,
    targetType,
    targetId,
    contentText,
  });
}

/**
 * 获取目标实体的最新安全检查报告。
 */
export async function getLatestGuardReport(
  targetType: "user_prompt" | "generated_prompt" | "asset" | "deliverable" | "script",
  targetId: string,
): Promise<ContentGuardReport | null> {
  return invokeNative("content_guard_v1_get_report", contentGuardReportSchema.nullable(), {
    targetType,
    targetId,
  });
}
