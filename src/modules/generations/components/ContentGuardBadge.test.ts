/**
 * ContentGuardBadge 组件单元测试
 */
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import ContentGuardBadge from "./ContentGuardBadge.vue";
import type { ContentGuardReport } from "../../../bridge/content_guard";

describe("ContentGuardBadge", () => {
  const mockPassedReport: ContentGuardReport = {
    id: "guard-1",
    projectId: "project-1",
    targetType: "asset",
    targetId: "asset-1",
    taskId: null,
    assetId: "asset-1",
    guardVersion: null,
    guardProvider: null,
    status: "passed",
    riskLevel: "low",
    checksJson: JSON.stringify([]),
    actionsJson: JSON.stringify({ allowed: ["export", "share"], blocked: [] }),
    createdAt: "2026-07-20T10:00:00Z",
  };

  const mockFlaggedReport: ContentGuardReport = {
    id: "guard-2",
    projectId: "project-1",
    targetType: "asset",
    targetId: "asset-2",
    taskId: null,
    assetId: "asset-2",
    guardVersion: null,
    guardProvider: null,
    status: "flagged",
    riskLevel: "high",
    checksJson: JSON.stringify([
      { category: "political_sensitivity", status: "block", message: "检测到敏感政治关键词" },
      { category: "nsfw_content", status: "flag", message: "检测到敏感内容" },
    ]),
    actionsJson: JSON.stringify({ allowed: ["revise"], blocked: ["export", "publish"] }),
    createdAt: "2026-07-20T10:05:00Z",
  };

  it("mounts without crashing in compact mode", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockPassedReport, compact: true },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("mounts without crashing in detailed mode", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockFlaggedReport, compact: false },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("renders passed report text in compact mode", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockPassedReport, compact: true },
    });
    expect(wrapper.text()).toContain("安全通过");
  });

  it("renders flagged report text in detailed mode", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockFlaggedReport, compact: false },
    });
    expect(wrapper.text()).toContain("已标记风险");
    expect(wrapper.text()).toContain("高风险");
  });

  it("renders check details for flagged report", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockFlaggedReport, compact: false },
    });
    expect(wrapper.text()).toContain("political_sensitivity");
    expect(wrapper.text()).toContain("检测到敏感政治关键词");
  });

  it("renders blocked actions for flagged report", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockFlaggedReport, compact: false },
    });
    expect(wrapper.text()).toContain("已阻止的操作");
    expect(wrapper.text()).toContain("export");
    expect(wrapper.text()).toContain("publish");
  });

  it("handles null report without crashing", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: null, compact: true },
    });
    expect(wrapper.exists()).toBe(true);
    expect(wrapper.text()).toContain("未检查");
  });

  it("emits view-details event when clicked in compact mode", async () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockPassedReport, compact: true },
    });
    await wrapper.find("div").trigger("click");
    expect(wrapper.emitted("view-details")).toBeTruthy();
  });

  it("renders low risk level for passed report", () => {
    const wrapper = mount(ContentGuardBadge, {
      props: { report: mockPassedReport, compact: false },
    });
    expect(wrapper.text()).toContain("低风险");
  });
});
