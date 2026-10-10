/**
 * 哪些 Agent 工具的调用需要在画布上创建/更新「生成 pending 节点」。
 *
 * 只有真正产出媒体的任务才配得到一个 image 节点：image_generation /
 * video_generation。画布工具（canvas_add_note / canvas_search / canvas_add_image /
 * canvas_connect …）的节点由后端直接写入 memory canvas、成功后 refreshNodes()
 * 刷新；若把它们也送进 pending 节点逻辑，画布会被「摘要=工具名」的幻影 image
 * 节点污染——2026-10-01 `0338e63` 引入、2026-10-10 修复的缺陷。
 */

const GENERATION_NODE_TOOLS: ReadonlySet<string> = new Set([
  "image_generation",
  "video_generation",
]);

/** 该工具调用是否应驱动画布上的生成 pending 节点（创建 / 回填 / 失败标记）。 */
export function isGenerationNodeTool(toolName: string): boolean {
  return GENERATION_NODE_TOOLS.has(toolName);
}
