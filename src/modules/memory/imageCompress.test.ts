import { afterEach, describe, expect, it, vi } from "vitest";

import {
  PAYLOAD_SIZE_LIMIT,
  THUMBNAIL_SOURCE_THRESHOLD,
  clearThumbnailCache,
  compressIfNeeded,
  thumbnailFor,
} from "./imageCompress";

afterEach(() => {
  vi.restoreAllMocks();
  clearThumbnailCache();
});

describe("imageCompress", () => {
  it("compressIfNeeded：体积不超标或非 dataUrl 图片时原样返回", async () => {
    const small = "data:image/png;base64,QUJD";
    await expect(compressIfNeeded(small)).resolves.toBe(small);

    const hugeText = "text/plain:hello".repeat(PAYLOAD_SIZE_LIMIT);
    await expect(compressIfNeeded(hugeText)).resolves.toBe(hugeText);
  });

  it("compressIfNeeded：Canvas 不可用时安全回退原图", async () => {
    const huge = `data:image/png;base64,${"A".repeat(PAYLOAD_SIZE_LIMIT + 1)}`;
    await expect(compressIfNeeded(huge)).resolves.toBe(huge);
  });

  it("thumbnailFor：小图与非 dataUrl 直接返回原图", async () => {
    const small = "data:image/png;base64,QUJD";
    await expect(thumbnailFor(small, "k1")).resolves.toBe(small);

    const remote = "https://cdn.example.com/a.png";
    await expect(thumbnailFor(remote, "k2")).resolves.toBe(remote);
  });

  it("thumbnailFor：大图在 Canvas 不可用时回退原图并按 cacheKey 缓存", async () => {
    const huge = `data:image/jpeg;base64,${"B".repeat(THUMBNAIL_SOURCE_THRESHOLD + 1)}`;
    const first = await thumbnailFor(huge, "big-key");
    const second = await thumbnailFor(huge, "big-key");
    expect(first).toBe(huge);
    expect(second).toBe(first);
  });
});
