import { z } from "zod";

import { invokeNative, voidResponse } from "./native";

// ── Schemas ──────────────────────────────────────────

const memoryCanvasSchema = z
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

const memoryNodeSchema = z
  .object({
    id: z.string(),
    canvasId: z.string(),
    nodeType: z.string(),
    positionX: z.number(),
    positionY: z.number(),
    width: z.number().nullable(),
    height: z.number().nullable(),
    payloadJson: z.string(),
    summary: z.string().nullable(),
    assetId: z.string().nullable(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .passthrough();

const memoryEdgeSchema = z
  .object({
    id: z.string(),
    canvasId: z.string(),
    sourceNodeId: z.string(),
    targetNodeId: z.string(),
    edgeType: z.string(),
    label: z.string().nullable(),
    createdAt: z.string(),
  })
  .passthrough();

const memoryViewportSchema = z
  .object({
    canvasId: z.string(),
    zoom: z.number(),
    panX: z.number(),
    panY: z.number(),
    updatedAt: z.string(),
  })
  .passthrough();

const canvasListSchema = z.array(memoryCanvasSchema);
const nodeListSchema = z.array(memoryNodeSchema);
const edgeListSchema = z.array(memoryEdgeSchema);

// ── Types ────────────────────────────────────────────

export type MemoryCanvas = z.infer<typeof memoryCanvasSchema>;
export type MemoryNode = z.infer<typeof memoryNodeSchema>;
export type MemoryEdge = z.infer<typeof memoryEdgeSchema>;
export type MemoryViewport = z.infer<typeof memoryViewportSchema>;

// ── Canvas IPC ───────────────────────────────────────

export async function createCanvas(
  workspaceId: string,
  name: string,
  description?: string,
): Promise<MemoryCanvas> {
  return invokeNative("memory_canvas_v1_create", memoryCanvasSchema, {
    request: { workspace_id: workspaceId, name, description },
  });
}

export async function listCanvases(workspaceId: string): Promise<MemoryCanvas[]> {
  return invokeNative("memory_canvas_v1_list", canvasListSchema, {
    request: { workspace_id: workspaceId },
  });
}

export async function getCanvas(id: string): Promise<MemoryCanvas | null> {
  return invokeNative("memory_canvas_v1_get", memoryCanvasSchema.nullable(), {
    request: { id },
  });
}

export async function deleteCanvas(id: string): Promise<void> {
  await invokeNative("memory_canvas_v1_delete", voidResponse, {
    request: { id },
  });
}

// ── Node IPC ─────────────────────────────────────────

export async function addNode(
  canvasId: string,
  nodeType: string,
  positionX: number,
  positionY: number,
  payloadJson: string,
  summary?: string,
  assetId?: string,
): Promise<MemoryNode> {
  return invokeNative("memory_node_v1_add", memoryNodeSchema, {
    request: {
      canvas_id: canvasId,
      node_type: nodeType,
      position_x: positionX,
      position_y: positionY,
      payload_json: payloadJson,
      summary,
      asset_id: assetId,
    },
  });
}

export async function updateNode(
  id: string,
  canvasId: string,
  nodeType: string,
  positionX: number,
  positionY: number,
  payloadJson: string,
  summary?: string,
  assetId?: string,
): Promise<MemoryNode> {
  return invokeNative("memory_node_v1_update", memoryNodeSchema, {
    request: {
      id,
      canvas_id: canvasId,
      node_type: nodeType,
      position_x: positionX,
      position_y: positionY,
      payload_json: payloadJson,
      summary,
      asset_id: assetId,
    },
  });
}

export async function deleteNode(id: string): Promise<void> {
  await invokeNative("memory_node_v1_delete", voidResponse, {
    request: { id },
  });
}

export async function listNodes(canvasId: string): Promise<MemoryNode[]> {
  return invokeNative("memory_node_v1_list", nodeListSchema, {
    request: { canvas_id: canvasId },
  });
}

// ── Edge IPC ─────────────────────────────────────────

export async function addEdge(
  canvasId: string,
  sourceNodeId: string,
  targetNodeId: string,
  edgeType: string,
  label?: string,
): Promise<MemoryEdge> {
  return invokeNative("memory_edge_v1_add", memoryEdgeSchema, {
    request: {
      canvas_id: canvasId,
      source_node_id: sourceNodeId,
      target_node_id: targetNodeId,
      edge_type: edgeType,
      label,
    },
  });
}

export async function deleteEdge(id: string): Promise<void> {
  await invokeNative("memory_edge_v1_delete", voidResponse, {
    request: { id },
  });
}

export async function listEdges(canvasId: string): Promise<MemoryEdge[]> {
  return invokeNative("memory_edge_v1_list", edgeListSchema, {
    request: { canvas_id: canvasId },
  });
}

// ── Viewport IPC ─────────────────────────────────────

export async function saveViewport(
  canvasId: string,
  zoom: number,
  panX: number,
  panY: number,
): Promise<void> {
  await invokeNative("memory_viewport_v1_save", voidResponse, {
    request: { canvas_id: canvasId, zoom, pan_x: panX, pan_y: panY },
  });
}

export async function getViewport(canvasId: string): Promise<MemoryViewport> {
  return invokeNative("memory_viewport_v1_get", memoryViewportSchema, {
    request: { canvas_id: canvasId },
  });
}
