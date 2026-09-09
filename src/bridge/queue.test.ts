import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { queueV1SubmitAttempt } from "./queue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const attempt = {
  id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
  taskId: "11111111-2222-4333-8444-555555555555",
  credentialId: "99999999-8888-4777-9666-555555555555",
  capability: "text-to-image",
  requestSnapshotJson: '{"prompt":"test"}',
  remoteJobId: "text2image:remote-1",
  status: "submitted" as const,
  progress: 0,
  errorCode: null,
  errorMessage: null,
  resultAssetId: null,
  providerId: "kling",
  consecutiveFailures: 0,
  lastPollAt: null,
  startedAt: null,
  finishedAt: null,
  createdAt: "2026-07-22T00:00:00Z",
  updatedAt: "2026-07-22T00:00:00Z",
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("queue bridge", () => {
  it("submits provider id and validates the full attempt response", async () => {
    mockedInvoke.mockResolvedValue(attempt);

    await expect(
      queueV1SubmitAttempt({
        taskId: attempt.taskId,
        credentialId: attempt.credentialId,
        capability: attempt.capability,
        requestSnapshotJson: attempt.requestSnapshotJson,
        providerId: attempt.providerId,
      }),
    ).resolves.toEqual(attempt);

    expect(mockedInvoke).toHaveBeenCalledWith("queue_v1_submit_attempt", {
      taskId: attempt.taskId,
      credentialId: attempt.credentialId,
      capability: attempt.capability,
      requestSnapshotJson: attempt.requestSnapshotJson,
      providerId: attempt.providerId,
    });
  });
});
