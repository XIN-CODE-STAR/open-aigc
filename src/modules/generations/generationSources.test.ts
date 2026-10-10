import { describe, expect, it } from "vitest";

import { generationMediaKinds, isGenerationSourceFor } from "./generationSources";

describe("生成来源的媒体能力", () => {
  it("LLM 凭据不是生成来源——视频模式下不该出现（2026-10-10 的缺陷）", () => {
    const glm = { providerName: "zhipu", displayName: "GLM", modelName: "glm-4.5-air" };
    expect(generationMediaKinds(glm)).toEqual([]);
    expect(isGenerationSourceFor(glm, "video")).toBe(false);
    expect(isGenerationSourceFor(glm, "image")).toBe(false);
  });

  it("grok 只出图，不出视频", () => {
    const grok = { providerName: "grok", displayName: "Grok", modelName: "grok-2-image" };
    expect(generationMediaKinds(grok)).toEqual(["image"]);
    expect(isGenerationSourceFor(grok, "image")).toBe(true);
    expect(isGenerationSourceFor(grok, "video")).toBe(false);
  });

  it("kling / seedance / 即梦账号都同时支持图片与视频", () => {
    const cases = [
      { providerName: "kling", displayName: "可灵", modelName: "kling-v1" },
      { providerName: "seedance", displayName: "Seedance", modelName: "seedance-v2" },
      { providerName: "jimeng", displayName: "xin", modelName: "" },
      { providerName: "volcengine", displayName: "火山方舟", modelName: "doubao-seedance" },
    ];
    for (const source of cases) {
      expect(generationMediaKinds(source), JSON.stringify(source)).toEqual(["image", "video"]);
      expect(isGenerationSourceFor(source, "video")).toBe(true);
      expect(isGenerationSourceFor(source, "image")).toBe(true);
    }
  });

  it("匹配语义与后端一致：小写子串匹配 provider + display + model", () => {
    // 名字里带 needle 就命中，不要求 providerName 本身等于 needle
    expect(generationMediaKinds({ providerName: "my-kling-proxy", displayName: "x" })).toEqual([
      "image",
      "video",
    ]);
    expect(generationMediaKinds({ providerName: "x", displayName: "快影" })).toEqual([
      "image",
      "video",
    ]);
    expect(
      generationMediaKinds({ providerName: "x", displayName: "y", modelName: "seedream-3" }),
    ).toEqual(["image", "video"]);
  });

  it("kind 为 null（Agent 模式）时不过滤", () => {
    expect(isGenerationSourceFor({ providerName: "zhipu" }, null)).toBe(true);
    expect(isGenerationSourceFor({ providerName: "kling" }, null)).toBe(true);
  });
});
