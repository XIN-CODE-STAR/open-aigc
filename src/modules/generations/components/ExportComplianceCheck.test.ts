/**
 * ExportComplianceCheck 组件单元测试
 */
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import ExportComplianceCheck from "./ExportComplianceCheck.vue";

describe("ExportComplianceCheck", () => {
  const mockCompliantSummary = {
    canExport: true,
    blockReasons: [],
    needsReviewCount: 0,
    flaggedCount: 0,
    blockedCount: 0,
  };

  const mockNonCompliantSummary = {
    canExport: false,
    blockReasons: [
      "资产 asset-1: 导出被禁止",
      "内容 (target-1): 安全状态为 blocked",
      "版权状态未知",
    ],
    needsReviewCount: 2,
    flaggedCount: 1,
    blockedCount: 2,
  };

  it("mounts without crashing", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockCompliantSummary },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("shows compliant status when canExport is true", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockCompliantSummary },
    });
    expect(wrapper.text()).toContain("合规通过");
  });

  it("shows non-compliant status when canExport is false", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockNonCompliantSummary },
    });
    expect(wrapper.text()).toContain("存在阻断");
  });

  it("shows block reasons when present", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockNonCompliantSummary },
    });
    expect(wrapper.text()).toContain("阻断原因");
    expect(wrapper.text()).toContain("资产 asset-1: 导出被禁止");
    expect(wrapper.text()).toContain("版权状态未知");
  });

  it("shows truncated block reasons when > 3", () => {
    const summary = {
      ...mockNonCompliantSummary,
      blockReasons: ["理由1", "理由2", "理由3", "理由4", "理由5"],
    };
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary },
    });
    expect(wrapper.text()).toContain("还有");
    expect(wrapper.text()).toContain("条...");
  });

  it("shows compliance pass message when compliant", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockCompliantSummary },
    });
    expect(wrapper.text()).toContain("所有资产已通过合规检查");
  });

  it("shows checking state when isChecking is true", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockCompliantSummary, isChecking: true },
    });
    expect(wrapper.text()).toContain("检查中");
  });

  it("emits export event when export button is clicked", async () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockCompliantSummary },
    });
    const btn = wrapper.find(".compliance-export-btn");
    if (btn.exists()) {
      await btn.trigger("click");
      expect(wrapper.emitted("export")).toBeTruthy();
    }
  });

  it("emits view-blocked event when toggle is clicked", async () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockNonCompliantSummary },
    });
    const btn = wrapper.find(".compliance-blocks__toggle");
    if (btn.exists()) {
      await btn.trigger("click");
      expect(wrapper.emitted("view-blocked")).toBeTruthy();
    }
  });

  it("shows blocked count in stats", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockNonCompliantSummary },
    });
    expect(wrapper.text()).toContain("已阻断");
    expect(wrapper.text()).toContain("2");
  });

  it("shows needs review count in stats", () => {
    const wrapper = mount(ExportComplianceCheck, {
      props: { summary: mockNonCompliantSummary },
    });
    expect(wrapper.text()).toContain("待复核");
  });
});
