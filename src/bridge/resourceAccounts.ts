import { z } from "zod";

import { invokeNative, NativeCommandError, voidResponse } from "./native";

const resourceAccountRecordSchema = z
  .object({
    id: z.string().min(1),
    providerId: z.string().min(1),
    accountType: z.string().min(1),
    displayName: z.string(),
    status: z.string().min(1),
    credentialKey: z.string().min(1),
    baseUrl: z.string(),
    extraJson: z.string(),
    enabled: z.boolean(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
    lastHealthCheckAt: z.string().nullable().optional(),
  })
  .strict();

const listRequestSchema = z.object({}).strict();

const createRequestSchema = z
  .object({
    providerId: z.string().trim().min(1, "平台标识不能为空。").max(80),
    accountType: z.string().trim().max(30).optional(),
    displayName: z.string().trim().min(1, "显示名称不能为空。").max(120),
    baseUrl: z.string().trim().max(500).optional(),
    sessionSecret: z.string().trim().min(1, "Session 不能为空。").max(8000),
    extraJson: z.string().max(2000).optional(),
  })
  .strict();

const updateRequestSchema = z
  .object({
    id: z.string().min(1),
    displayName: z.string().trim().min(1, "显示名称不能为空。").max(120),
    baseUrl: z.string().trim().max(500).optional(),
    extraJson: z.string().max(2000).optional(),
  })
  .strict();

const updateSessionRequestSchema = z
  .object({
    id: z.string().min(1),
    sessionSecret: z.string().trim().min(1, "Session 不能为空。").max(8000),
  })
  .strict();

const setEnabledRequestSchema = z
  .object({
    id: z.string().min(1),
    enabled: z.boolean(),
  })
  .strict();

const deleteRequestSchema = z
  .object({
    id: z.string().min(1),
  })
  .strict();

const accountListSchema = z.array(resourceAccountRecordSchema);

export type ResourceAccountRecord = z.infer<typeof resourceAccountRecordSchema>;
export type CreateResourceAccountInput = z.input<typeof createRequestSchema>;
export type UpdateResourceAccountInput = z.input<typeof updateRequestSchema>;

export { NativeCommandError as ResourceAccountCommandError };

export async function listResourceAccounts(): Promise<ResourceAccountRecord[]> {
  const request = parseRequest(listRequestSchema, {}, "请求无效。");
  return invokeNative("resource_account_v1_list", accountListSchema, { request });
}

export async function createResourceAccount(
  input: CreateResourceAccountInput,
): Promise<ResourceAccountRecord> {
  const request = parseRequest(createRequestSchema, { ...input }, "账号信息无效。");
  return invokeNative("resource_account_v1_create", resourceAccountRecordSchema, { request });
}

export async function updateResourceAccount(
  input: UpdateResourceAccountInput,
): Promise<ResourceAccountRecord> {
  const request = parseRequest(updateRequestSchema, { ...input }, "更新信息无效。");
  return invokeNative("resource_account_v1_update", resourceAccountRecordSchema, { request });
}

export async function updateAccountSession(id: string, sessionSecret: string): Promise<void> {
  const request = parseRequest(updateSessionRequestSchema, { id, sessionSecret }, "Session 无效。");
  await invokeNative("resource_account_v1_update_session", voidResponse, { request });
}

export async function setAccountEnabled(id: string, enabled: boolean): Promise<void> {
  const request = parseRequest(setEnabledRequestSchema, { id, enabled }, "请求无效。");
  await invokeNative("resource_account_v1_set_enabled", voidResponse, { request });
}

export async function deleteResourceAccount(id: string): Promise<void> {
  const request = parseRequest(deleteRequestSchema, { id }, "账号标识无效。");
  await invokeNative("resource_account_v1_delete", voidResponse, { request });
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    const firstIssue = result.error.issues[0];
    throw new NativeCommandError(
      "validation_error",
      firstIssue?.message ?? fallback,
      firstIssue?.path.join("."),
    );
  }
  return result.data;
}

/**
 * 账号的即梦积分余额快照。
 *
 * 由 `application/account_health_worker.rs` 每轮健康检查从代理的 `/token/points`
 * 取回并写进 `resource_accounts.extra_json`——此前那段数据被解析出来后直接丢弃，
 * 所以 UI 无处可显示。解析失败返回 `null`（老记录里没有这个键）。
 */
export interface AccountCredits {
  total: number;
  gift: number;
  purchase: number;
  vip: number;
  checkedAt: string | null;
}

/** 从账号记录的 `extraJson` 里读积分快照；没有或格式不对时返回 `null`。 */
export function parseAccountCredits(record: { extraJson: string }): AccountCredits | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(record.extraJson || "{}");
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const credits = (parsed as { credits?: unknown }).credits;
  if (typeof credits !== "object" || credits === null) return null;
  const raw = credits as Record<string, unknown>;
  if (typeof raw.total !== "number") return null;
  const num = (key: string): number => (typeof raw[key] === "number" ? (raw[key] as number) : 0);
  const checkedAt = (parsed as { creditsCheckedAt?: unknown }).creditsCheckedAt;
  return {
    total: raw.total,
    gift: num("gift"),
    purchase: num("purchase"),
    vip: num("vip"),
    checkedAt: typeof checkedAt === "string" ? checkedAt : null,
  };
}
