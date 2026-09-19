import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import type { MemoryEdge, MemoryNode } from "../../../bridge/memoryCanvas";
import {
  AddEdgeCommand,
  AddNodeCommand,
  CanvasHistory,
  MacroCommand,
  MoveNodeCommand,
  RemoveEdgeCommand,
  RemoveNodeCommand,
  UpdatePayloadCommand,
} from "./canvasCommands";
import type { CanvasCommandContext, EdgeDraft, NodeDraft } from "./canvasCommands";

const T = "2026-01-01T00:00:00Z";

function makeNode(id: string, draft: Partial<NodeDraft> & { canvasId: string }): MemoryNode {
  return {
    id,
    canvasId: draft.canvasId,
    nodeType: draft.nodeType ?? "note",
    positionX: draft.positionX ?? 0,
    positionY: draft.positionY ?? 0,
    width: null,
    height: null,
    payloadJson: draft.payloadJson ?? "{}",
    summary: draft.summary ?? null,
    assetId: draft.assetId ?? null,
    createdAt: T,
    updatedAt: T,
  };
}

function makeEdge(id: string, draft: EdgeDraft): MemoryEdge {
  return {
    id,
    canvasId: draft.canvasId,
    sourceNodeId: draft.sourceNodeId,
    targetNodeId: draft.targetNodeId,
    edgeType: draft.edgeType,
    label: draft.label ?? null,
    createdAt: T,
  };
}

