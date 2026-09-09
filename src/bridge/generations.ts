import { invoke, isTauri } from "@tauri-apps/api/core";
import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError } from "./native";

const generationStatusSchema = z.enum(["pending", "running", "succeeded", "failed"]);

const generationTaskRecordSchema = z
  .object({
    id: z.uuid("任务标识无效。"),
    workspaceId: z.uuid("工作空间标识无效。"),
    providerName: z.string().min(1).max(80),
    modelName: z.string().min(1).max(120),
    promptText: z.string().min(1).max(4000),
    status: generationStatusSchema,
    errorMessage: z.string().max(500).nullable(),
    progress: z.number().int().min(0).max(100),
    revision: z.number().int().positive(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
    startedAt: z.string().min(1).nullable(),
    completedAt: z.string().min(1).nullable(),
  })
  .strict();

const generationResultRecordSchema = z
  .object({
    id: z.uuid("结果标识无效。"),
    taskId: z.uuid("任务标识无效。"),
    assetId: z.uuid("资产标识无效。"),
    createdAt: z.string().min(1),
  })
  .strict();

const submitTaskRequestSchema = z
  .object({
    providerName: z
      .string()
      .trim()
      .min(1, "供应商名称不能为空。")
      .max(80, "供应商名称不能超过 80 个字符。"),
    modelName: z
      .string()
      .trim()
      .min(1, "模型名称不能为空。")
      .max(120, "模型名称不能超过 120 个字符。"),
    promptText: z
      .string()
      .trim()
      .min(1, "提示词不能为空。")
      .max(4000, "提示词不能超过 4000 个字符。"),
  })
  .strict();

const recordOutputRequestSchema = z
  .object({
    taskId: z.uuid("任务标识无效。"),
    sourcePath: z
      .string()
      .trim()
      .min(1, "生成结果文件路径不能为空。")
      .max(500, "生成结果文件路径不能超过 500 个字符。"),
  })
  .strict();

const markFailedRequestSchema = z
  .object({
    taskId: z.uuid("任务标识无效。"),
    errorMessage: z
      .string()
      .trim()
      .min(1, "失败原因不能为空。")
      .max(500, "失败原因不能超过 500 个字符。"),
  })
  .strict();

const listTasksRequestSchema = z
  .object({
    status: generationStatusSchema.optional(),
    providerName: z.string().trim().max(80, "供应商名称不能超过 80 个字符。").optional(),
    limit: z.number().int().positive().max(500, "查询数量不能超过 500。").optional(),
  })
  .strict();

const getTaskRequestSchema = z
  .object({
    id: z.uuid("任务标识无效。"),
  })
  .strict();

const listResultsRequestSchema = z
  .object({
    taskId: z.uuid("任务标识无效。"),
  })
  .strict();

const generationTaskListSchema = z.array(generationTaskRecordSchema);
const generationTaskOrNullSchema = generationTaskRecordSchema.nullable();
const generationResultListSchema = z.array(generationResultRecordSchema);

export type GenerationStatus = z.infer<typeof generationStatusSchema>;
export type GenerationTaskRecord = z.infer<typeof generationTaskRecordSchema>;
export type GenerationResultRecord = z.infer<typeof generationResultRecordSchema>;
export type SubmitTaskInput = z.input<typeof submitTaskRequestSchema>;
export type ListTasksOptions = z.input<typeof listTasksRequestSchema>;

export { NativeCommandError as GenerationCommandError };

export const GENERATION_STATUS_LABELS: Record<GenerationStatus, string> = {
  pending: "待处理",
  running: "进行中",
  succeeded: "已完成",
  failed: "已失败",
};

/**
 * 提交一个生成任务，初始状态为 pending。workspaceId 由后端从当前工作空间自动注入。
 */
export async function submitTask(input: SubmitTaskInput): Promise<GenerationTaskRecord> {
  const request = parseRequest(submitTaskRequestSchema, { ...input }, "生成任务信息无效。");
  return invokeNative("generation_v1_submit", generationTaskRecordSchema, { request });
}

/**
 * 记录生成输出：把 AI 产出的文件导入受管存储，再写入生成历史和资源关联。
 * 后端会强制走 staging → 校验 → manifest 流程，失败时自动标记任务为 failed。
 */
export async function recordOutput(
  taskId: string,
  sourcePath: string,
): Promise<GenerationResultRecord> {
  const request = parseRequest(
    recordOutputRequestSchema,
    { taskId, sourcePath },
    "生成结果记录无效。",
  );
  return invokeNative("generation_v1_record_output", generationResultRecordSchema, { request });
}

/**
 * 手动标记任务为失败。
 */
export async function markFailed(
  taskId: string,
  errorMessage: string,
): Promise<GenerationTaskRecord> {
  const request = parseRequest(markFailedRequestSchema, { taskId, errorMessage }, "失败标记无效。");
  return invokeNative("generation_v1_mark_failed", generationTaskRecordSchema, { request });
}

/**
 * 查询生成任务列表。可按 status / providerName 过滤。
 */
export async function listTasks(options: ListTasksOptions = {}): Promise<GenerationTaskRecord[]> {
  const request = parseRequest(listTasksRequestSchema, { ...options }, "生成任务查询条件无效。");
  return invokeNative("generation_v1_list_tasks", generationTaskListSchema, { request });
}

/**
 * 按 id 获取单条生成任务。不存在时返回 null。
 */
export async function getTask(id: string): Promise<GenerationTaskRecord | null> {
  const request = parseRequest(getTaskRequestSchema, { id }, "任务标识无效。");
  return invokeNative("generation_v1_get_task", generationTaskOrNullSchema, { request });
}

/**
 * 列出生成任务关联的结果资产。
 */
export async function listResults(taskId: string): Promise<GenerationResultRecord[]> {
  const request = parseRequest(listResultsRequestSchema, { taskId }, "任务标识无效。");
  return invokeNative("generation_v1_list_results", generationResultListSchema, { request });
}

/**
 * 弹出原生文件打开对话框，选择生成结果文件路径。用户取消时返回 null。
 */
export async function pickGenerationOutputFile(): Promise<string | null> {
  if (!isTauri()) {
    throw new NativeCommandError(
      "desktop_runtime_required",
      "此功能只能在 OPEN AIGC 桌面应用中使用。",
    );
  }

  const response = await invoke<string | null>("plugin:dialog|open", {
    options: {
      multiple: false,
      directory: false,
      title: "选择生成结果文件",
      filters: [
        {
          name: "媒体文件",
          extensions: [
            "mp4",
            "mov",
            "avi",
            "mkv",
            "webm",
            "png",
            "jpg",
            "jpeg",
            "webp",
            "gif",
            "mp3",
            "wav",
            "flac",
          ],
        },
        { name: "所有文件", extensions: ["*"] },
      ],
    },
  });
  return response;
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw firstValidationError(result, fallback);
  }
  return result.data;
}
