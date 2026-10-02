/**
 * useCanvasStore：画布集中状态管理（Phase 2 数据层重构）。
 *
 * 集中管理 nodes / edges / viewport / selection / history 与所有 mutations；
 * MemoryCanvasPanel 只做渲染和事件转发，不直接操作状态。
 *
 * 持久化策略：节点内容/位置写库经 500ms 防抖合并（连续 move 只落一次库，
 * flush 时读取最新本地状态）；新增/删除与视口写入保持即时。离开画布
 * （loadCanvas 切换 / 面板卸载）时立即 flush 待写数据。
 */
import { computed, ref } from "vue";
import type {
  MemoryCanvas,
  MemoryEdge,
  MemoryNode,
  MemoryViewport,
} from "../../../bridge/memoryCanvas";
import {
  addEdge as addEdgeRpc,
  addNode as addNodeRpc,
  createCanvas as createCanvasRpc,
  deleteEdge as deleteEdgeRpc,
  updateEdgeLabel as updateEdgeLabelRpc,
  deleteNode as deleteNodeRpc,
  getViewport as getViewportRpc,
  listCanvases as listCanvasesRpc,
  listEdges as listEdgesRpc,
  listNodes as listNodesRpc,
  saveViewport as saveViewportRpc,
  updateNode as updateNodeRpc,
} from "../../../bridge/memoryCanvas";
import { clampNodeSize } from "../canvasNodeSize";
import { compressIfNeeded } from "../imageCompress";
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
import type { CanvasCommand, CanvasCommandContext, EdgeDraft, NodeDraft } from "./canvasCommands";

/** 节点写库防抖窗口。 */
export const PERSIST_DEBOUNCE_MS = 500;

/** 便签循环配色（payload.color）。 */
export const NOTE_COLORS = ["#fbbf24", "#60a5fa", "#34d399", "#f472b6"];

// ── IPC 适配层（与 bridge 一一对应，测试可整体伪造） ───

export interface NewNodeInput {
  nodeType: string;
  positionX: number;
  positionY: number;
  payloadJson: string;
  summary?: string;
  assetId?: string;
}

export interface CanvasStoreIpc {
  listCanvases(workspaceId: string): Promise<MemoryCanvas[]>;
  createCanvas(workspaceId: string, name: string): Promise<MemoryCanvas>;
  listNodes(canvasId: string): Promise<MemoryNode[]>;
  listEdges(canvasId: string): Promise<MemoryEdge[]>;
  getViewport(canvasId: string): Promise<MemoryViewport | null>;
  addNode(input: NewNodeInput & { canvasId: string }): Promise<MemoryNode>;
  updateNode(node: MemoryNode): Promise<MemoryNode>;
  deleteNode(id: string): Promise<void>;
  addEdge(input: {
    canvasId: string;
    sourceNodeId: string;
    targetNodeId: string;
    edgeType: string;
    label?: string;
  }): Promise<MemoryEdge>;
  deleteEdge(id: string): Promise<void>;
  saveViewport(canvasId: string, zoom: number, panX: number, panY: number): Promise<void>;
}

const bridgeIpc: CanvasStoreIpc = {
  listCanvases: (workspaceId) => listCanvasesRpc(workspaceId),
  createCanvas: (workspaceId, name) => createCanvasRpc(workspaceId, name),
  listNodes: (canvasId) => listNodesRpc(canvasId),
  listEdges: (canvasId) => listEdgesRpc(canvasId),
  getViewport: async (canvasId) => {
    try {
      return await getViewportRpc(canvasId);
    } catch {
      return null; // 尚未保存过视口
    }
  },
  addNode: (input) =>
    addNodeRpc(
      input.canvasId,
      input.nodeType,
      input.positionX,
      input.positionY,
      input.payloadJson,
      input.summary,
      input.assetId,
    ),
  updateNode: (node) =>
    updateNodeRpc(
      node.id,
      node.canvasId,
      node.nodeType,
      node.positionX,
      node.positionY,
      node.payloadJson,
      node.summary ?? undefined,
      node.assetId ?? undefined,
    ),
  deleteNode: (id) => deleteNodeRpc(id),
  addEdge: (input) =>
    addEdgeRpc(input.canvasId, input.sourceNodeId, input.targetNodeId, input.edgeType, input.label),
  deleteEdge: (id) => deleteEdgeRpc(id),
  saveViewport: (canvasId, zoom, panX, panY) => saveViewportRpc(canvasId, zoom, panX, panY),
};

