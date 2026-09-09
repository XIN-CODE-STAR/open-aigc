import { z } from "zod";

import {
  firstValidationError,
  hasControlCharacters,
  invokeNative,
  isNativeRuntimeAvailable,
  NativeCommandError,
} from "./native";

const workspaceProfileSchema = z
  .object({
    workspaceId: z.uuid(),
    workspaceName: z.string().min(1).max(80),
    teacherId: z.uuid(),
    teacherName: z.string().min(1).max(60),
    createdAt: z.string().min(1),
  })
  .strict();

const databaseHealthSchema = z
  .object({
    schemaVersion: z.number().int().nonnegative(),
    sqliteVersion: z.string().min(1),
    journalMode: z.string().min(1),
    foreignKeysEnabled: z.boolean(),
  })
  .strict();

const managedStorageHealthSchema = z
  .object({
    directoriesReady: z.boolean(),
    writable: z.boolean(),
    manifestReady: z.boolean(),
    assetCount: z.number().int().nonnegative(),
    totalBytes: z.number().int().nonnegative(),
    missingAssetCount: z.number().int().nonnegative(),
    checkedAt: z.string().min(1),
  })
  .strict();

const workspaceStatusSchema = z
  .object({
    initialized: z.boolean(),
    workspace: workspaceProfileSchema.nullable(),
    database: databaseHealthSchema,
    storage: managedStorageHealthSchema,
  })
  .strict()
  .superRefine((status, context) => {
    if (status.initialized !== (status.workspace !== null)) {
      context.addIssue({
        code: "custom",
        message: "Workspace initialization state is inconsistent.",
        path: ["workspace"],
      });
    }
  });

const initializeWorkspaceRequestSchema = z
  .object({
    workspaceName: z
      .string()
      .trim()
      .min(1, "工作空间名称不能为空。")
      .max(80, "工作空间名称不能超过 80 个字符。")
      .refine((value) => !hasControlCharacters(value), {
        message: "工作空间名称包含不支持的控制字符。",
      }),
    teacherName: z
      .string()
      .trim()
      .min(1, "教师姓名不能为空。")
      .max(60, "教师姓名不能超过 60 个字符。")
      .refine((value) => !hasControlCharacters(value), {
        message: "教师姓名包含不支持的控制字符。",
      }),
  })
  .strict();

const renameWorkspaceRequestSchema = z
  .object({
    workspaceName: z
      .string()
      .trim()
      .min(1, "工作空间名称不能为空。")
      .max(80, "工作空间名称不能超过 80 个字符。")
      .refine((value) => !hasControlCharacters(value), {
        message: "工作空间名称包含不支持的控制字符。",
      }),
  })
  .strict();

export type WorkspaceProfile = z.infer<typeof workspaceProfileSchema>;
export type ManagedStorageHealth = z.infer<typeof managedStorageHealthSchema>;
export type WorkspaceStatus = z.infer<typeof workspaceStatusSchema>;
export type InitializeWorkspaceRequest = z.infer<typeof initializeWorkspaceRequestSchema>;
export type RenameWorkspaceRequest = z.infer<typeof renameWorkspaceRequestSchema>;

export { NativeCommandError as WorkspaceCommandError };

export function isWorkspaceRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

export async function getWorkspaceStatus(): Promise<WorkspaceStatus> {
  return invokeNative("workspace_v1_get_status", workspaceStatusSchema);
}

export async function initializeWorkspace(
  request: InitializeWorkspaceRequest,
): Promise<WorkspaceProfile> {
  const parsedRequest = initializeWorkspaceRequestSchema.safeParse(request);
  if (!parsedRequest.success) {
    throw firstValidationError(parsedRequest, "工作空间信息无效。");
  }

  return invokeNative("workspace_v1_initialize", workspaceProfileSchema, {
    request: parsedRequest.data,
  });
}

/**
 * 重命名当前工作空间。教师自助修改，无需 Admin 权限。
 * 后端 `workspace_v1_rename` 会复用与 `initialize` 相同的校验。
 */
export async function renameWorkspace(request: RenameWorkspaceRequest): Promise<WorkspaceProfile> {
  const parsedRequest = renameWorkspaceRequestSchema.safeParse(request);
  if (!parsedRequest.success) {
    throw firstValidationError(parsedRequest, "工作空间信息无效。");
  }

  return invokeNative("workspace_v1_rename", workspaceProfileSchema, {
    request: parsedRequest.data,
  });
}

const workspaceRootPathSchema = z
  .object({
    workspaceDir: z.string().min(1),
    managedFilesDir: z.string().min(1),
  })
  .strict();

export type WorkspaceRootPath = z.infer<typeof workspaceRootPathSchema>;

/**
 * 获取工作空间根目录路径（用于构造资产文件的完整路径）。
 */
export async function getWorkspaceRootPath(): Promise<WorkspaceRootPath> {
  return invokeNative("workspace_v1_get_root_path", workspaceRootPathSchema);
}
