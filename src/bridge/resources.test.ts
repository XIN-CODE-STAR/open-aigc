import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  type ResourceCommandError,
  createAssociation,
  deleteAssociation,
  getAssociation,
  listAssociations,
} from "./resources";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const association = {
  id: "11111111-2222-4333-8444-555555555555",
  assetId: "123e4567-e89b-42d3-a456-426614174000",
  contextKind: "teaching-resource",
  contextRef: "classroom:abc",
  role: "source",
  notes: "教案原图",
  revision: 1,
  createdAt: "2026-07-16T00:00:00Z",
  updatedAt: "2026-07-16T00:00:00Z",
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("resources bridge", () => {
  it("validates the association list response", async () => {
    mockedInvoke.mockResolvedValue([association]);

    await expect(listAssociations()).resolves.toEqual([association]);
    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_list", { request: {} });
  });

  it("passes filter options through to the native command", async () => {
    mockedInvoke.mockResolvedValue([association]);

    await listAssociations({
      assetId: "123e4567-e89b-42d3-a456-426614174000",
      contextKind: "teaching-resource",
      contextRef: "classroom:abc",
      role: "source",
      limit: 50,
    });

    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_list", {
      request: {
        assetId: "123e4567-e89b-42d3-a456-426614174000",
        contextKind: "teaching-resource",
        contextRef: "classroom:abc",
        role: "source",
        limit: 50,
      },
    });
  });

  it("trims and forwards the context ref", async () => {
    mockedInvoke.mockResolvedValue([]);

    await listAssociations({ contextRef: "  classroom:abc  " });

    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_list", {
      request: { contextRef: "classroom:abc" },
    });
  });

  it("rejects an invalid context kind before invoking native code", async () => {
    await expect(
      listAssociations({ contextKind: "personal" as unknown as never }),
    ).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects a non-positive limit before invoking native code", async () => {
    await expect(listAssociations({ limit: 0 })).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("returns null when the association does not exist", async () => {
    mockedInvoke.mockResolvedValue(null);

    await expect(getAssociation(association.id)).resolves.toBeNull();
    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_get", {
      request: { id: association.id },
    });
  });

  it("returns the association record when found", async () => {
    mockedInvoke.mockResolvedValue(association);

    await expect(getAssociation(association.id)).resolves.toEqual(association);
  });

  it("rejects an invalid association id before invoking native code", async () => {
    await expect(getAssociation("not-a-uuid")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("creates an association and validates the response", async () => {
    mockedInvoke.mockResolvedValue(association);

    await expect(
      createAssociation({
        assetId: association.assetId,
        contextKind: "teaching-resource",
        contextRef: "classroom:abc",
        role: "source",
        notes: "教案原图",
      }),
    ).resolves.toEqual(association);

    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_create", {
      request: {
        assetId: association.assetId,
        contextKind: "teaching-resource",
        contextRef: "classroom:abc",
        role: "source",
        notes: "教案原图",
      },
    });
  });

  it("rejects create with an invalid asset id before invoking native code", async () => {
    await expect(
      createAssociation({
        assetId: "not-a-uuid",
        contextKind: "teaching-resource",
        contextRef: "",
        role: "source",
      }),
    ).rejects.toMatchObject({ code: "validation_failed" });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects create with an unknown role before invoking native code", async () => {
    await expect(
      createAssociation({
        assetId: association.assetId,
        contextKind: "teaching-resource",
        contextRef: "",
        role: "thumbnail" as unknown as never,
      }),
    ).rejects.toMatchObject({ code: "validation_failed" });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("deletes an association by id", async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await expect(deleteAssociation(association.id)).resolves.toBeUndefined();
    expect(mockedInvoke).toHaveBeenCalledWith("resource_v1_delete", {
      request: { id: association.id },
    });
  });

  it("rejects malformed association records", async () => {
    mockedInvoke.mockResolvedValue([{ ...association, revision: 0 }]);

    await expect(listAssociations()).rejects.toMatchObject({ code: "ipc_contract_invalid" });
  });

  it("maps structured resource data errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "resource_data_unavailable",
      message: "资源关联数据暂时不可用，请重试或重启应用。",
    });

    await expect(listAssociations()).rejects.toEqual(
      expect.objectContaining<Partial<ResourceCommandError>>({
        code: "resource_data_unavailable",
      }),
    );
  });

  it("maps duplicate association errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "duplicate_value",
      message: "同一资产在该上下文中的关联已存在。",
    });

    await expect(
      createAssociation({
        assetId: association.assetId,
        contextKind: "teaching-resource",
        contextRef: "classroom:abc",
        role: "source",
      }),
    ).rejects.toEqual(
      expect.objectContaining<Partial<ResourceCommandError>>({
        code: "duplicate_value",
      }),
    );
  });
});