/** vue-flow 变更的最小结构（节点/边变更共用；add 变更无 id，处理时跳过）。 */
export interface CanvasChange {
  type: string;
  id?: string;
  selected?: boolean;
}

export interface ViewportState {
  zoom: number;
  x: number;
  y: number;
}

export function createCanvasStore(ipc: CanvasStoreIpc = bridgeIpc) {
  // ── 响应式状态 ───
  const nodes = ref<MemoryNode[]>([]);
  const edges = ref<MemoryEdge[]>([]);
  const canvasId = ref<string | null>(null);
  const loading = ref(false);
  const loadError = ref<string | null>(null);
  const selectedIds = ref<Set<string>>(new Set());
  const viewport = ref<ViewportState | null>(null);

  const history = new CanvasHistory();
  const remap = new Map<string, string>();
  const canUndo = computed(() => history.canUndo);
  const canRedo = computed(() => history.canRedo);

  // ── 500ms 防抖批量写库（合并连续 move/payload 更新） ───

  const pendingNodeWrites = new Set<string>();
  let flushTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleNodeWrite(id: string): void {
    pendingNodeWrites.add(id);
    if (flushTimer !== null) return;
    flushTimer = setTimeout(() => {
      flushTimer = null;
      void flushPendingWrites();
    }, PERSIST_DEBOUNCE_MS);
  }

  /** 立即写入所有待写节点（读取最新本地状态；节点已删除则丢弃）。 */
  async function flushPendingWrites(): Promise<void> {
    if (flushTimer !== null) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    const ids = [...pendingNodeWrites];
    pendingNodeWrites.clear();
    for (const id of ids) {
      const node = nodes.value.find((n) => n.id === id);
      if (!node || node.canvasId !== canvasId.value) continue;
      try {
        await ipc.updateNode(node);
      } catch (e) {
        console.warn("[CanvasStore] flush node write failed:", e);
      }
    }
  }

  // ── 命令上下文 ───

  const commandContext: CanvasCommandContext = {
    nodes,
    edges,
    selectedIds,
    remap,
    addNodeIpc: async (draft: NodeDraft) =>
      ipc.addNode({
        canvasId: draft.canvasId,
        nodeType: draft.nodeType,
        positionX: draft.positionX,
        positionY: draft.positionY,
        payloadJson: draft.payloadJson,
        summary: draft.summary ?? undefined,
        assetId: draft.assetId ?? undefined,
      }),
    deleteNodeIpc: (id) => ipc.deleteNode(id),
    addEdgeIpc: async (draft: EdgeDraft) =>
      ipc.addEdge({
        canvasId: draft.canvasId,
        sourceNodeId: draft.sourceNodeId,
        targetNodeId: draft.targetNodeId,
        edgeType: draft.edgeType,
        label: draft.label ?? undefined,
      }),
    deleteEdgeIpc: (id) => ipc.deleteEdge(id),
    scheduleNodeWrite,
  };

  function findNode(id: string): MemoryNode | undefined {
    return nodes.value.find((n) => n.id === id);
  }

  // ── 画布生命周期 ───

  async function loadCanvas(conversationId: string | null): Promise<void> {
    await flushPendingWrites(); // 离开画布前把待写数据落库
    history.clear();
    remap.clear();
    selectedIds.value = new Set();
    viewport.value = null;
    nodes.value = [];
    edges.value = [];
    canvasId.value = null;
    loadError.value = null;
    if (!conversationId) return;
    loading.value = true;
    try {
      const canvases = await ipc.listCanvases("default");
      const existing = canvases.find((c) => c.name === `conv-${conversationId}`);
      if (existing) {
        canvasId.value = existing.id;
        const rawNodes = await ipc.listNodes(existing.id);
        const rawEdges = await ipc.listEdges(existing.id);
        // 数据完整性校验：丢弃畸形节点（缺 id / 坐标非法）
        const validNodes = rawNodes.filter(
          (n) => n.id.length > 0 && Number.isFinite(n.positionX) && Number.isFinite(n.positionY),
        );
        const nodeIds = new Set(validNodes.map((n) => n.id));
        // 孤立边清理：端点节点已不存在的连线直接删除
        const danglingEdges = rawEdges.filter(
          (e) => e.id.length === 0 || !nodeIds.has(e.sourceNodeId) || !nodeIds.has(e.targetNodeId),
        );
        for (const edge of danglingEdges) {
          if (edge.id.length === 0) continue;
          try {
            await ipc.deleteEdge(edge.id);
          } catch (e) {
            console.warn("[CanvasStore] dangling edge cleanup failed:", e);
          }
        }
        if (danglingEdges.length > 0) {
          console.info(`[CanvasStore] cleaned ${danglingEdges.length} dangling edge(s)`);
        }
        nodes.value = validNodes;
        edges.value = rawEdges.filter((e) => !danglingEdges.some((d) => d.id === e.id));
        const viewportRow = await ipc.getViewport(existing.id);
        viewport.value = viewportRow
          ? { zoom: viewportRow.zoom, x: viewportRow.panX, y: viewportRow.panY }
          : null;
      } else {
        const canvas = await ipc.createCanvas("default", `conv-${conversationId}`);
        canvasId.value = canvas.id;
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      loadError.value = message;
      console.error("[CanvasStore] loadCanvas error:", e);
    } finally {
      loading.value = false;
    }
  }

  /** 更新连线标签：即时 IPC + 本地同步（不入命令历史）。 */
  async function updateEdgeLabel(edgeId: string, label: string): Promise<void> {
    await updateEdgeLabelRpc(edgeId, label);
    edges.value = edges.value.map((e) =>
      e.id === edgeId ? { ...e, label: label.trim() === "" ? null : label } : e,
    );
  }

  /** Agent 画布工具直接写库后的实时刷新（与前端状态对齐）。 */
  async function refreshNodes(): Promise<void> {
    const cid = canvasId.value;
    if (!cid) return;
    try {
      nodes.value = await ipc.listNodes(cid);
      edges.value = await ipc.listEdges(cid);
    } catch (e) {
      console.warn("[CanvasStore] refresh nodes failed:", e);
    }
  }

  async function saveViewport(zoom: number, x: number, y: number): Promise<void> {
    if (!canvasId.value) return;
    viewport.value = { zoom, x, y };
    try {
      await ipc.saveViewport(canvasId.value, zoom, x, y);
    } catch (e) {
      console.warn("[CanvasStore] save viewport failed:", e);
    }
  }

  // ── 选择（vue-flow 变更转发入口） ───

  function toggleSelected(id: string, selected: boolean): void {
    const next = new Set(selectedIds.value);
    if (selected) next.add(id);
    else next.delete(id);
    selectedIds.value = next;
  }

  function handleNodeChanges(changes: CanvasChange[]): void {
    for (const change of changes) {
      if (!change.id) continue;
      if (change.type === "select") toggleSelected(change.id, change.selected === true);
      else if (change.type === "remove") void removeNode(change.id);
    }
  }

  function handleEdgeChanges(changes: CanvasChange[]): void {
    for (const change of changes) {
      if (!change.id) continue;
      if (change.type === "select") toggleSelected(change.id, change.selected === true);
      else if (change.type === "remove") void removeEdge(change.id);
    }
  }

  // ── 节点 mutations ───

  async function addNode(draft: NewNodeInput): Promise<MemoryNode | null> {
    if (!canvasId.value) return null;
    const command = new AddNodeCommand(commandContext, {
      canvasId: canvasId.value,
      nodeType: draft.nodeType,
      positionX: Math.round(draft.positionX),
      positionY: Math.round(draft.positionY),
      payloadJson: draft.payloadJson,
      summary: draft.summary,
      assetId: draft.assetId,
    });
    await history.push(command);
    return command.createdNode;
  }

  async function addNoteNodeAt(flowX: number, flowY: number): Promise<MemoryNode | null> {
    const text = "双击编辑便签";
    return addNode({
      nodeType: "note",
      positionX: flowX,
      positionY: flowY,
      payloadJson: JSON.stringify({ text, source: "manual" }),
      summary: text,
    });
  }

  async function addUploadNode(
    nodeType: string,
    name: string,
    dataUrl: string,
    description?: string,
  ): Promise<string | null> {
    // Phase 5：payload 超限的大图自动压缩（Canvas 不可用时原样返回）
    const finalDataUrl = await compressIfNeeded(dataUrl);
    const lastNode = nodes.value[nodes.value.length - 1];
    const x = lastNode ? lastNode.positionX + 260 : 40;
    const y = lastNode ? lastNode.positionY : 40;
    // 必须存完整 data URL：截断会让 base64 解码失败，节点图片永远无法渲染
    const payload: Record<string, string> = { source: "user-upload", dataUrl: finalDataUrl };
    if (description) payload.description = description;
    const node = await addNode({
      nodeType,
      positionX: x,
      positionY: y,
      payloadJson: JSON.stringify(payload),
      summary: name,
    });
    return node?.id ?? null;
  }

  async function addGenerationNode(
    nodeType: string,
    prompt: string,
    assetId?: string,
    name?: string,
  ): Promise<string | null> {
    const lastNode = nodes.value[nodes.value.length - 1];
    const x = lastNode ? lastNode.positionX + 260 : 40;
    const y = lastNode ? lastNode.positionY + (nodes.value.length % 3 === 0 ? -80 : 80) : 40;
    const node = await addNode({
      nodeType,
      positionX: x,
      positionY: y,
      payloadJson: JSON.stringify({ prompt, status: "pending" }),
      summary: name || prompt.slice(0, 50),
      assetId,
    });
    return node?.id ?? null;
  }

  /** 按 (dx,dy) 偏移克隆一批节点（粘贴/克隆共用），一次撤销步。 */
  async function cloneNodes(
    sources: MemoryNode[],
    offset: { x: number; y: number },
    message = "粘贴节点",
  ): Promise<MemoryNode[]> {
    const cid = canvasId.value;
    if (!cid || sources.length === 0) return [];
    const commands = sources.map(
      (src) =>
        new AddNodeCommand(
          commandContext,
          {
            canvasId: cid,
            nodeType: src.nodeType,
            positionX: Math.round(src.positionX + offset.x),
            positionY: Math.round(src.positionY + offset.y),
            payloadJson: src.payloadJson,
            summary: src.summary ?? undefined,
            assetId: src.assetId ?? undefined,
          },
          message,
        ),
    );
    const command: CanvasCommand =
      commands.length === 1 ? commands[0] : new MacroCommand(commands, message);
    await history.push(command);
    return commands.map((c) => c.createdNode).filter((n): n is MemoryNode => n !== null);
  }

  async function removeNode(id: string): Promise<void> {
    const node = findNode(id);
    if (!node) return;
    const connected = edges.value.filter((e) => e.sourceNodeId === id || e.targetNodeId === id);
    await history.push(new RemoveNodeCommand(commandContext, node, connected));
  }

  async function removeEdge(id: string): Promise<void> {
    const edge = edges.value.find((e) => e.id === id);
    if (!edge) return;
    await history.push(new RemoveEdgeCommand(commandContext, edge));
  }

  /** 连线结果：created=新建，duplicate=同向已存在，reverse=反向已存在，self=自连。 */
  type ConnectResult = "created" | "duplicate" | "reverse" | "self";

  async function connectNodes(
    sourceId: string,
    targetId: string,
    edgeType = "reference",
  ): Promise<ConnectResult> {
    const cid = canvasId.value;
    if (!cid || sourceId === targetId) return "self";
    const exists = edges.value.some(
      (e) => e.sourceNodeId === sourceId && e.targetNodeId === targetId && e.edgeType === edgeType,
    );
    if (exists) return "duplicate";
    // 反向已有连线时不再建反向边（视觉上两条线重叠，语义重复）
    const reversed = edges.value.some(
      (e) => e.sourceNodeId === targetId && e.targetNodeId === sourceId && e.edgeType === edgeType,
    );
    if (reversed) return "reverse";
    const command = new AddEdgeCommand(commandContext, {
      canvasId: cid,
      sourceNodeId: sourceId,
      targetNodeId: targetId,
      edgeType,
    });
    await history.push(command);
    return "created";
  }

  /** 端点重连：把已有连线换到新的两端（保留类型与标签），打包为一条可撤销历史。 */
  async function reconnectEdge(
    edgeId: string,
    sourceId: string,
    targetId: string,
  ): Promise<boolean> {
    const edge = edges.value.find((e) => e.id === edgeId);
    const cid = canvasId.value;
    if (!edge || !cid) return false;
    // 端点放回原位：无操作
    if (edge.sourceNodeId === sourceId && edge.targetNodeId === targetId) return true;
    if (sourceId === targetId) return false;
    // 换到的目标已有另一条同向边：拒绝（避免视觉重叠）
    const duplicated = edges.value.some(
      (e) =>
        e.id !== edgeId &&
        e.sourceNodeId === sourceId &&
        e.targetNodeId === targetId &&
        e.edgeType === edge.edgeType,
    );
    if (duplicated) return false;
    await history.push(
      new MacroCommand(
        [
          new RemoveEdgeCommand(commandContext, edge),
          new AddEdgeCommand(commandContext, {
            canvasId: edge.canvasId,
            sourceNodeId: sourceId,
            targetNodeId: targetId,
            edgeType: edge.edgeType,
            label: edge.label ?? undefined,
          }),
        ],
        "调整连线",
      ),
    );
    return true;
  }

  /** 拖拽结束的位置持久化入口；无位移的节点自动跳过。 */
  async function moveNodes(moves: { id: string; x: number; y: number }[]): Promise<void> {
    const commands: MoveNodeCommand[] = [];
    for (const move of moves) {
      const node = findNode(move.id);
      if (!node) continue;
      const x = Math.round(move.x);
      const y = Math.round(move.y);
      if (node.positionX === x && node.positionY === y) continue;
      commands.push(
        new MoveNodeCommand(
          commandContext,
          node.id,
          { x: node.positionX, y: node.positionY },
          { x, y },
        ),
      );
    }
    if (commands.length === 0) return;
    const command: CanvasCommand =
      commands.length === 1 ? commands[0] : new MacroCommand(commands, "移动节点");
    await history.push(command);
  }

  /** 自动整理：按类型分组排成竖列，打包为一条 MacroCommand。 */
  async function autoArrange(): Promise<void> {
    if (!canvasId.value) return;
    const order = ["upload", "image", "note", "video", "document", "fact"];
    const groups = new Map<string, MemoryNode[]>();
    for (const n of nodes.value) {
      const list = groups.get(n.nodeType) || [];
      list.push(n);
      groups.set(n.nodeType, list);
    }
    const sortedTypes = [...groups.keys()].sort((a, b) => order.indexOf(a) - order.indexOf(b));
    const commands: MoveNodeCommand[] = [];
    let column = 0;
    for (const nodeType of sortedTypes) {
      let row = 0;
      for (const n of groups.get(nodeType) || []) {
        const x = 60 + column * 280;
        const y = 60 + row * 260;
        if (n.positionX !== x || n.positionY !== y) {
          commands.push(
            new MoveNodeCommand(
              commandContext,
              n.id,
              { x: n.positionX, y: n.positionY },
              { x, y },
              "自动整理",
            ),
          );
        }
        row += 1;
      }
      column += 1;
    }
    if (commands.length === 0) return;
    await history.push(new MacroCommand(commands, "自动整理"));
  }

  async function deleteSelected(): Promise<void> {
    const commands: CanvasCommand[] = [];
    const consumedEdges = new Set<string>();
    for (const id of [...selectedIds.value]) {
      const node = findNode(id);
      if (!node) continue;
      const connected = edges.value.filter(
        (e) => (e.sourceNodeId === id || e.targetNodeId === id) && !consumedEdges.has(e.id),
      );
      for (const edge of connected) consumedEdges.add(edge.id);
      commands.push(new RemoveNodeCommand(commandContext, node, connected));
    }
    for (const id of [...selectedIds.value]) {
      if (consumedEdges.has(id)) continue;
      const edge = edges.value.find((e) => e.id === id);
      if (edge) commands.push(new RemoveEdgeCommand(commandContext, edge));
    }
    if (commands.length === 0) return;
    const command: CanvasCommand =
      commands.length === 1 ? commands[0] : new MacroCommand(commands, "删除选中");
    await history.push(command);
    selectedIds.value = new Set();
  }

  // ── payload mutations ───

  async function updateNodePayload(
    id: string,
    mutate: (payload: Record<string, unknown>) => void,
    message: string,
    options?: { summary?: string | null },
  ): Promise<void> {
    const node = findNode(id);
    if (!node) return;
    let payload: Record<string, unknown> = {};
    try {
      payload = JSON.parse(node.payloadJson) as Record<string, unknown>;
    } catch {
      payload = {};
    }
    const before = { payloadJson: node.payloadJson, summary: node.summary };
    mutate(payload);
    const after = {
      payloadJson: JSON.stringify(payload),
      summary: options?.summary !== undefined ? options.summary : node.summary,
    };
    if (after.payloadJson === before.payloadJson && after.summary === before.summary) return;
    await history.push(new UpdatePayloadCommand(commandContext, id, before, after, message));
  }

  function updateNoteText(id: string, text: string): Promise<void> {
    return updateNodePayload(id, (p) => void (p.text = text), "编辑便签", { summary: text });
  }

  function cycleNoteColor(id: string, colors: string[] = NOTE_COLORS): Promise<void> {
    return updateNodePayload(
      id,
      (p) => {
        const current = (p.color as string) || colors[0];
        p.color = colors[(colors.indexOf(current) + 1) % colors.length];
      },
      "换个颜色",
    );
  }

  /** 节点尺寸持久化在 payload.size（后端 update IPC 不透传 width/height 列）。 */
  function resizeNode(id: string, width: number, height?: number): Promise<void> {
    const size = clampNodeSize(width, height);
    return updateNodePayload(id, (p) => void (p.size = size), "调整节点大小");
  }

  function setNodeImageUrl(id: string, imageUrl: string): Promise<void> {
    return updateNodePayload(id, (p) => void (p.imageUrl = imageUrl), "更新节点图片");
  }

  /** 状态徽标仅本地更新（生成进行中的高频变更，不进历史、不落库）。 */
  function setNodeStatus(nodeId: string, status: string): void {
    const node = findNode(nodeId);
    if (!node) return;
    try {
      const payload = JSON.parse(node.payloadJson) as Record<string, unknown>;
      payload.status = status;
      const newPayload = JSON.stringify(payload);
      nodes.value = nodes.value.map((n) =>
        n.id === nodeId ? { ...n, payloadJson: newPayload } : n,
      );
    } catch {
      // ignore
    }
  }

  function findRecentNodesByType(nodeType: string, limit = 3): MemoryNode[] {
    return nodes.value.filter((n) => n.nodeType === nodeType).slice(-limit);
  }

  // ── 历史 ───

  function undo(): Promise<void> {
    return history.undo();
  }

  function redo(): Promise<void> {
    return history.redo();
  }

  return {
    // 状态
    nodes,
    edges,
    canvasId,
    loading,
    loadError,
    selectedIds,
    viewport,
    canUndo,
    canRedo,
    // 生命周期
    loadCanvas,
    saveViewport,
    flushPendingWrites,
    // 变更转发入口（vue-flow）
    handleNodeChanges,
    handleEdgeChanges,
    // 节点 mutations
    addNode,
    addNoteNodeAt,
    addUploadNode,
    addGenerationNode,
    cloneNodes,
    removeNode,
    moveNodes,
    autoArrange,
    deleteSelected,
    // 边 mutations
    connectNodes,
    reconnectEdge,
    updateEdgeLabel,
    refreshNodes,
    removeEdge,
    // payload mutations
    updateNoteText,
    cycleNoteColor,
    resizeNode,
    setNodeImageUrl,
    setNodeStatus,
    findRecentNodesByType,
    // 历史
    undo,
    redo,
  };
}

export type CanvasStore = ReturnType<typeof createCanvasStore>;

let singleton: CanvasStore | null = null;

/** 画布 store 单例（面板与对外暴露方法共享同一份状态）。 */
export function useCanvasStore(): CanvasStore {
  if (!singleton) singleton = createCanvasStore();
  return singleton;
}
