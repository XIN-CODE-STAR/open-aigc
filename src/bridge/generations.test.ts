import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  type GenerationCommandError,
  getTask,
  listResults,
  listTasks,
  markFailed,
  recordOutput,
  submitTask,
} from "./generations";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const taskId = "11111111-2222-4333-8444-555555555555";
const workspaceId = "123e4567-e89b-42d3-a456-426614174000";
const assetId = "99999999-8888-4777-9666-555555555555";

const task = {
  id: taskId,
  workspaceId,
  providerName: "Seedance",
  modelName: "seedance-v2",
  promptText: "生成一段舞蹈视频",
  status: "pending" as const,
  errorMessage: null,
  progress: 0,
  revision: 1,
  createdAt: "2026-07-16T00:00:00Z",
  updatedAt: "2026-07-16T00:00:00Z",
  startedAt: null,
  completedAt: null,
};

const result = {
  id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
  taskId,
  assetId,
  createdAt: "2026-07-16T00:00:00Z",
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("generations bridge", () => {
  it("submits a task and validates the response", async () => {
    mockedInvoke.mockResolvedValue(task);

    await expect(
      submitTask({
        providerName: "Seedance",
        modelName: "seedance-v2",
        promptText: "生成一段舞蹈视频",
      }),
    ).resolves.toEqual(task);

    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_submit", {
      request: {
        providerName: "Seedance",
        modelName: "seedance-v2",
        promptText: "生成一段舞蹈视频",
      },
    });
  });

  it("trims whitespace before submitting a task", async () => {
    mockedInvoke.mockResolvedValue(task);

    await submitTask({
      providerName: "  Seedance  ",
      modelName: "  seedance-v2  ",
      promptText: "  生成一段舞蹈视频  ",
    });

    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_submit", {
      request: {
        providerName: "Seedance",
        modelName: "seedance-v2",
        promptText: "生成一段舞蹈视频",
      },
    });
  });

  it("rejects an empty provider name before invoking native code", async () => {
    await expect(
      submitTask({ providerName: "  ", modelName: "model", promptText: "prompt" }),
    ).rejects.toMatchObject({ code: "validation_failed" });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects an overly long prompt before invoking native code", async () => {
    await expect(
      submitTask({
        providerName: "Seedance",
        modelName: "model",
        promptText: "x".repeat(4001),
      }),
    ).rejects.toMatchObject({ code: "validation_failed" });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("records output and validates the response", async () => {
    mockedInvoke.mockResolvedValue(result);

    await expect(recordOutput(taskId, "D:/outputs/video.mp4")).resolves.toEqual(result);
    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_record_output", {
      request: { taskId, sourcePath: "D:/outputs/video.mp4" },
    });
  });

  it("trims the source path before recording output", async () => {
    mockedInvoke.mockResolvedValue(result);

    await recordOutput(taskId, "  D:/outputs/video.mp4  ");

    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_record_output", {
      request: { taskId, sourcePath: "D:/outputs/video.mp4" },
    });
  });

  it("rejects an invalid task id before recording output", async () => {
    await expect(recordOutput("not-a-uuid", "D:/outputs/video.mp4")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects an empty source path before recording output", async () => {
    await expect(recordOutput(taskId, "   ")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("marks a task as failed and validates the response", async () => {
    const failedTask = { ...task, status: "failed" as const, errorMessage: "超时" };
    mockedInvoke.mockResolvedValue(failedTask);

    await expect(markFailed(taskId, "超时")).resolves.toEqual(failedTask);
    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_mark_failed", {
      request: { taskId, errorMessage: "超时" },
    });
  });

  it("rejects marking failed with an empty error message", async () => {
    await expect(markFailed(taskId, "   ")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("lists tasks and forwards filter options", async () => {
    mockedInvoke.mockResolvedValue([task]);

    await listTasks({ status: "pending", providerName: "Seedance", limit: 50 });

    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_list_tasks", {
      request: { status: "pending", providerName: "Seedance", limit: 50 },
    });
  });

  it("lists tasks with empty filters by default", async () => {
    mockedInvoke.mockResolvedValue([task]);

    await listTasks();

    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_list_tasks", {
      request: {},
    });
  });

  it("rejects an unknown status filter before invoking native code", async () => {
    await expect(listTasks({ status: "unknown" as unknown as never })).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects a non-positive limit before invoking native code", async () => {
    await expect(listTasks({ limit: 0 })).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("returns null when the task does not exist", async () => {
    mockedInvoke.mockResolvedValue(null);

    await expect(getTask(taskId)).resolves.toBeNull();
    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_get_task", {
      request: { id: taskId },
    });
  });

  it("returns the task record when found", async () => {
    mockedInvoke.mockResolvedValue(task);

    await expect(getTask(taskId)).resolves.toEqual(task);
  });

  it("rejects an invalid task id before invoking native code", async () => {
    await expect(getTask("not-a-uuid")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("lists results for a task", async () => {
    mockedInvoke.mockResolvedValue([result]);

    await expect(listResults(taskId)).resolves.toEqual([result]);
    expect(mockedInvoke).toHaveBeenCalledWith("generation_v1_list_results", {
      request: { taskId },
    });
  });

  it("rejects listing results with an invalid task id", async () => {
    await expect(listResults("not-a-uuid")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects malformed task records", async () => {
    mockedInvoke.mockResolvedValue([{ ...task, revision: 0 }]);

    await expect(listTasks()).rejects.toMatchObject({ code: "ipc_contract_invalid" });
  });

  it("rejects malformed result records", async () => {
    mockedInvoke.mockResolvedValue([{ ...result, assetId: "not-a-uuid" }]);

    await expect(listResults(taskId)).rejects.toMatchObject({
      code: "ipc_contract_invalid",
    });
  });

  it("maps generation data errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "generation_data_unavailable",
      message: "生成历史数据暂时不可用，请重试或重启应用。",
    });

    await expect(listTasks()).rejects.toEqual(
      expect.objectContaining<Partial<GenerationCommandError>>({
        code: "generation_data_unavailable",
      }),
    );
  });

  it("maps record not found errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "record_not_found",
      message: "生成任务不存在或已被移除。",
    });

    await expect(getTask(taskId)).rejects.toEqual(
      expect.objectContaining<Partial<GenerationCommandError>>({
        code: "record_not_found",
      }),
    );
  });

  it("maps duplicate result errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "duplicate_value",
      message: "该生成任务的结果已存在。",
    });

    await expect(recordOutput(taskId, "D:/outputs/video.mp4")).rejects.toEqual(
      expect.objectContaining<Partial<GenerationCommandError>>({
        code: "duplicate_value",
      }),
    );
  });
});
