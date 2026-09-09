import { invoke, isTauri } from "@tauri-apps/api/core";
import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError } from "./native";

const backupManifestSchema = z
  .object({
    formatVersion: z.number().int().nonnegative(),
    workspaceName: z.string().min(1).max(80),
    teacherName: z.string().min(1).max(60),
    schemaVersion: z.number().int().positive(),
    assetCount: z.number().int().nonnegative(),
    totalBytes: z.number().int().nonnegative(),
    createdAt: z.string().min(1),
    note: z.string().max(500).nullable(),
  })
  .strict();

const backupSummarySchema = z
  .object({
    archivePath: z.string().min(1),
    archiveSize: z.number().int().nonnegative(),
    managedFileCount: z.number().int().nonnegative(),
    managedTotalBytes: z.number().int().nonnegative(),
    createdAt: z.string().min(1),
  })
  .strict();

const restorePreviewSchema = z
  .object({
    archivePath: z.string().min(1),
    manifest: backupManifestSchema,
    managedFileCount: z.number().int().nonnegative(),
    managedTotalBytes: z.number().int().nonnegative(),
  })
  .strict();

const restoreSummarySchema = z
  .object({
    archivePath: z.string().min(1),
    safetyBackupPath: z.string().min(1).nullable(),
    restoredFileCount: z.number().int().nonnegative(),
    restoredTotalBytes: z.number().int().nonnegative(),
    restoredAt: z.string().min(1),
    requiresRestart: z.boolean(),
  })
  .strict();

const createBackupRequestSchema = z
  .object({
    archivePath: z
      .string()
      .trim()
      .min(1, "备份路径不能为空。")
      .max(500, "备份路径不能超过 500 个字符。"),
    note: z.string().trim().max(500, "备注不能超过 500 个字符。").optional(),
  })
  .strict();

const previewRestoreRequestSchema = z
  .object({
    archivePath: z
      .string()
      .trim()
      .min(1, "备份路径不能为空。")
      .max(500, "备份路径不能超过 500 个字符。"),
  })
  .strict();

const restoreBackupRequestSchema = z
  .object({
    archivePath: z
      .string()
      .trim()
      .min(1, "备份路径不能为空。")
      .max(500, "备份路径不能超过 500 个字符。"),
  })
  .strict();

export type BackupManifest = z.infer<typeof backupManifestSchema>;
export type BackupSummary = z.infer<typeof backupSummarySchema>;
export type RestorePreview = z.infer<typeof restorePreviewSchema>;
export type RestoreSummary = z.infer<typeof restoreSummarySchema>;
export type CreateBackupOptions = z.input<typeof createBackupRequestSchema>;

export { NativeCommandError as BackupCommandError };

/**
 * 创建一份完整工作区备份：SQLite 快照 + 受管文件 + manifest。
 * 备份归档原子写入：先写临时文件再 rename，避免半成品归档。
 */
export async function createBackup(options: CreateBackupOptions): Promise<BackupSummary> {
  const request = parseRequest(createBackupRequestSchema, options, "备份信息无效。");
  return invokeNative("backup_v1_create", backupSummarySchema, { request });
}

/**
 * 读取备份归档的预览信息，不修改任何本地状态。
 * 用于恢复前让用户确认归档内容。
 */
export async function previewRestore(archivePath: string): Promise<RestorePreview> {
  const request = parseRequest(previewRestoreRequestSchema, { archivePath }, "备份归档路径无效。");
  return invokeNative("backup_v1_preview_restore", restorePreviewSchema, { request });
}

/**
 * 从备份归档恢复工作区：先自动创建安全备份，再替换 SQLite 与受管文件。
 * 成功后调用方应提示用户重启应用。
 */
export async function restoreBackup(archivePath: string): Promise<RestoreSummary> {
  const request = parseRequest(restoreBackupRequestSchema, { archivePath }, "备份归档路径无效。");
  return invokeNative("backup_v1_restore", restoreSummarySchema, { request });
}

/**
 * 弹出原生文件保存对话框，返回用户选择的备份归档路径。
 * 用户取消时返回 null。仅在桌面运行时可用。
 */
export async function pickBackupSavePath(
  defaultName = "aigc-studio-backup.zip",
): Promise<string | null> {
  if (!isTauri()) {
    throw new NativeCommandError(
      "desktop_runtime_required",
      "此功能只能在 OPEN AIGC 桌面应用中使用。",
    );
  }

  const response = await invoke<string | null>("plugin:dialog|save", {
    options: {
      title: "选择备份保存位置",
      defaultPath: defaultName,
      filters: [
        { name: "OPEN AIGC 备份", extensions: ["zip"] },
        { name: "所有文件", extensions: ["*"] },
      ],
    },
  });
  return response;
}

/**
 * 弹出原生文件打开对话框，返回用户选择的备份归档路径。
 * 用户取消时返回 null。仅在桌面运行时可用。
 */
export async function pickBackupRestorePath(): Promise<string | null> {
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
      title: "选择要恢复的备份归档",
      filters: [
        { name: "OPEN AIGC 备份", extensions: ["zip"] },
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
