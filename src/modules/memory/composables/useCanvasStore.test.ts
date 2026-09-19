import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  MemoryCanvas,
  MemoryEdge,
  MemoryNode,
  MemoryViewport,
} from "../../../bridge/memoryCanvas";
import { PERSIST_DEBOUNCE_MS, createCanvasStore } from "./useCanvasStore";
import type { CanvasStoreIpc } from "./useCanvasStore";

const T = "2026-01-01T00:00:00Z";

function makeNode(id: string, canvasId: string, overrides: Partial<MemoryNode> = {}): MemoryNode {
  return {
    id,
    canvasId,
    nodeType: "note",
    positionX: 0,
    positionY: 0,
    width: null,
    height: null,
    payloadJson: "{}",
    summary: null,
    assetId: null,
    createdAt: T,
    updatedAt: T,
    ...overrides,
  };
}

function makeEdge(
  id: string,
  canvasId: string,
  sourceNodeId: string,
  targetNodeId: string,
): MemoryEdge {
  return {
    id,
    canvasId,
    sourceNodeId,
    targetNodeId,
    edgeType: "reference",
    label: null,
    createdAt: T,
  };
}

function createFakeIpc() {
  const canvases: MemoryCanvas[] = [];
  const nodeRows = new Map<string, MemoryNode>();
  const edgeRows = new Map<string, MemoryEdge>();
  const viewportRows = new Map<string, MemoryViewport>();
  const calls = {
    addNode: 0,
    updateNode: 0,
    deleteNode: 0,
    addEdge: 0,
    deleteEdge: 0,
    saveViewport: 0,
  };
  let canvasSeq = 0;
  let nodeSeq = 0;
  let edgeSeq = 0;

  const ipc: CanvasStoreIpc = {
    listCanvases: async () => [...canvases],
    createCanvas: async (workspaceId, name) => {
      canvasSeq += 1;
      const canvas: MemoryCanvas = {
        id: `canvas-${canvasSeq}`,
        workspaceId,
        name,
        description: null,
        nodeCount: 0,
        edgeCount: 0,
        createdAt: T,
        updatedAt: T,
      };
      canvases.push(canvas);
      return canvas;
    },
    listNodes: async (canvasId) => [...nodeRows.values()].filter((n) => n.canvasId === canvasId),
    listEdges: async (canvasId) => [...edgeRows.values()].filter((e) => e.canvasId === canvasId),
    getViewport: async (canvasId) => viewportRows.get(canvasId) ?? null,
    addNode: async (input) => {
      calls.addNode += 1;
      nodeSeq += 1;
      const node = makeNode(`srv-node-${nodeSeq}`, input.canvasId, {
        nodeType: input.nodeType,
        positionX: input.positionX,
        positionY: input.positionY,
        payloadJson: input.payloadJson,
        summary: input.summary ?? null,
        assetId: input.assetId ?? null,
      });
      nodeRows.set(node.id, node);
      return node;
    },
    updateNode: async (node) => {
      calls.updateNode += 1;
      nodeRows.set(node.id, node);
      return node;
    },
    deleteNode: async (id) => {
      calls.deleteNode += 1;
      nodeRows.delete(id);
    },
    addEdge: async (input) => {
      calls.addEdge += 1;
      edgeSeq += 1;
      const edge = makeEdge(
        `srv-edge-${edgeSeq}`,
        input.canvasId,
        input.sourceNodeId,
        input.targetNodeId,
      );
      edgeRows.set(edge.id, edge);
      return edge;
    },
    deleteEdge: async (id) => {
      calls.deleteEdge += 1;
      edgeRows.delete(id);
    },
    saveViewport: async (canvasId, zoom, panX, panY) => {
      calls.saveViewport += 1;
      viewportRows.set(canvasId, { canvasId, zoom, panX, panY, updatedAt: T });
    },
  };
  return { ipc, canvases, nodeRows, edgeRows, viewportRows, calls };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useCanvasStore", () => {
  it("loadCanvas：找不到画布时创建，再次加载复用", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);

    await store.loadCanvas("conv-1");
    expect(store.canvasId.value).toBe("canvas-1");
    expect(fake.canvases).toHaveLength(1);

    await store.loadCanvas("conv-1");
    expect(store.canvasId.value).toBe("canvas-1");
    expect(fake.canvases).toHaveLength(1);
  });

  it("addNode：入状态并返回服务端 id，新增即时落库不走防抖", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");

    const node = await store.addNode({
      nodeType: "note",
      positionX: 10.4,
      positionY: 20,
      payloadJson: "{}",
      summary: "便签",
    });

    expect(node?.id).toBe("srv-node-1");
    expect(store.nodes.value.map((n) => n.id)).toEqual(["srv-node-1"]);
    expect(node?.positionX).toBe(10); // 吸附取整
    expect(fake.calls.addNode).toBe(1);
    expect(fake.calls.updateNode).toBe(0);
  });

  it("moveNodes：写库防抖 500ms 合并，连续拖拽只产生一条历史", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const id = node?.id ?? "";

    await store.moveNodes([{ id, x: 100, y: 100 }]);
    await vi.advanceTimersByTimeAsync(100);
    await store.moveNodes([{ id, x: 200, y: 200 }]);
    expect(fake.calls.updateNode).toBe(0); // 窗口内未落库

    await vi.advanceTimersByTimeAsync(PERSIST_DEBOUNCE_MS);
    expect(fake.calls.updateNode).toBe(1); // 合并为一次写库
    const persisted = fake.nodeRows.get(id);
    expect(persisted?.positionX).toBe(200);

    // 连续两次拖拽合并为一条历史：一次 undo 回到起点
    await store.undo();
    expect(store.nodes.value.find((n) => n.id === id)?.positionX).toBe(0);
  });

  it("flushPendingWrites：节点已删除则丢弃待写", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const id = node?.id ?? "";

    await store.moveNodes([{ id, x: 50, y: 50 }]);
    await store.removeNode(id);
    await vi.advanceTimersByTimeAsync(PERSIST_DEBOUNCE_MS);

    expect(fake.calls.updateNode).toBe(0);
  });

  it("removeNode：级联删除连线，undo 整体重建且端点指向新节点", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const a = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const b = await store.addNode({
      nodeType: "fact",
      positionX: 100,
      positionY: 0,
      payloadJson: "{}",
    });
    await store.connectNodes(a?.id ?? "", b?.id ?? "");
    expect(fake.edgeRows.size).toBe(1);

    await store.removeNode(a?.id ?? "");
    expect(store.nodes.value.map((n) => n.id)).toEqual([b?.id]);
    expect(store.edges.value).toHaveLength(0);
    expect(fake.nodeRows.has(a?.id ?? "")).toBe(false);

    await store.undo();
    const restoredA = store.nodes.value.find((n) => n.id === a?.id);
    expect(restoredA).toBeUndefined(); // 旧 id 不会复用
    expect(store.nodes.value).toHaveLength(2);
    const newA = store.nodes.value.find((n) => n.id !== b?.id);
    expect(store.edges.value[0]?.sourceNodeId).toBe(newA?.id);
    // DB 行同步一致
    expect(fake.edgeRows.get(store.edges.value[0]?.id ?? "")?.sourceNodeId).toBe(newA?.id);
  });

  it("updateNoteText：更新 payload/摘要，undo 还原", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNoteNodeAt(10, 10);
    const id = node?.id ?? "";

    await store.updateNoteText(id, "新内容");
    const updated = store.nodes.value.find((n) => n.id === id);
    expect(JSON.parse(updated?.payloadJson ?? "{}").text).toBe("新内容");
    expect(updated?.summary).toBe("新内容");

    await vi.advanceTimersByTimeAsync(PERSIST_DEBOUNCE_MS);
    expect(JSON.parse(fake.nodeRows.get(id)?.payloadJson ?? "{}").text).toBe("新内容");

    await store.undo();
    const restored = store.nodes.value.find((n) => n.id === id);
    expect(JSON.parse(restored?.payloadJson ?? "{}").text).toBe("双击编辑便签");
  });

  it("resizeNode：尺寸写入 payload.size 并落库", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNoteNodeAt(0, 0);
    const id = node?.id ?? "";

    await store.resizeNode(id, 400, 300);
    await vi.advanceTimersByTimeAsync(PERSIST_DEBOUNCE_MS);

    const payload = JSON.parse(fake.nodeRows.get(id)?.payloadJson ?? "{}");
    expect(payload.size).toEqual({ width: 400, height: 300 });
  });

  it("connectNodes：重复/反向/自连被拦截，端点重连保留类型并整体可撤销", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const a = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const b = await store.addNode({
      nodeType: "note",
      positionX: 100,
      positionY: 0,
      payloadJson: "{}",
    });
    const c = await store.addNode({
      nodeType: "note",
      positionX: 200,
      positionY: 0,
      payloadJson: "{}",
    });

    expect(await store.connectNodes(a?.id ?? "", b?.id ?? "")).toBe("created");
    expect(await store.connectNodes(a?.id ?? "", b?.id ?? "")).toBe("duplicate");
    expect(await store.connectNodes(b?.id ?? "", a?.id ?? "")).toBe("reverse");
    expect(await store.connectNodes(a?.id ?? "", a?.id ?? "")).toBe("self");
    expect(fake.edgeRows.size).toBe(1);

    // 端点重连：a→b 改为 a→c
    const edgeId = store.edges.value[0]?.id ?? "";
    expect(await store.reconnectEdge(edgeId, a?.id ?? "", c?.id ?? "")).toBe(true);
    expect(store.edges.value).toHaveLength(1);
    expect(store.edges.value[0]?.targetNodeId).toBe(c?.id);

    // 撤销：恢复原端点 a→b（新边被删、旧边经 remap 还原）
    await store.undo();
    expect(store.edges.value).toHaveLength(1);
    expect(store.edges.value[0]?.sourceNodeId).toBe(a?.id);
    expect(store.edges.value[0]?.targetNodeId).toBe(b?.id);

    // 自连拒绝
    expect(
      await store.reconnectEdge(store.edges.value[0]?.id ?? "", a?.id ?? "", a?.id ?? ""),
    ).toBe(false);
  });

  it("deleteSelected：多选删除打包为一条历史", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const a = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });
    const b = await store.addNode({
      nodeType: "note",
      positionX: 100,
      positionY: 0,
      payloadJson: "{}",
    });
    await store.connectNodes(a?.id ?? "", b?.id ?? "");

    store.handleNodeChanges([
      { type: "select", id: a?.id ?? "", selected: true },
      { type: "select", id: b?.id ?? "", selected: true },
    ]);
    await store.deleteSelected();

    expect(store.nodes.value).toHaveLength(0);
    expect(store.edges.value).toHaveLength(0);

    await store.undo();
    expect(store.nodes.value).toHaveLength(2);
    expect(store.edges.value).toHaveLength(1);
  });

  it("autoArrange：有位移才入栈，undo 恢复原位置", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNode({
      nodeType: "note",
      positionX: 500,
      positionY: 500,
      payloadJson: "{}",
    });

    await store.autoArrange();
    const arranged = store.nodes.value.find((n) => n.id === node?.id);
    expect(arranged?.positionX).toBe(60); // 第一列
    expect(store.canUndo.value).toBe(true);

    await store.undo();
    expect(store.nodes.value.find((n) => n.id === node?.id)?.positionX).toBe(500);
  });

  it("loadCanvas：清理端点缺失的孤立边", async () => {
    const fake = createFakeIpc();
    // 预置已存在画布 + 一个有效节点 + 一条有效边 + 一条悬挂边
    fake.canvases.push({
      id: "canvas-x",
      workspaceId: "default",
      name: "conv-test",
      description: null,
      nodeCount: 0,
      edgeCount: 0,
      createdAt: T,
      updatedAt: T,
    });
    const node = makeNode("n1", "canvas-x");
    fake.nodeRows.set("n1", node);
    fake.edgeRows.set("e-ok", makeEdge("e-ok", "canvas-x", "n1", "n1"));
    fake.edgeRows.set("e-dangling", makeEdge("e-dangling", "canvas-x", "n1", "ghost"));

    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("test");

    expect(store.edges.value.map((e) => e.id)).toEqual(["e-ok"]);
    expect(fake.edgeRows.has("e-dangling")).toBe(false);
    expect(fake.calls.deleteEdge).toBe(1);
    expect(store.loadError.value).toBeNull();
  });

  it("loadCanvas：加载失败时暴露 loadError 供重试", async () => {
    const fake = createFakeIpc();
    let failuresLeft = 1;
    const store = createCanvasStore({
      ...fake.ipc,
      listCanvases: async () => {
        if (failuresLeft > 0) {
          failuresLeft -= 1;
          throw new Error("db locked");
        }
        return fake.ipc.listCanvases("default");
      },
    });

    await store.loadCanvas("test");

    expect(store.canvasId.value).toBeNull();
    expect(store.loading.value).toBe(false);
    expect(store.loadError.value).toContain("db locked");

    // 恢复后再次加载即可清除错误
    await store.loadCanvas("test");
    expect(store.loadError.value).toBeNull();
    expect(store.canvasId.value).toBe("canvas-1");
  });

  it("saveViewport：立即写库并更新本地视口", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");

    await store.saveViewport(1.5, 12, 34);
    expect(store.viewport.value).toEqual({ zoom: 1.5, x: 12, y: 34 });
    expect(fake.calls.saveViewport).toBe(1);
  });

  it("loadCanvas：切换画布前 flush 待写数据", async () => {
    const fake = createFakeIpc();
    const store = createCanvasStore(fake.ipc);
    await store.loadCanvas("conv-1");
    const node = await store.addNode({
      nodeType: "note",
      positionX: 0,
      positionY: 0,
      payloadJson: "{}",
    });

    await store.moveNodes([{ id: node?.id ?? "", x: 80, y: 80 }]);
    await store.loadCanvas("conv-2"); // 未等防抖窗口直接切换

    expect(fake.calls.updateNode).toBe(1);
    expect(fake.nodeRows.get(node?.id ?? "")?.positionX).toBe(80);
  });
});
