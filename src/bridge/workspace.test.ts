import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkspaceCommandError } from "./workspace";
import { getWorkspaceStatus, initializeWorkspace } from "./workspace";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);
const storage = {
  directoriesReady: true,
  writable: true,
  manifestReady: true,
  assetCount: 0,
  totalBytes: 0,
  missingAssetCount: 0,
  checkedAt: "2026-07-16T02:00:00Z",
} as const;

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("workspace bridge", () => {
  it("validates a workspace status response", async () => {
    mockedInvoke.mockResolvedValue({
      initialized: false,
      workspace: null,
      database: {
        schemaVersion: 3,
        sqliteVersion: "3.51.2",
        journalMode: "wal",
        foreignKeysEnabled: true,
      },
      storage,
    });

    const status = await getWorkspaceStatus();

    expect(status.initialized).toBe(false);
    expect(status.database.schemaVersion).toBe(3);
    expect(mockedInvoke).toHaveBeenCalledWith("workspace_v1_get_status");
  });

  it("rejects an inconsistent native response", async () => {
    mockedInvoke.mockResolvedValue({
      initialized: true,
      workspace: null,
      database: {
        schemaVersion: 3,
        sqliteVersion: "3.51.2",
        journalMode: "wal",
        foreignKeysEnabled: true,
      },
      storage,
    });

    await expect(getWorkspaceStatus()).rejects.toMatchObject({
      code: "ipc_contract_invalid",
    });
  });

  it("maps structured native errors", async () => {
    mockedInvoke.mockRejectedValue({
      code: "workspace_already_initialized",
      message: "本地工作空间已经初始化。",
    });

    await expect(
      initializeWorkspace({ workspaceName: "春季课程", teacherName: "王老师" }),
    ).rejects.toEqual(
      expect.objectContaining<Partial<WorkspaceCommandError>>({
        code: "workspace_already_initialized",
        message: "本地工作空间已经初始化。",
      }),
    );
  });

  it("does not call native commands in browser preview", async () => {
    mockedIsTauri.mockReturnValue(false);

    await expect(getWorkspaceStatus()).rejects.toMatchObject({
      code: "desktop_runtime_required",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});
