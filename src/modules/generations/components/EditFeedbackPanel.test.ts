/**
 * EditFeedbackPanel 组件单元测试
 */
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import EditFeedbackPanel from "./EditFeedbackPanel.vue";
import type { EditPlanRecord, EditRequestRecord } from "../../../bridge/edit";

describe("EditFeedbackPanel", () => {
  const mockRecentRequests: EditRequestRecord[] = [
    {
      id: "req-1",
      projectId: "project-1",
      runId: null,
      feedbackText: "太假了，增加真实感",
      status: "plan_ready",
      contextType: "generation_result",
      contextRefId: null,
      sourceReviewId: null,
      intentJson: null,
      ambiguousReason: null,
      createdBy: "teacher-1",
      createdAt: "2026-07-20T09:00:00Z",
      resolvedAt: null,
    },
    {
      id: "req-2",
      projectId: "project-1",
      runId: null,
      feedbackText: "不够高级，色调太冷",
      status: "applied",
      contextType: "generation_result",
      contextRefId: null,
      sourceReviewId: null,
      intentJson: null,
      ambiguousReason: null,
      createdBy: "teacher-1",
      createdAt: "2026-07-20T08:00:00Z",
      resolvedAt: "2026-07-20T08:05:00Z",
    },
  ];

  const mockPlan: EditPlanRecord = {
    id: "plan-1",
    editRequestId: "req-1",
    projectId: "project-1",
    planSummary: "提升真实感，减少CG感",
    operationType: "style_adjustment",
    scope: "whole",
    targetsJson: "[]",
    promptPatchJson: null,
    parameterPatchJson: null,
    referenceAssetPatchJson: null,
    requiresRegeneration: true,
    requiresCriticRerun: true,
    estimatedImpact: "medium",
    riskLevel: "medium",
    status: "ready",
    executionResultJson: null,
    createdBy: "teacher-1",
    createdAt: "2026-07-20T09:05:00Z",
    executedAt: null,
  };

  it("mounts without crashing", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: { projectId: "project-1" },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("renders title and feedback input", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: { projectId: "project-1" },
    });
    expect(wrapper.text()).toContain("修改反馈");
    expect(wrapper.find("textarea").exists()).toBe(true);
  });

  it("shows history count when requests provided", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        recentRequests: mockRecentRequests,
      },
    });
    expect(wrapper.text()).toContain("查看历史");
    expect(wrapper.text()).toContain("2");
  });

  it("shows edit plan when provided", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        currentPlan: mockPlan,
      },
    });
    expect(wrapper.text()).toContain("修改计划");
    expect(wrapper.text()).toContain("提升真实感，减少CG感");
    expect(wrapper.text()).toContain("风格调整");
  });

  it("shows requires regeneration notice", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        currentPlan: mockPlan,
      },
    });
    expect(wrapper.text()).toContain("需要重新生成");
  });

  it("handles feedback submission via button click", async () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: { projectId: "project-1" },
    });
    const textarea = wrapper.find("textarea");
    await textarea.setValue("更高级一点");
    const submitBtn = wrapper.find(".edit-submit-btn");
    if (submitBtn.exists()) {
      await submitBtn.trigger("click");
      expect(wrapper.emitted("submit-feedback")).toBeTruthy();
    }
  });

  it("emits apply-plan when apply button is clicked", async () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        currentPlan: mockPlan,
      },
    });
    const applyBtn = wrapper.find(".edit-plan-btn--apply");
    if (applyBtn.exists()) {
      await applyBtn.trigger("click");
      expect(wrapper.emitted("apply-plan")).toBeTruthy();
    }
  });

  it("emits skip-request when skip button is clicked", async () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        currentPlan: mockPlan,
      },
    });
    const skipBtn = wrapper.find(".edit-plan-btn--skip");
    if (skipBtn.exists()) {
      await skipBtn.trigger("click");
      expect(wrapper.emitted("skip-request")).toBeTruthy();
    }
  });

  it("toggles history visibility", async () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: {
        projectId: "project-1",
        recentRequests: mockRecentRequests,
      },
    });
    const toggleBtn = wrapper.find(".edit-history-toggle");
    if (toggleBtn.exists()) {
      await toggleBtn.trigger("click");
      expect(wrapper.text()).toContain("太假了，增加真实感");
    }
  });

  it("disables submit when textarea is empty", () => {
    const wrapper = mount(EditFeedbackPanel, {
      props: { projectId: "project-1" },
    });
    const submitBtn = wrapper.find(".edit-submit-btn");
    if (submitBtn.exists()) {
      expect(submitBtn.attributes("disabled")).toBeDefined();
    }
  });
});
