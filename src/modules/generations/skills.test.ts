import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  buildGenerationHint,
  buildSkillOverride,
  buildSystemPromptOverride,
  EXTENSION_SKILLS,
  loadSelectedSkillIds,
  saveSelectedSkillIds,
} from "./skills";

describe("skills", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("buildSkillOverride 未选中时返回 undefined", () => {
    expect(buildSkillOverride([])).toBeUndefined();
    expect(buildSkillOverride(["not-a-skill"])).toBeUndefined();
  });

  it("buildSkillOverride 单个技能返回带头部标记的片段", () => {
    const override = buildSkillOverride(["pro-photographer"]);
    expect(override).toContain("[已启用技能");
    expect(override).toContain("专业摄影师");
  });

  it("buildSkillOverride 多技能按声明顺序合并", () => {
    const override = buildSkillOverride([
      "storyboard-director",
      "pro-photographer",
    ]);
    expect(override).toContain("专业摄影师");
    expect(override).toContain("分镜导演");
  });

  it("buildGenerationHint 未选中时返回 undefined", () => {
    expect(buildGenerationHint([])).toBeUndefined();
  });

  it("buildGenerationHint 以 风格技能: 前缀拼接提示", () => {
    const hint = buildGenerationHint(["ecommerce-copy"]);
    expect(hint).toMatch(/^风格技能:/);
    expect(hint).toContain("电商");
  });

  it("loadSelectedSkillIds 过滤未知技能 id", () => {
    localStorage.setItem(
      "aigc.selectedSkillIds",
      JSON.stringify(["pro-photographer", "bogus-id", 42]),
    );
    expect(loadSelectedSkillIds()).toEqual(["pro-photographer"]);
  });

  it("loadSelectedSkillIds 对损坏 JSON 返回空数组", () => {
    localStorage.setItem("aigc.selectedSkillIds", "{not json");
    expect(loadSelectedSkillIds()).toEqual([]);
  });

  it("save/load 往返保留全部技能 id", () => {
    const ids = EXTENSION_SKILLS.map((s) => s.id);
    saveSelectedSkillIds(ids);
    expect(loadSelectedSkillIds()).toEqual(ids);
  });
});

describe("buildSystemPromptOverride", () => {
  it("技能与项目记忆都为空时返回 undefined", () => {
    expect(buildSystemPromptOverride([], null)).toBeUndefined();
    expect(buildSystemPromptOverride([], "   ")).toBeUndefined();
  });

  it("只有项目记忆时返回项目说明片段", () => {
    const override = buildSystemPromptOverride([], "视觉风格：扁平插画");
    expect(override).toContain("[项目说明");
    expect(override).toContain("扁平插画");
    expect(override).not.toContain("[已启用技能");
  });

  it("两者同时存在时合并输出", () => {
    const override = buildSystemPromptOverride(["pro-photographer"], "主色 #4F46E5");
    expect(override).toBeDefined();
    const text = override ?? "";
    const skillIdx = text.indexOf("[已启用技能");
    const memIdx = text.indexOf("[项目说明");
    expect(skillIdx).toBeGreaterThan(-1);
    expect(memIdx).toBeGreaterThan(skillIdx);
  });
});
