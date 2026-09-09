import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError, voidResponse } from "./native";

/**
 * Phase 3 扩展：Provider 认证模式。
 * 旧记录（API Key 模式）默认归类为 "api_key"。
 */
const providerAuthModeSchema = z
  .enum(["api_key", "oauth", "account_login", "session_cookie", "manual"])
  .default("api_key");

/**
 * Phase 3 扩展：Provider 账号状态。
 * 旧记录默认归类为 "ok"（健康）。
 */
const providerAccountStatusSchema = z
  .enum(["ok", "degraded", "expired", "failed", "unknown"])
  .default("ok");

const credentialTypeSchema = z
  .enum([
    "api_key",
    "access_secret",
    "oauth_token",
    "session_cookie",
    "browser_session",
    "local_endpoint",
  ])
  .default("api_key");

const credentialScopeSchema = z.enum(["user", "workspace", "system"]).default("user");

const credentialRecordSchema = z
  .object({
    id: z.uuid("凭据标识无效。"),
    providerName: z.string().min(1).max(80),
    displayName: z.string().min(1).max(120),
    baseUrl: z.string().min(1).max(500),
    modelName: z.string().min(1).max(120),
    credentialKey: z.string().min(1).max(200),
    enabled: z.boolean(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
    // Phase A 新增：认证类型与作用域（后端带 default，始终返回）
    credentialType: credentialTypeSchema.optional(),
    scope: credentialScopeSchema.optional(),
    // Phase 3 扩展字段：旧凭据自动获得默认值
    authMode: providerAuthModeSchema.optional(),
    accountId: z.string().max(200).optional(),
    status: providerAccountStatusSchema.optional(),
    expiresAt: z.string().optional(),
    lastHealthCheckAt: z.string().optional(),
  })
  .strict();

const listCredentialsRequestSchema = z.object({}).strict();

const createCredentialRequestSchema = z
  .object({
    providerName: z
      .string()
      .trim()
      .min(1, "供应商名称不能为空。")
      .max(80, "供应商名称不能超过 80 个字符。"),
    displayName: z
      .string()
      .trim()
      .min(1, "显示名称不能为空。")
      .max(120, "显示名称不能超过 120 个字符。"),
    baseUrl: z
      .string()
      .trim()
      .min(1, "服务地址不能为空。")
      .max(500, "服务地址不能超过 500 个字符。"),
    modelName: z
      .string()
      .trim()
      .min(1, "模型名称不能为空。")
      .max(120, "模型名称不能超过 120 个字符。"),
    apiKey: z
      .string()
      .trim()
      .min(1, "API 密钥不能为空。")
      .max(500, "API 密钥不能超过 500 个字符。"),
    // 认证类型（可选，默认 api_key。账号类传 session_cookie / browser_session）
    credentialType: z.string().trim().max(30).optional(),
  })
  .strict();

const updateCredentialRequestSchema = z
  .object({
    id: z.uuid("凭据标识无效。"),
    providerName: z
      .string()
      .trim()
      .min(1, "供应商名称不能为空。")
      .max(80, "供应商名称不能超过 80 个字符。"),
    displayName: z
      .string()
      .trim()
      .min(1, "显示名称不能为空。")
      .max(120, "显示名称不能超过 120 个字符。"),
    baseUrl: z
      .string()
      .trim()
      .min(1, "服务地址不能为空。")
      .max(500, "服务地址不能超过 500 个字符。"),
    modelName: z
      .string()
      .trim()
      .min(1, "模型名称不能为空。")
      .max(120, "模型名称不能超过 120 个字符。"),
    apiKey: z
      .string()
      .trim()
      .min(1, "API 密钥不能为空。")
      .max(500, "API 密钥不能超过 500 个字符。")
      .optional(),
  })
  .strict();

const deleteCredentialRequestSchema = z
  .object({
    id: z.uuid("凭据标识无效。"),
  })
  .strict();

const credentialListSchema = z.array(credentialRecordSchema);

export type CredentialRecord = z.infer<typeof credentialRecordSchema>;

// Phase 3 扩展：Provider 认证模式与账号状态类型
export type ProviderAuthMode = "api_key" | "oauth" | "account_login" | "session_cookie" | "manual";
export type ProviderAccountStatus = "ok" | "degraded" | "expired" | "failed" | "unknown";

export type CreateCredentialInput = z.input<typeof createCredentialRequestSchema>;
export type UpdateCredentialInput = z.input<typeof updateCredentialRequestSchema>;

export { NativeCommandError as CredentialCommandError };

/**
 * 列出所有已配置的供应商凭据。密钥本身不会返回，只返回 keychain 引用键。
 */
export async function listCredentials(): Promise<CredentialRecord[]> {
  const request = parseRequest(listCredentialsRequestSchema, {}, "会话令牌无效。");
  return invokeNative("credential_v1_list", credentialListSchema, { request });
}

/**
 * 新建一条供应商凭据。API 密钥会写入 OS keychain，数据库只保存引用键。
 */
export async function createCredential(input: CreateCredentialInput): Promise<CredentialRecord> {
  const request = parseRequest(createCredentialRequestSchema, { ...input }, "凭据信息无效。");
  return invokeNative("credential_v1_create", credentialRecordSchema, { request });
}

/**
 * 更新凭据元数据。apiKey 留空时保留原有密钥；提供新值时覆盖 keychain 中的密钥。
 */
export async function updateCredential(input: UpdateCredentialInput): Promise<CredentialRecord> {
  const request = parseRequest(updateCredentialRequestSchema, { ...input }, "凭据更新信息无效。");
  return invokeNative("credential_v1_update", credentialRecordSchema, { request });
}

/**
 * 软删一条凭据，同时尝试从 OS keychain 中移除密钥。
 */
export async function deleteCredential(id: string): Promise<void> {
  const request = parseRequest(deleteCredentialRequestSchema, { id }, "凭据标识无效。");
  await invokeNative("credential_v1_delete", voidResponse, { request });
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw firstValidationError(result, fallback);
  }
  return result.data;
}