function createFakeContext() {
  const nodes = ref<MemoryNode[]>([]);
  const edges = ref<MemoryEdge[]>([]);
  const selectedIds = ref<Set<string>>(new Set());
  const remap = new Map<string, string>();
  let seq = 0;
  const addedNodes: NodeDraft[] = [];
  const deletedNodeIds: string[] = [];
  const addedEdges: EdgeDraft[] = [];
  const deletedEdgeIds: string[] = [];
  const scheduledWrites: string[] = [];

  const ctx: CanvasCommandContext = {
    nodes,
    edges,
    selectedIds,
    remap,
    addNodeIpc: async (draft) => {
      addedNodes.push(draft);
      seq += 1;
      return makeNode(`srv-${seq}`, draft);
    },
    deleteNodeIpc: async (id) => {
      deletedNodeIds.push(id);
    },
    addEdgeIpc: async (draft) => {
      addedEdges.push(draft);
      seq += 1;
      return makeEdge(`srv-${seq}`, draft);
    },
    deleteEdgeIpc: async (id) => {
      deletedEdgeIds.push(id);
    },
    scheduleNodeWrite: (id) => {
      scheduledWrites.push(id);
    },
  };
  return {
    ctx,
    nodes,
    edges,
    selectedIds,
    remap,
    addedNodes,
    deletedNodeIds,
    addedEdges,
    deletedEdgeIds,
    scheduledWrites,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("canvasCommands", () => {
  it("AddNodeCommand: redo 创建、undo 删除、再 redo 重建并重映射 id", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const command = new AddNodeCommand(fake.ctx, {
      canvasId: "c1",
      nodeType: "note",
      positionX: 10,
      positionY: 20,
      payloadJson: "{}",
    });
    await history.push(command);

    expect(fake.nodes.value).toHaveLength(1);
    const firstId = command.createdNode?.id;
    expect(firstId).toBeDefined();

    await history.undo();
    expect(fake.nodes.value).toHaveLength(0);
    expect(fake.deletedNodeIds).toEqual([firstId]);

    await history.redo();
    expect(fake.nodes.value).toHaveLength(1);
    const secondId = fake.nodes.value[0]?.id;
    expect(secondId).not.toBe(firstId);
    expect(fake.remap.get(firstId ?? "")).toBe(secondId);
  });

  it("MoveNodeCommand: 同节点连续移动在时间窗内合并为一条历史", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const seed = makeNode("n1", { canvasId: "c1", positionX: 10, positionY: 10 });
    fake.nodes.value = [seed];

    await history.push(new MoveNodeCommand(fake.ctx, "n1", { x: 10, y: 10 }, { x: 20, y: 20 }));
    await vi.advanceTimersByTimeAsync(100);
    await history.push(new MoveNodeCommand(fake.ctx, "n1", { x: 20, y: 20 }, { x: 30, y: 30 }));

    expect(history.undoStack.value).toHaveLength(1);
    expect(fake.nodes.value[0]?.positionX).toBe(30);
    expect(fake.scheduledWrites).toEqual(["n1", "n1"]);

    await history.undo();
    expect(fake.nodes.value[0]?.positionX).toBe(10);
  });

  it("MoveNodeCommand: 超出时间窗不再合并", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const seed = makeNode("n1", { canvasId: "c1", positionX: 10, positionY: 10 });
    fake.nodes.value = [seed];

    await history.push(new MoveNodeCommand(fake.ctx, "n1", { x: 10, y: 10 }, { x: 20, y: 20 }));
    await vi.advanceTimersByTimeAsync(1000);
    await history.push(new MoveNodeCommand(fake.ctx, "n1", { x: 20, y: 20 }, { x: 30, y: 30 }));

    expect(history.undoStack.value).toHaveLength(2);
  });

  it("RemoveNodeCommand: 级联删除连线，undo 重建并把端点重映射到新节点 id", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const nodeA = makeNode("a", { canvasId: "c1" });
    const nodeB = makeNode("b", { canvasId: "c1", positionX: 100, positionY: 0 });
    const edge = makeEdge("e1", {
      canvasId: "c1",
      sourceNodeId: "a",
      targetNodeId: "b",
      edgeType: "reference",
    });
    fake.nodes.value = [nodeA, nodeB];
    fake.edges.value = [edge];

    await history.push(new RemoveNodeCommand(fake.ctx, nodeA, [edge]));
    expect(fake.nodes.value.map((n) => n.id)).toEqual(["b"]);
    expect(fake.edges.value).toHaveLength(0);
    expect(fake.deletedNodeIds).toContain("a");
    expect(fake.deletedEdgeIds).toContain("e1");

    await history.undo();
    const restoredNode = fake.nodes.value.find((n) => n.id !== "b");
    expect(restoredNode).toBeDefined();
    expect(restoredNode?.id).not.toBe("a"); // 服务端重新生成 id
    expect(fake.remap.get("a")).toBe(restoredNode?.id);
    expect(fake.edges.value).toHaveLength(1);
    expect(fake.edges.value[0]?.sourceNodeId).toBe(restoredNode?.id);
    expect(fake.edges.value[0]?.targetNodeId).toBe("b");
  });

  it("RemoveEdgeCommand: undo 重建连线并登记 remap", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const edge = makeEdge("e1", {
      canvasId: "c1",
      sourceNodeId: "a",
      targetNodeId: "b",
      edgeType: "reference",
    });
    fake.edges.value = [edge];

    await history.push(new RemoveEdgeCommand(fake.ctx, edge));
    expect(fake.edges.value).toHaveLength(0);

    await history.undo();
    expect(fake.edges.value).toHaveLength(1);
    expect(fake.edges.value[0]?.id).not.toBe("e1");
    expect(fake.remap.get("e1")).toBe(fake.edges.value[0]?.id);
  });

  it("AddEdgeCommand: redo 经 remap 解析端点", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    fake.remap.set("a", "a-new");
    const command = new AddEdgeCommand(fake.ctx, {
      canvasId: "c1",
      sourceNodeId: "a",
      targetNodeId: "b",
      edgeType: "reference",
    });
    await history.push(command);

    expect(fake.addedEdges[0]?.sourceNodeId).toBe("a-new");
    expect(fake.edges.value).toHaveLength(1);
  });

  it("UpdatePayloadCommand: redo 应用新 payload，undo 还原，并调度写库", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const seed = makeNode("n1", {
      canvasId: "c1",
      payloadJson: JSON.stringify({ text: "旧" }),
      summary: "旧",
    });
    fake.nodes.value = [seed];

    const command = new UpdatePayloadCommand(
      fake.ctx,
      "n1",
      { payloadJson: JSON.stringify({ text: "旧" }), summary: "旧" },
      { payloadJson: JSON.stringify({ text: "新" }), summary: "新" },
      "编辑便签",
    );
    await history.push(command);

    expect(JSON.parse(fake.nodes.value[0]?.payloadJson ?? "{}")).toEqual({ text: "新" });
    expect(fake.nodes.value[0]?.summary).toBe("新");
    expect(fake.scheduledWrites).toContain("n1");

    await history.undo();
    expect(JSON.parse(fake.nodes.value[0]?.payloadJson ?? "{}")).toEqual({ text: "旧" });
    expect(fake.nodes.value[0]?.summary).toBe("旧");
  });

  it("MacroCommand: undo 逆序执行", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    const first = new AddNodeCommand(fake.ctx, {
      canvasId: "c1",
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const second = new AddNodeCommand(fake.ctx, {
      canvasId: "c1",
      nodeType: "note",
      positionX: 10,
      positionY: 0,
      payloadJson: "{}",
    });
    await history.push(new MacroCommand([first, second], "批量添加"));

    expect(fake.nodes.value).toHaveLength(2);
    await history.undo();
    expect(fake.nodes.value).toHaveLength(0);
    // 逆序：后添加的先删除
    expect(fake.deletedNodeIds[0]).toBe(second.createdNode?.id);
  });

  it("CanvasHistory: 上限 50 条，clear 清空", async () => {
    const fake = createFakeContext();
    const history = new CanvasHistory();
    for (let i = 0; i < 60; i += 1) {
      await vi.advanceTimersByTimeAsync(1000); // 避免同 key 合并
      await history.push(new MoveNodeCommand(fake.ctx, `n${i}`, { x: 0, y: 0 }, { x: i, y: 0 }));
    }
    expect(history.undoStack.value).toHaveLength(50);
    expect(history.canUndo).toBe(true);
    expect(history.canRedo).toBe(false);
    history.clear();
    expect(history.canUndo).toBe(false);
  });
});
