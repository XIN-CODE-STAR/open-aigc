import { invoke, isTauri } from "@tauri-apps/api/core";
import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError, voidResponse } from "./native";

const storageNamespaceSchema = z.enum(["workspace", "generation", "teaching-resource", "system"]);
const assetKindSchema = z.enum(["image", "video", "audio", "document", "archive", "other"]);
const integrityStatusSchema = z.enum(["unverified", "valid", "missing", "corrupt", "quarantined"]);

const sha256Pattern = /^[0-9a-f]{64}$/;

const assetRecordSchema = z
  .object({
    id: z.uuid("资产标识无效。"),
    storageNamespace: storageNamespaceSchema,
    assetKind: assetKindSchema,
    displayName: z.string().min(1).max(160),
    relativePath: z.string().min(1).max(500),
    sizeBytes: z.number().int().nonnegative(),
    sha256: z.string().regex(sha256Pattern).nullable(),
    mimeType: z.string().min(1).max(120).nullable(),
    integrityStatus: integrityStatusSchema,
    metadataJson: z.string().min(1),
    originDeviceId: z.uuid().nullable(),
    revision: z.number().int().positive(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

const assetImportOutcomeSchema = z
  .object({
    asset: assetRecordSchema,
    sourcePath: z.string().min(1),
  })
  .strict();

const assetImportSkippedSchema = z
  .object({
    sourcePath: z.string().min(1),
    reason: z.string().min(1),
    existingAssetId: z.uuid().nullable(),
  })
  .strict();

const assetImportFailureSchema = z
  .object({
    sourcePath: z.string().min(1),
    reason: z.string().min(1),
  })
  .strict();

const assetImportSummarySchema = z
  .object({
    imported: z.array(assetImportOutcomeSchema),
    skipped: z.array(assetImportSkippedSchema),
    failures: z.array(assetImportFailureSchema),
  })
  .strict();

const assetReverificationSummarySchema = z
  .object({
    checked: z.number().int().nonnegative(),
    valid: z.number().int().nonnegative(),
    missing: z.number().int().nonnegative(),
    corrupt: z.number().int().nonnegative(),
    quarantined: z.number().int().nonnegative(),
  })
  .strict();

const listAssetsRequestSchema = z
  .object({
    search: z.string().trim().max(80, "搜索内容不能超过 80 个字符。").optional(),
    storageNamespace: storageNamespaceSchema.optional(),
    assetKind: assetKindSchema.optional(),
    integrityStatus: integrityStatusSchema.optional(),
    limit: z.number().int().positive().max(1_000, "查询数量不能超过 1000。").optional(),
  })
  .strict();

const getAssetRequestSchema = z
  .object({
    id: z.uuid("资产标识无效。"),
  })
  .strict();

const importAssetsRequestSchema = z
  .object({
    sourcePaths: z.array(z.string().min(1)).min(1, "请至少选择一个文件。"),
    namespace: storageNamespaceSchema,
  })
  .strict();

const reverifyAssetsRequestSchema = z.object({}).strict();

const assetListSchema = z.array(assetRecordSchema);
const assetOrNullSchema = assetRecordSchema.nullable();

export type StorageNamespace = z.infer<typeof storageNamespaceSchema>;
export type AssetKind = z.infer<typeof assetKindSchema>;
export type IntegrityStatus = z.infer<typeof integrityStatusSchema>;
export type AssetRecord = z.infer<typeof assetRecordSchema>;
export type AssetImportOutcome = z.infer<typeof assetImportOutcomeSchema>;
export type AssetImportSkipped = z.infer<typeof assetImportSkippedSchema>;
export type AssetImportFailure = z.infer<typeof assetImportFailureSchema>;
export type AssetImportSummary = z.infer<typeof assetImportSummarySchema>;
export type AssetReverificationSummary = z.infer<typeof assetReverificationSummarySchema>;
export type ListAssetsOptions = z.input<typeof listAssetsRequestSchema>;

export { NativeCommandError as AssetCommandError };

export async function listAssets(options: ListAssetsOptions = {}): Promise<AssetRecord[]> {
  const request = parseRequest(listAssetsRequestSchema, { ...options }, "资产查询条件无效。");
  return invokeNative("asset_v1_list", assetListSchema, { request });
}

export async function getAsset(id: string): Promise<AssetRecord | null> {
  const request = parseRequest(getAssetRequestSchema, { id }, "资产标识无效。");
  return invokeNative("asset_v1_get", assetOrNullSchema, { request });
}

export async function importAssets(
  sourcePaths: string[],
  namespace: StorageNamespace,
): Promise<AssetImportSummary> {
  const request = parseRequest(
    importAssetsRequestSchema,
    { sourcePaths, namespace },
    "导入条件无效。",
  );
  return invokeNative("asset_v1_import", assetImportSummarySchema, { request });
}

/**
 * 触发文件完整性复检：扫描所有未软删的 manifest 记录，
 * 按受管文件实际存在性和哈希更新 integrity_status。
 */
export async function reverifyAssets(): Promise<AssetReverificationSummary> {
  const request = parseRequest(reverifyAssetsRequestSchema, {}, "会话令牌无效。");
  return invokeNative("asset_v1_reverify", assetReverificationSummarySchema, { request });
}

const openAssetRequestSchema = z
  .object({
    id: z.uuid("资产标识无效。"),
  })
  .strict();

/**
 * 通过系统默认程序打开资产对应的受管文件。仅允许打开 manifest 中已记录的资产。
 */
export async function openAssetFile(id: string): Promise<void> {
  const request = parseRequest(openAssetRequestSchema, { id }, "资产标识无效。");
  await invokeNative("asset_v1_open_file", voidResponse, { request });
}

/**
 * 在文件资源管理器中打开资产所在目录并选中该文件。仅允许定位 manifest 中已记录的资产。
 */
export async function openAssetFolder(id: string): Promise<void> {
  const request = parseRequest(openAssetRequestSchema, { id }, "资产标识无效。");
  await invokeNative("asset_v1_open_containing_folder", voidResponse, { request });
}

/**
 * 打开原生文件选择对话框，返回所选文件的绝对路径列表。
 * 用户取消选择时返回空数组。仅在桌面运行时可用。
 */
export async function openAssetFileDialog(
  options: {
    multiple?: boolean;
    title?: string;
  } = {},
): Promise<string[]> {
  if (!isTauri()) {
    throw new NativeCommandError(
      "desktop_runtime_required",
      "此功能只能在 OPEN AIGC 桌面应用中使用。",
    );
  }

  const response = await invoke<string | string[] | null>("plugin:dialog|open", {
    options: {
      multiple: options.multiple ?? true,
      directory: false,
      title: options.title ?? "选择要导入的文件",
      filters: [
        {
          name: "图片",
          extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tiff", "ico"],
        },
        {
          name: "视频",
          extensions: ["mp4", "mov", "avi", "mkv", "webm", "flv", "wmv", "m4v"],
        },
        {
          name: "音频",
          extensions: ["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma"],
        },
        {
          name: "文档",
          extensions: [
            "pdf",
            "doc",
            "docx",
            "xls",
            "xlsx",
            "ppt",
            "pptx",
            "txt",
            "md",
            "csv",
            "json",
            "xml",
            "html",
            "rtf",
          ],
        },
        {
          name: "压缩包",
          extensions: ["zip", "tar", "gz", "7z", "rar", "bz2", "xz"],
        },
        { name: "所有文件", extensions: ["*"] },
      ],
    },
  });

  if (response === null) {
    return [];
  }
  return Array.isArray(response) ? response : [response];
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw firstValidationError(result, fallback);
  }
  return result.data;
}
