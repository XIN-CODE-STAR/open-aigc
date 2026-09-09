/**
 * Creative Memory — 前端桥接层。
 *
 * 封装用户创意记忆的增删改查，支持偏好学习、风格沉淀、工作流习惯记录。
 */
import { z } from "zod";
import { invokeNative, isNativeRuntimeAvailable, NativeCommandError } from "./native";

// ═══════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════

const memoryTypeSchema = z.enum([
  "style_preference",
  "negative_preference",
  "brand_rule",
  "workflow_habit",
  "prompt_pattern",
  "review_history",
]);

const memoryScopeSchema = z.enum(["user", "project", "brand"]);
const memorySourceSchema = z.enum(["explicit_save", "confirmed_pattern", "project_template"]);
const memoryStatusSchema = z.enum(["active", "paused", "archived"]);

const creativeMemoryRecordSchema = z
  .object({
    id: z.string().min(1),
    memoryType: memoryTypeSchema,
    scope: memoryScopeSchema,
    scopeRefId: z.string().min(1).nullable(),
    contentJson: z.string().min(1),
    summary: z.string().min(1).max(500),
    source: memorySourceSchema,
    sourceRefId: z.string().min(1).nullable(),
    status: memoryStatusSchema,
    confidence: z.number().min(0).max(1),
    confirmCount: z.number().int().nonnegative(),
    createdBy: z.string().min(1),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

const creativeMemoryEventRecordSchema = z
  .object({
    id: z.string().min(1),
    memoryId: z.string().min(1),
    eventType: z.enum([
      "created",
      "updated",
      "paused",
      "resumed",
      "archived",
      "deleted",
      "confirmed",
    ]),
    eventDetail: z.string().nullable(),
    createdBy: z.string().min(1),
    createdAt: z.string().min(1),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type MemoryType = z.infer<typeof memoryTypeSchema>;
export type MemoryScope = z.infer<typeof memoryScopeSchema>;
export type MemorySource = z.infer<typeof memorySourceSchema>;
export type MemoryStatus = z.infer<typeof memoryStatusSchema>;
export type CreativeMemoryRecord = z.infer<typeof creativeMemoryRecordSchema>;
export type CreativeMemoryEventRecord = z.infer<typeof creativeMemoryEventRecordSchema>;

export { NativeCommandError as MemoryCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isMemoryRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 保存新的创意记忆。
 */
export async function saveCreativeMemory(request: {
  memoryType: MemoryType;
  scope: MemoryScope;
  scopeRefId?: string;
  contentJson: string;
  summary: string;
  source: MemorySource;
  sourceRefId?: string;
  createdBy: string;
}): Promise<CreativeMemoryRecord> {
  return invokeNative("creative_memory_v1_save", creativeMemoryRecordSchema, { request });
}

/**
 * 获取单条创意记忆。
 */
export async function getCreativeMemory(memoryId: string): Promise<CreativeMemoryRecord | null> {
  return invokeNative("creative_memory_v1_get", creativeMemoryRecordSchema.nullable(), {
    request: { memoryId },
  });
}

/**
 * 列出创意记忆。
 */
export async function listCreativeMemories(request?: {
  memoryType?: MemoryType;
  scope?: MemoryScope;
  scopeRefId?: string;
  status?: MemoryStatus;
  limit?: number;
}): Promise<CreativeMemoryRecord[]> {
  return invokeNative("creative_memory_v1_list", z.array(creativeMemoryRecordSchema), {
    request: request ?? {},
  });
}

/**
 * 更新创意记忆状态。
 */
export async function updateMemoryStatus(
  memoryId: string,
  status: MemoryStatus,
): Promise<CreativeMemoryRecord> {
  return invokeNative("creative_memory_v1_update_status", creativeMemoryRecordSchema, {
    request: { memoryId, status },
  });
}

/**
 * 更新创意记忆内容。
 */
export async function updateMemoryContent(
  memoryId: string,
  contentJson: string,
  summary: string,
): Promise<CreativeMemoryRecord> {
  return invokeNative("creative_memory_v1_update_content", creativeMemoryRecordSchema, {
    request: { memoryId, contentJson, summary },
  });
}

/**
 * 确认创意记忆（增加置信度）。
 */
export async function confirmMemory(memoryId: string): Promise<CreativeMemoryRecord> {
  return invokeNative("creative_memory_v1_confirm", creativeMemoryRecordSchema, {
    request: { memoryId },
  });
}

/**
 * 删除创意记忆（软删除）。
 */
export async function deleteMemory(memoryId: string): Promise<{ deleted: boolean }> {
  return invokeNative("creative_memory_v1_delete", z.object({ deleted: z.boolean() }), {
    request: { memoryId },
  });
}

/**
 * 获取创意记忆的事件历史。
 */
export async function listMemoryEvents(memoryId: string): Promise<CreativeMemoryEventRecord[]> {
  return invokeNative("creative_memory_v1_list_events", z.array(creativeMemoryEventRecordSchema), {
    request: { memoryId },
  });
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/**
 * 将记忆类型翻译为用户可读的中文标签。
 */
export function memoryTypeLabel(type: MemoryType): string {
  const labels: Record<MemoryType, string> = {
    style_preference: "风格偏好",
    negative_preference: "不喜欢的风格",
    brand_rule: "品牌规范",
    workflow_habit: "工作流习惯",
    prompt_pattern: "Prompt 模式",
    review_history: "审核历史",
  };
  return labels[type] ?? type;
}

/**
 * 将记忆作用域翻译为用户可读的中文标签。
 */
export function memoryScopeLabel(scope: MemoryScope): string {
  const labels: Record<MemoryScope, string> = {
    user: "个人",
    project: "项目",
    brand: "品牌",
  };
  return labels[scope] ?? scope;
}

/**
 * 将记忆状态翻译为用户可读的中文标签。
 */
export function memoryStatusLabel(status: MemoryStatus): string {
  const labels: Record<MemoryStatus, string> = {
    active: "生效中",
    paused: "已暂停",
    archived: "已归档",
  };
  return labels[status] ?? status;
}

/**
 * 解析记忆内容 JSON（带类型安全）。
 */
export function parseMemoryContent<T>(contentJson: string): T | null {
  try {
    return JSON.parse(contentJson) as T;
  } catch {
    return null;
  }
}
