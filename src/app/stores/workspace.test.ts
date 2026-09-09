import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  getWorkspaceStatus,
  initializeWorkspace,
  isWorkspaceRuntimeAvailable,
} from "../../bridge/workspace";
import { useWorkspaceStore } from "./workspace";

vi.mock("../../bridge/workspace", async (importOriginal) => ({
  ...(await importOriginal()),
  getWorkspaceStatus: vi.fn(),
  initializeWorkspace: vi.fn(),
  isWorkspaceRuntimeAvailable: vi.fn(),
}));

const mockedGetStatus = vi.mocked(getWorkspaceStatus);
const mockedInitialize = vi.mocked(initializeWorkspace);
const mockedRuntimeAvailable = vi.mocked(isWorkspaceRuntimeAvailable);

const uninitializedStatus = {
  initialized: false,
  workspace: null,
  database: {
    schemaVersion: 3,
    sqliteVersion: "3.51.2",
    journalMode: "wal",
    foreignKeysEnabled: true,
  },
  storage: {
    directoriesReady: true,
    writable: true,
    manifestReady: true,
    assetCount: 0,
    totalBytes: 0,
    missingAssetCount: 0,
    checkedAt: "2026-07-16T02:00:00Z",
  },
} as const;

const initializedStatus = {
  ...uninitializedStatus,
  initialized: true,
  workspace: {
    workspaceId: "550e8400-e29b-41d4-a716-446655440000",
    workspaceName: "春季课程",
    teacherId: "550e8400-e29b-41d4-a716-446655440001",
    teacherName: "王老师",
    createdAt: "2026-07-16T02:00:00Z",
  },
} as const;

beforeEach(() => {
  setActivePinia(createPinia());
  mockedGetStatus.mockReset();
  mockedInitialize.mockReset();
  mockedRuntimeAvailable.mockReturnValue(true);
});

describe("workspace store", () => {
  it("marks ready when workspace is already initialized", async () => {
    mockedGetStatus.mockResolvedValue(initializedStatus);
    const store = useWorkspaceStore();

    await store.ensureReady();

    expect(store.isReady).toBe(true);
    expect(store.errorMessage).toBeNull();
    expect(mockedInitialize).not.toHaveBeenCalled();
  });

  it("treats preview runtime as ready without invoking the bridge", async () => {
    mockedRuntimeAvailable.mockReturnValue(false);
    const store = useWorkspaceStore();

    await store.ensureReady();

    expect(store.isReady).toBe(true);
    expect(mockedGetStatus).not.toHaveBeenCalled();
    expect(mockedInitialize).not.toHaveBeenCalled();
  });

  it("auto-initializes workspace with the default name when not yet initialized", async () => {
    const initializedWorkspace = initializedStatus.workspace;
    if (!initializedWorkspace) throw new Error("expected initialized workspace payload");
    mockedGetStatus
      .mockResolvedValueOnce(uninitializedStatus)
      .mockResolvedValueOnce(initializedStatus);
    mockedInitialize.mockResolvedValue({
      workspaceId: initializedWorkspace.workspaceId,
      workspaceName: "OPEN AIGC的工作空间",
      teacherId: initializedWorkspace.teacherId,
      teacherName: "OPEN AIGC",
      createdAt: "2026-07-16T02:00:00Z",
    });
    const store = useWorkspaceStore();

    await store.ensureReady();

    expect(mockedInitialize).toHaveBeenCalledWith({
      workspaceName: "OPEN AIGC的工作空间",
      teacherName: "OPEN AIGC",
    });
    expect(store.isReady).toBe(true);
  });

  it("captures errors and keeps store unready", async () => {
    mockedGetStatus.mockRejectedValue(new Error("数据库锁定"));
    const store = useWorkspaceStore();

    await store.ensureReady();

    expect(store.isReady).toBe(false);
    expect(store.errorMessage).toBe("数据库锁定");
  });

  it("is idempotent when called multiple times", async () => {
    mockedGetStatus.mockResolvedValue(initializedStatus);
    const store = useWorkspaceStore();

    await store.ensureReady();
    await store.ensureReady();

    expect(mockedGetStatus).toHaveBeenCalledTimes(1);
  });
});
