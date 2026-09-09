import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError, voidResponse } from "./native";

const associationContextKindSchema = z.enum([
  "teaching-resource",
  "project-attachment",
  "generation-output",
]);

const associationRoleSchema = z.enum(["source", "result", "reference", "attachment"]);

const associationRecordSchema = z
  .object({
    id: z.uuid("关联标识无效。"),
    assetId: z.uuid("资产标识无效。"),
    contextKind: associationContextKindSchema,
    contextRef: z.string().max(200),
    role: associationRoleSchema,
    notes: z.string().max(500),
    revision: z.number().int().positive(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

const listAssociationsRequestSchema = z
  .object({
    assetId: z.uuid("资产标识无效。").optional(),
    contextKind: associationContextKindSchema.optional(),
    contextRef: z.string().trim().max(200, "上下文引用不能超过 200 个字符。").optional(),
    role: associationRoleSchema.optional(),
    limit: z.number().int().positive().max(1_000, "查询数量不能超过 1000。").optional(),
  })
  .strict();

const getAssociationRequestSchema = z
  .object({
    id: z.uuid("关联标识无效。"),
  })
  .strict();

const createAssociationRequestSchema = z
  .object({
    assetId: z.uuid("资产标识无效。"),
    contextKind: associationContextKindSchema,
    contextRef: z.string().trim().max(200, "上下文引用不能超过 200 个字符。"),
    role: associationRoleSchema,
    notes: z.string().trim().max(500, "备注不能超过 500 个字符。").optional(),
  })
  .strict();

const deleteAssociationRequestSchema = z
  .object({
    id: z.uuid("关联标识无效。"),
  })
  .strict();

const associationListSchema = z.array(associationRecordSchema);
const associationOrNullSchema = associationRecordSchema.nullable();

export type AssociationContextKind = z.infer<typeof associationContextKindSchema>;
export type AssociationRole = z.infer<typeof associationRoleSchema>;
export type AssociationRecord = z.infer<typeof associationRecordSchema>;
export type ListAssociationsOptions = z.input<typeof listAssociationsRequestSchema>;
export type CreateAssociationInput = z.input<typeof createAssociationRequestSchema>;

export { NativeCommandError as ResourceCommandError };

export const ASSOCIATION_CONTEXT_KIND_LABELS: Record<AssociationContextKind, string> = {
  "teaching-resource": "教学资源",
  "project-attachment": "项目附件",
  "generation-output": "生成输出",
};

export const ASSOCIATION_ROLE_LABELS: Record<AssociationRole, string> = {
  source: "原始素材",
  result: "生成产物",
  reference: "参考",
  attachment: "附件",
};

/**
 * 查询资源关联列表。可按 assetId / contextKind / contextRef / role 过滤。
 */
export async function listAssociations(
  options: ListAssociationsOptions = {},
): Promise<AssociationRecord[]> {
  const request = parseRequest(listAssociationsRequestSchema, options, "关联查询条件无效。");
  return invokeNative("resource_v1_list", associationListSchema, { request });
}

/**
 * 按 id 获取单条资源关联。不存在时返回 null。
 */
export async function getAssociation(id: string): Promise<AssociationRecord | null> {
  const request = parseRequest(getAssociationRequestSchema, { id }, "关联标识无效。");
  return invokeNative("resource_v1_get", associationOrNullSchema, { request });
}

/**
 * 创建一条资源关联：把资产绑定到教学资源 / 项目附件 / 生成输出等业务上下文。
 */
export async function createAssociation(input: CreateAssociationInput): Promise<AssociationRecord> {
  const request = parseRequest(createAssociationRequestSchema, input, "关联信息无效。");
  return invokeNative("resource_v1_create", associationRecordSchema, { request });
}

/**
 * 软删一条资源关联。仅标记 deleted_at，不物理删除。
 */
export async function deleteAssociation(id: string): Promise<void> {
  const request = parseRequest(deleteAssociationRequestSchema, { id }, "关联标识无效。");
  await invokeNative("resource_v1_delete", voidResponse, { request });
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw firstValidationError(result, fallback);
  }
  return result.data;
}
