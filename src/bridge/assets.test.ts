import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AssetCommandError,
  type ListAssetsOptions,
  getAsset,
  importAssets,
  listAssets,
  openAssetFileDialog,
  reverifyAssets,
} from "./assets";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const asset = {
  id: "123e4567-e89b-42d3-a456-426614174000",
  storageNamespace: "workspace",
  assetKind: "image",
  displayName: "示例图片.png",
  relativePath: "assets/123e4567-e89b-42d3-a456-426614174000.png",
  sizeBytes: 4_096,
  sha256: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
  mimeType: "image/png",
  integrityStatus: "valid",
  metadataJson: "{}",
  originDeviceId: "223e4567-e89b-42d3-a456-426614174000",
  revision: 1,
  createdAt: "2026-07-16T00:00:00Z",
  updatedAt: "2026-07-16T00:00:00Z",
};

const importSummary = {
  imported: [{ asset, sourcePath: "C:/tmp/sample.png" }],
  skipped: [],
  failures: [],
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("assets bridge", () => {
  it("validates the asset list response", async () => {
    mockedInvoke.mockResolvedValue([asset]);

    await expect(listAssets()).resolves.toEqual([asset]);
    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_list", { request: {} });
  });

  it("passes filter options through to the native command", async () => {
    mockedInvoke.mockResolvedValue([asset]);

    await listAssets({
      search: "示例",
      storageNamespace: "teaching-resource",
      assetKind: "image",
      integrityStatus: "valid",
      limit: 50,
    });

    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_list", {
      request: {
        search: "示例",
        storageNamespace: "teaching-resource",
        assetKind: "image",
        integrityStatus: "valid",
        limit: 50,
      },
    });
  });

  it("trims and forwards the search term", async () => {
    mockedInvoke.mockResolvedValue([]);

    await listAssets({ search: "  示例  " });

    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_list", {
      request: { search: "示例" },
    });
  });

  it("rejects an invalid storage namespace before invoking native code", async () => {
    await expect(
      listAssets({ storageNamespace: "personal" } as unknown as ListAssetsOptions),
    ).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("rejects a non-positive limit before invoking native code", async () => {
    await expect(listAssets({ limit: 0 })).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("returns null when the asset does not exist", async () => {
    mockedInvoke.mockResolvedValue(null);

    await expect(getAsset(asset.id)).resolves.toBeNull();
    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_get", {
      request: { id: asset.id },
    });
  });

  it("returns the asset record when found", async () => {
    mockedInvoke.mockResolvedValue(asset);

    await expect(getAsset(asset.id)).resolves.toEqual(asset);
  });

  it("rejects an invalid asset id before invoking native code", async () => {
    await expect(getAsset("not-a-uuid")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("imports assets and validates the summary response", async () => {
    mockedInvoke.mockResolvedValue(importSummary);

    await expect(importAssets(["C:/tmp/sample.png"], "workspace")).resolves.toEqual(importSummary);
    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_import", {
      request: {
        sourcePaths: ["C:/tmp/sample.png"],
        namespace: "workspace",
      },
    });
  });

  it("rejects an empty source path list before invoking native code", async () => {
    await expect(importAssets([], "workspace")).rejects.toMatchObject({
      code: "validation_failed",
    });
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("validates the reverification summary response", async () => {
    const summary = { checked: 3, valid: 2, missing: 1, corrupt: 0, quarantined: 0 };
    mockedInvoke.mockResolvedValue(summary);

    await expect(reverifyAssets()).resolves.toEqual(summary);
    expect(mockedInvoke).toHaveBeenCalledWith("asset_v1_reverify", {
      request: {},
    });
  });

  it("rejects malformed reverification summary", async () => {
    mockedInvoke.mockResolvedValue({
      checked: -1,
      valid: 0,
      missing: 0,
      corrupt: 0,
      quarantined: 0,
    });
    await expect(reverifyAssets()).rejects.toMatchObject({ code: "ipc_contract_invalid" });
  });

  it("maps asset integrity errors from the native layer", async () => {
    mockedInvoke.mockRejectedValue({
      code: "asset_integrity_unavailable",
      message: "无法读取受管文件完成完整性校验。",
    });

    await expect(reverifyAssets()).rejects.toEqual(
      expect.objectContaining<Partial<AssetCommandError>>({
        code: "asset_integrity_unavailable",
      }),
    );
  });

  it("rejects invalid native records", async () => {
    mockedInvoke.mockResolvedValue([{ ...asset, revision: 0 }]);

    await expect(listAssets()).rejects.toMatchObject({ code: "ipc_contract_invalid" });
  });

  it("maps structured asset storage errors", async () => {
    mockedInvoke.mockRejectedValue({
      code: "asset_storage_unavailable",
      message: "受管文件目录暂不可用，请检查磁盘空间或权限后重试。",
    });

    await expect(importAssets(["C:/tmp/sample.png"], "workspace")).rejects.toEqual(
      expect.objectContaining<Partial<AssetCommandError>>({
        code: "asset_storage_unavailable",
      }),
    );
  });

  describe("openAssetFileDialog", () => {
    it("returns selected file paths when multiple files are chosen", async () => {
      mockedInvoke.mockResolvedValue(["C:/tmp/a.png", "C:/tmp/b.png"]);

      await expect(openAssetFileDialog()).resolves.toEqual(["C:/tmp/a.png", "C:/tmp/b.png"]);
      expect(mockedInvoke).toHaveBeenCalledWith(
        "plugin:dialog|open",
        expect.objectContaining({
          options: expect.objectContaining({ multiple: true, directory: false }),
        }),
      );
    });

    it("wraps a single selection into an array", async () => {
      mockedInvoke.mockResolvedValue("C:/tmp/single.png");

      await expect(openAssetFileDialog({ multiple: false })).resolves.toEqual([
        "C:/tmp/single.png",
      ]);
    });

    it("returns an empty array when the user cancels", async () => {
      mockedInvoke.mockResolvedValue(null);

      await expect(openAssetFileDialog()).resolves.toEqual([]);
    });

    it("throws when the native runtime is unavailable", async () => {
      mockedIsTauri.mockReturnValue(false);

      await expect(openAssetFileDialog()).rejects.toMatchObject({
        code: "desktop_runtime_required",
      });
      expect(mockedInvoke).not.toHaveBeenCalled();
    });
  });
});
