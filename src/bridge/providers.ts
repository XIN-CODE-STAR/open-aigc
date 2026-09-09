/**
 * Provider 元数据桥接层（Phase 3 架构准备）。
 *
 * 此文件不引入新的 IPC 调用，仅在前端层面对
 * - ProviderAuthMode
 * - ProviderAccountStatus
 * - ProviderCapability
 * - ProviderHealth
 * - ProviderDescriptor
 * 等概念做集中类型定义与工具函数，避免后续模块各自重复。
 *
 * 实际 IPC（listProviders / validateCredential）由后续 PR 引入，
 * 不会破坏现有凭据管理 UI。
 */

import type { ProviderAccountStatus, ProviderAuthMode } from "./credentials";

// Re-export 让调用方统一从 `./providers` 引入
export type { ProviderAccountStatus, ProviderAuthMode } from "./credentials";

/** Provider 能力种类。 */
export type ProviderCapabilityKind =
  | "text_to_image"
  | "image_to_image"
  | "text_to_video"
  | "image_to_video"
  | "text_to_speech"
  | "voice_clone"
  | "agent_loop"
  | "digital_human";

/** Provider 单项能力定义。 */
export interface ProviderCapability {
  kind: ProviderCapabilityKind;
  /** 支持的模型列表（空数组表示任意）。 */
  models: string[];
  /** 是否支持参考图/参考素材。 */
  supportsReference: boolean;
}

/** Provider 健康快照（前端视图模型）。 */
export interface ProviderHealthSnapshot {
  status: ProviderAccountStatus;
  message?: string;
  lastCheckedAt?: string;
  refreshInSeconds?: number;
}

/** Provider 描述符（前端 UI 用）。 */
export interface ProviderDescriptor {
  providerId: string;
  displayName: string;
  authModes: ProviderAuthMode[];
  capabilities: ProviderCapability[];
}

/**
 * 解析凭据所属 Provider 的"最可能"认证模式。
 * 当前阶段仅基于 providerName 启发式映射；后续将来自后端 ProviderDescriptor。
 */
export function inferAuthMode(providerName: string): ProviderAuthMode {
  const lower = providerName.toLowerCase();
  if (lower.includes("grok") || lower.includes("xai")) return "oauth";
  if (lower.includes("seedance") || lower.includes("kling") || lower.includes("hailuo")) {
    return "api_key";
  }
  return "api_key";
}

/** Provider 账号状态指示色（用于 UI 状态点）。 */
export function statusColor(status: ProviderAccountStatus | undefined): string {
  switch (status) {
    case "ok":
      return "#22c55e";
    case "degraded":
      return "#f59e0b";
    case "expired":
      return "#f97316";
    case "failed":
      return "#ef4444";
    case "unknown":
    default:
      return "#94a3b8";
  }
}

/** Provider 账号状态的中文描述。 */
export function statusLabel(status: ProviderAccountStatus | undefined): string {
  switch (status) {
    case "ok":
      return "在线";
    case "degraded":
      return "受限";
    case "expired":
      return "已过期";
    case "failed":
      return "不可用";
    case "unknown":
    default:
      return "未知";
  }
}

/** 凭据是否仍可发起请求。 */
export function isAccountAvailable(status: ProviderAccountStatus | undefined): boolean {
  return status === "ok" || status === "degraded";
}
