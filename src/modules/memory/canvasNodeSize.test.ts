import { describe, expect, it } from "vitest";

import { NODE_SIZE_LIMITS, clampNodeSize, getEffectiveSize, withNodeSize } from "./canvasNodeSize";

describe("canvasNodeSize", () => {
  it("clamps sizes to limits and rounds", () => {
    expect(clampNodeSize(10, 5)).toEqual({
      width: NODE_SIZE_LIMITS.minWidth,
      height: NODE_SIZE_LIMITS.minHeight,
    });
    expect(clampNodeSize(5000, 5000)).toEqual({
      width: NODE_SIZE_LIMITS.maxWidth,
      height: NODE_SIZE_LIMITS.maxHeight,
    });
    expect(clampNodeSize(300.6)).toEqual({ width: 301 });
  });

  it("prefers payload.size over db columns and defaults", () => {
    const node = {
      payloadJson: JSON.stringify({ text: "hi", size: { width: 320, height: 200 } }),
      width: 280,
      height: 160,
    };
    expect(getEffectiveSize(node)).toEqual({ width: 320, height: 200 });
  });

  it("falls back to db columns, then default width", () => {
    expect(getEffectiveSize({ payloadJson: "{}", width: 280, height: 160 })).toEqual({
      width: 280,
      height: 160,
    });
    expect(getEffectiveSize({ payloadJson: "{}", width: null, height: null })).toEqual({
      width: NODE_SIZE_LIMITS.defaultWidth,
    });
    expect(getEffectiveSize({ payloadJson: "not json", width: null, height: null })).toEqual({
      width: NODE_SIZE_LIMITS.defaultWidth,
    });
  });

  it("ignores malformed payload sizes", () => {
    const node = {
      payloadJson: JSON.stringify({ size: { width: "bad" } }),
      width: 200,
      height: null,
    };
    expect(getEffectiveSize(node)).toEqual({ width: 200 });
  });

  it("keeps payload fields when writing size", () => {
    const json = withNodeSize(JSON.stringify({ text: "a", color: "#fff" }), {
      width: 300,
      height: 180,
    });
    expect(JSON.parse(json)).toEqual({
      text: "a",
      color: "#fff",
      size: { width: 300, height: 180 },
    });
    expect(JSON.parse(withNodeSize("not json", { width: 300 }))).toEqual({
      size: { width: 300 },
    });
  });
});
