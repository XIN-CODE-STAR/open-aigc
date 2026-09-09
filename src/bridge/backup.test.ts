import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  type BackupCommandError,
  type BackupManifest,
  type BackupSummary,
  type RestorePreview,
  type RestoreSummary,
  createBackup,
  pickBackupRestorePath,
  pickBackupSavePath,
  previewRestore,
  restoreBackup,
} from "./backup";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const manifest: BackupManifest = {
  formatVersion: 1,
  workspaceName: "春季课程",
  teacherName: "王老师",
  schemaVersion: 4,
  assetCount: 12,
  totalBytes: 1_024,
  createdAt: "2026-07-16T00:00:00Z",
  note: "期末归档",
};

const backupSummary: BackupSummary = {
  archivePath: "C:/tmp/backup.zip",
  archiveSize: 4_096,
  managedFileCount: 5,
  managedTotalBytes: 1_024,
  createdAt: "2026-07-16T00:00:00Z",
};

const restorePreview: RestorePreview = {
  archivePath: "C:/tmp/backup.zip",
  manifest,
  managedFileCount: 5,
  managedTotalBytes: 1_024,
};

const restoreSummary: RestoreSummary = {
  archivePath: "C:/tmp/backup.zip",
  safetyBackupPath: "C:/workspace/backups/pre-restore-2026-07-16T00-00-00Z.zip",
  restoredFileCount: 5,
  restoredTotalBytes: 1_024,
  restoredAt: "2026-07-16T00:00:00Z",
  requiresRestart: true,
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("backup bridge", () => {
  it("validates the create backup request and response", async () => {
    mockedInvoke.mockResolvedValue(backupSummary);

    await expect(
      createBackup({ archivePath: "C:/tmp/backup.zip", note: "期末归档" }),
    ).resolves.toEqual(backupSummary);
    expect(mockedInvoke).toHaveBeenCalledWith("backup_v1_create", {
      request: { archivePath: "C:/tmp/backup.zip", note: "期末归档" },
    });
  });

  it("rejects an empty archive path before invoking native code", async () => {
    await expect(createBackup({ archivePath: "  " })).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects a malformed backup summary", async () => {
    mockedInvoke.mockResolvedValue({ ...backupSummary, archiveSize: -1 });
    await expect(createBackup({ archivePath: "C:/tmp/backup.zip" })).rejects.toMatchObject({
      code: "ipc_contract_invalid",
    });
  });

  it("maps backup unavailable errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "backup_unavailable",
      message: "备份归档暂不可用，请检查磁盘空间或权限后重试。",
    });

    await expect(createBackup({ archivePath: "C:/tmp/backup.zip" })).rejects.toEqual(
      expect.objectContaining<Partial<BackupCommandError>>({
        code: "backup_unavailable",
      }),
    );
  });

  it("validates the restore preview response", async () => {
    mockedInvoke.mockResolvedValue(restorePreview);

    await expect(previewRestore("C:/tmp/backup.zip")).resolves.toEqual(restorePreview);
    expect(mockedInvoke).toHaveBeenCalledWith("backup_v1_preview_restore", {
      request: { archivePath: "C:/tmp/backup.zip" },
    });
  });

  it("rejects an empty restore preview path", async () => {
    await expect(previewRestore("  ")).rejects.toMatchObject({ code: "validation_failed" });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("maps invalid archive errors during preview", async () => {
    mockedInvoke.mockRejectedValue({
      code: "backup_archive_invalid",
      message: "备份归档损坏或格式不正确，无法继续。",
    });

    await expect(previewRestore("C:/tmp/bad.zip")).rejects.toEqual(
      expect.objectContaining<Partial<BackupCommandError>>({
        code: "backup_archive_invalid",
      }),
    );
  });

  it("maps incompatible schema errors during preview", async () => {
    mockedInvoke.mockRejectedValue({
      code: "backup_incompatible_schema",
      message: "备份的 schema 版本 8 高于当前应用支持的 4，无法恢复。",
    });

    await expect(previewRestore("C:/tmp/newer.zip")).rejects.toEqual(
      expect.objectContaining<Partial<BackupCommandError>>({
        code: "backup_incompatible_schema",
      }),
    );
  });

  it("validates the restore summary response", async () => {
    mockedInvoke.mockResolvedValue(restoreSummary);

    await expect(restoreBackup("C:/tmp/backup.zip")).resolves.toEqual(restoreSummary);
    expect(mockedInvoke).toHaveBeenCalledWith("backup_v1_restore", {
      request: { archivePath: "C:/tmp/backup.zip" },
    });
  });

  it("rejects a malformed restore summary", async () => {
    mockedInvoke.mockResolvedValue({ ...restoreSummary, requiresRestart: "yes" });
    await expect(restoreBackup("C:/tmp/backup.zip")).rejects.toMatchObject({
      code: "ipc_contract_invalid",
    });
  });

  it("returns the chosen path from the save dialog", async () => {
    mockedInvoke.mockResolvedValue("D:/exports/backup.zip");

    await expect(pickBackupSavePath()).resolves.toBe("D:/exports/backup.zip");
    expect(mockedInvoke).toHaveBeenCalledWith(
      "plugin:dialog|save",
      expect.objectContaining({
        options: expect.objectContaining({
          defaultPath: "aigc-studio-backup.zip",
          filters: expect.arrayContaining([expect.objectContaining({ name: "OPEN AIGC 备份" })]),
        }),
      }),
    );
  });

  it("returns null when the user cancels the save dialog", async () => {
    mockedInvoke.mockResolvedValue(null);
    await expect(pickBackupSavePath()).resolves.toBeNull();
  });

  it("returns the chosen path from the restore open dialog", async () => {
    mockedInvoke.mockResolvedValue("D:/imports/backup.zip");
    await expect(pickBackupRestorePath()).resolves.toBe("D:/imports/backup.zip");
  });

  it("throws when the native runtime is unavailable for the save dialog", async () => {
    mockedIsTauri.mockReturnValue(false);
    await expect(pickBackupSavePath()).rejects.toMatchObject({
      code: "desktop_runtime_required",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("throws when the native runtime is unavailable for the restore dialog", async () => {
    mockedIsTauri.mockReturnValue(false);
    await expect(pickBackupRestorePath()).rejects.toMatchObject({
      code: "desktop_runtime_required",
    });
  });
});
