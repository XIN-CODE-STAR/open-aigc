import { z } from "zod";
import { invokeNative, voidResponse } from "./native";

// ── Schemas ──────────────────────────────────────────

export const canvasNodeKindSchema = z.enum([
  "Scene",
  "Shot",
  "Image",
  "Video",
  "Upload",
  "Note",
  "Fact",
  "Document",
  "Character",
  "Prompt",
  "AgentState",
]);

export const canvasNodeStatusSchema = z.enum([
  "Pending",
  "Generating",
  "Succeeded",
  "Failed",
  "Draft",
  "Ready",
]);

export const canvasEdgeKindSchema = z.enum([
  "Reference",
  "Dependency",
  "Sequence",
  "Composition",
  "Similarity",
  "AgentLink",
]);

export const canvasPositionSchema = z
  .object({
    x: z.number(),
    y: z.number(),
  })
  .passthrough();

export const canvasSizeSchema = z
  .object({
    width: z.number(),
    height: z.number(),
  })
  .passthrough();

export const canvasRecordSchema = z
  .object({
    id: z.string(),
    workspaceId: z.string(),
    name: z.string(),
    description: z.string().nullable(),
    nodeCount: z.number(),
    edgeCount: z.number(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .passthrough();

export const canvasNodeRefsSchema = z
  .object({
    memoryId: z.string().nullable(),
    artifactId: z.string().nullable(),
    taskId: z.string().nullable(),
    shotId: z.string().nullable(),
    sceneId: z.string().nullable(),
    projectId: z.string().nullable(),
    agentId: z.string().nullable(),
    assetId: z.string().nullable(),
    conversationId: z.string().nullable(),
  })
  .passthrough();

export const canvasNodeSchema = z
  .object({
    id: z.string(),
    canvasId: z.string(),
    kind: canvasNodeKindSchema,
    position: canvasPositionSchema,
    size: canvasSizeSchema.nullable(),
    label: z.string().nullable(),
    summary: z.string().nullable(),
    description: z.string().nullable(),
    prompt: z.string().nullable(),
    status: canvasNodeStatusSchema.nullable(),
    refs: canvasNodeRefsSchema,
    metadata: z.record(z.unknown()),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .passthrough();

export const canvasEdgeSchema = z
  .object({
    id: z.string(),
    canvasId: z.string(),
    sourceNodeId: z.string(),
    targetNodeId: z.string(),
    kind: canvasEdgeKindSchema,
    label: z.string().nullable(),
    metadata: z.record(z.unknown()),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .passthrough();

export type CanvasRecord = z.infer<typeof canvasRecordSchema>;
export type CanvasNode = z.infer<typeof canvasNodeSchema>;
export type CanvasEdge = z.infer<typeof canvasEdgeSchema>;
export type CanvasNodeKind = z.infer<typeof canvasNodeKindSchema>;
export type CanvasNodeStatus = z.infer<typeof canvasNodeStatusSchema>;
export type CanvasEdgeKind = z.infer<typeof canvasEdgeKindSchema>;
export type CanvasPosition = z.infer<typeof canvasPositionSchema>;

// ── Commands ──────────────────────────────────────────

/** 创建画布 */
export async function canvasCreate(
  workspaceId: string,
  name: string,
  description?: string,
): Promise<CanvasRecord> {
  return invokeNative("canvas_v1_create", canvasRecordSchema, {
    request: { workspaceId, name, description },
  });
}

/** 列出所有画布 */
export async function canvasList(workspaceId: string): Promise<CanvasRecord[]> {
  return invokeNative("canvas_v1_list", z.array(canvasRecordSchema), {
    request: { workspaceId },
  });
}

/** 获取单个画布 */
export async function canvasGet(id: string): Promise<CanvasRecord | null> {
  return invokeNative("canvas_v1_get", canvasRecordSchema.nullable(), {
    request: { id },
  });
}

/** 删除画布 */
export async function canvasDelete(id: string): Promise<void> {
  return invokeNative("canvas_v1_delete", voidResponse, {
    request: { id },
  });
}

/** 列出画布上的所有节点 */
export async function canvasListNodes(canvasId: string): Promise<CanvasNode[]> {
  return invokeNative("canvas_v1_list_nodes", z.array(canvasNodeSchema), {
    request: { canvasId },
  });
}

/** 添加节点 */
export async function canvasAddNode(
  canvasId: string,
  kind: CanvasNodeKind,
  position: CanvasPosition,
  opts?: { summary?: string; description?: string; prompt?: string },
): Promise<CanvasNode> {
  return invokeNative("canvas_v1_add_node", canvasNodeSchema, {
    request: { canvasId, kind, position, ...opts },
  });
}

/** 更新节点 */
export async function canvasUpdateNode(
  nodeId: string,
  patch: Record<string, unknown>,
): Promise<CanvasNode> {
  return invokeNative("canvas_v1_update_node", canvasNodeSchema, {
    request: { nodeId, patch },
  });
}

/** 删除节点 */
export async function canvasDeleteNode(nodeId: string): Promise<void> {
  return invokeNative("canvas_v1_delete_node", voidResponse, {
    request: { nodeId },
  });
}

/** 列出画布上的所有边 */
export async function canvasListEdges(canvasId: string): Promise<CanvasEdge[]> {
  return invokeNative("canvas_v1_list_edges", z.array(canvasEdgeSchema), {
    request: { canvasId },
  });
}

/** 添加边 */
export async function canvasAddEdge(
  canvasId: string,
  sourceNodeId: string,
  targetNodeId: string,
  kind: CanvasEdgeKind,
  label?: string,
): Promise<CanvasEdge> {
  return invokeNative("canvas_v1_add_edge", canvasEdgeSchema, {
    request: { canvasId, sourceNodeId, targetNodeId, kind, label },
  });
}

/** 删除边 */
export async function canvasDeleteEdge(edgeId: string): Promise<void> {
  return invokeNative("canvas_v1_delete_edge", voidResponse, {
    request: { nodeId: edgeId },
  });
}
