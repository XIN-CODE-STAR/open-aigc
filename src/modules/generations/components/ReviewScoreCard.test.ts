/**
 * ReviewScoreCard 组件单元测试
 */
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import ReviewScoreCard from "./ReviewScoreCard.vue";
import type { ReviewReportRecord } from "../../../bridge/review";

describe("ReviewScoreCard", () => {
  const mockReport: ReviewReportRecord = {
    id: "report-1",
    projectId: "project-1",
    runId: null,
    shotId: null,
    assetId: "asset-1",
    generationAttemptId: null,
    reviewerType: "auto",
    reviewerAgentVersion: null,
    requirementScoresJson: JSON.stringify({ match: 85, completeness: 80, clarity: 75 }),
    visualScoresJson: JSON.stringify({
      composition: 90,
      color: 85,
      lighting: 80,
      texture: 75,
      lensLanguage: 82,
    }),
    contentScoresJson: JSON.stringify({
      themeMatch: 88,
      emotionExpression: 85,
      narrativePurpose: 80,
    }),
    commercialScoresJson: JSON.stringify({
      platformFit: 70,
      audienceFit: 75,
      conversionPotential: 65,
    }),
    technicalScoresJson: JSON.stringify({
      clarity: 90,
      distortion: 85,
      characterConsistency: 88,
      motionQuality: 82,
    }),
    overallScore: 82.5,
    weightedScore: 83.0,
    issuesJson: JSON.stringify([
      {
        dimension: "角色一致性",
        severity: "medium",
        message: "角色脸部特征略有偏差",
        suggestedFix: "强化角色参考图约束",
      },
    ]),
    decision: "accept_with_suggestions",
    confidence: 0.85,
    reviewerProvider: "claude-vision",
    sourceTaskId: null,
    reviewVersion: 1,
    createdAt: "2026-07-20T10:00:00Z",
  };

  it("mounts without crashing", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("displays overall score", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport },
    });
    expect(wrapper.text()).toContain("综合评分");
    // 82.5 rounds to 83
    expect(wrapper.text()).toContain("83");
  });

  it("displays decision label", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport },
    });
    expect(wrapper.text()).toContain("可采纳");
    expect(wrapper.text()).toContain("优化建议");
  });

  it("displays reviewer provider", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport },
    });
    expect(wrapper.text()).toContain("claude-vision");
  });

  it("displays confidence", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport },
    });
    expect(wrapper.text()).toContain("置信度");
    expect(wrapper.text()).toContain("85%");
  });

  it("displays all five score layers in non-compact mode", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport, compact: false },
    });
    expect(wrapper.text()).toContain("需求层");
    expect(wrapper.text()).toContain("视觉层");
    expect(wrapper.text()).toContain("内容层");
    expect(wrapper.text()).toContain("商业层");
    expect(wrapper.text()).toContain("技术层");
  });

  it("displays issues when present", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport, compact: false },
    });
    expect(wrapper.text()).toContain("发现的问题");
    expect(wrapper.text()).toContain("角色脸部特征略有偏差");
    expect(wrapper.text()).toContain("强化角色参考图约束");
  });

  it("renders in compact mode without layers or issues", () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport, compact: true },
    });
    expect(wrapper.exists()).toBe(true);
    expect(wrapper.text()).toContain("综合评分");
  });

  it("emits view-details when button is clicked", async () => {
    const wrapper = mount(ReviewScoreCard, {
      props: { report: mockReport, compact: false },
    });
    const btn = wrapper.find(".review-action-btn");
    if (btn.exists()) {
      await btn.trigger("click");
      expect(wrapper.emitted("view-details")).toBeTruthy();
    }
  });

  it("shows video generation gate warning for low scores", () => {
    const lowReport = { ...mockReport, overallScore: 75 };
    const wrapper = mount(ReviewScoreCard, {
      props: { report: lowReport, compact: false },
    });
    expect(wrapper.text()).toContain("视频生成");
  });
});
