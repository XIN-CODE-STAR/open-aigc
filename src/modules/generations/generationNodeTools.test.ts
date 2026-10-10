import { describe, expect, it } from "vitest";

import { isGenerationNodeTool } from "./generationNodeTools";

// 与 src-tauri/src/adapters/agent/builtin_tools.rs 里的 TOOL_CANVAS_* 常量保持一致；
// 后端新增画布工具时这里要同步补一行。
const CANVAS_TOOLS = [
  "canvas_search",
  "canvas_add_note",
  "canvas_connect",
  "canvas_add_image",
  "canvas_update_node",
  "canvas_auto_layout",
  "canvas_export",
];

describe("isGenerationNodeTool", () => {
  it("生成类工具需要在画布上创建/更新 pending 节点", () => {
    expect(isGenerationNodeTool("image_generation")).toBe(true);
    expect(isGenerationNodeTool("video_generation")).toBe(true);
  });

  it("画布工具一律不走生成 pending 节点路径", () => {
    for (const tool of CANVAS_TOOLS) {
      expect(isGenerationNodeTool(tool), `${tool} 不应被当作生成任务`).toBe(false);
    }
  });

  it("任何 canvas_ 前缀的工具都不算生成任务（防未来新增画布工具漏网）", () => {
    for (const tool of [...CANVAS_TOOLS, "canvas_future_thing"]) {
      if (tool.startsWith("canvas_")) {
        expect(isGenerationNodeTool(tool), tool).toBe(false);
      }
    }
  });

  it("未知工具不建 pending 节点", () => {
    expect(isGenerationNodeTool("mcp_call")).toBe(false);
    expect(isGenerationNodeTool("current_time")).toBe(false);
    expect(isGenerationNodeTool("")).toBe(false);
  });
});
