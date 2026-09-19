/**
 * canvasCommands：画布命令模式 Undo/Redo（参考 drawdb）。
 *
 * 每条操作封装为可反转命令（undo/redo + message），由 CanvasHistory 统一压栈；
 * 批量操作（多选拖拽、自动整理、删除选中）打包为 MacroCommand。
 *
 * id 重映射：后端 addNode/addEdge 生成 id，撤销「删除」重建实体时会拿到新 id。
 * 命令上下文维护 remap 链（旧 id → 新 id），命令对旧引用一律经 resolveRemappedId
 * 解析后再操作（旧快照模式 idMap 的泛化，跨撤销/重做周期累积）。
 */
import { shallowRef } from "vue";
import type { Ref } from "vue";
import type { MemoryEdge, MemoryNode } from "../../../bridge/memoryCanvas";

/** 连续同类操作在该时间窗内合并进同一条历史记录（与 500ms 持久化防抖同节奏）。 */
export const COMMAND_COALESCE_MS = 500;

/** 历史栈上限（与旧快照模式一致）。 */
export const MAX_HISTORY_ENTRIES = 50;

/** 命令重建实体时提交给 IPC 的草稿（id 由后端生成，故不含）。 */
export interface NodeDraft {
  canvasId: string;
  nodeType: string;
  positionX: number;
  positionY: number;
  payloadJson: string;
  summary?: string | null;
  assetId?: string | null;
}

export interface EdgeDraft {
  canvasId: string;
  sourceNodeId: string;
  targetNodeId: string;
  edgeType: string;
  label?: string | null;
}

/** 命令对状态与 IPC 的访问面（由 store 提供，测试可整体伪造）。 */
export interface CanvasCommandContext {
  nodes: Ref<MemoryNode[]>;
  edges: Ref<MemoryEdge[]>;
  selectedIds: Ref<Set<string>>;
  remap: Map<string, string>;
  addNodeIpc(draft: NodeDraft): Promise<MemoryNode>;
  deleteNodeIpc(id: string): Promise<void>;
  addEdgeIpc(draft: EdgeDraft): Promise<MemoryEdge>;
  deleteEdgeIpc(id: string): Promise<void>;
  /** 节点内容/位置变化后的写库调度（store 侧实现 500ms 防抖合并）。 */
  scheduleNodeWrite(id: string): void;
}

export interface CanvasCommand {
  /** 撤销/重做菜单展示用的操作描述。 */
  readonly message: string;
  /** 相同 key 的连续命令在时间窗内合并进栈顶（可选）。 */
  readonly coalesceKey?: string;
  undo(): Promise<void>;
  redo(): Promise<void>;
  /** 合并后续同 key 命令的终态（如移动命令吸收新的目标位置）。 */
  absorb?(later: CanvasCommand): void;
}

/** 解析 id 重映射链（实体可能被多次删除重建）。 */
export function resolveRemappedId(remap: Map<string, string>, id: string): string {
  let current = id;
  for (;;) {
    const next = remap.get(current);
    if (next === undefined || next === current) return current;
    current = next;
  }
}

async function safeIpc<T>(step: string, run: () => Promise<T>): Promise<T | null> {
  try {
    return await run();
  } catch (error) {
    console.warn(`[CanvasCommands] ${step} failed:`, error);
    return null;
  }
}

function upsertById<T extends { id: string }>(list: T[], item: T): T[] {
  return list.some((x) => x.id === item.id)
    ? list.map((x) => (x.id === item.id ? item : x))
    : [...list, item];
}

function removeFromList<T extends { id: string }>(list: T[], id: string): T[] {
  return list.filter((x) => x.id !== id);
}

/** 新增节点：redo 首次执行创建并记录服务端 id；undo 删除；再 redo 重建并重映射。 */
export class AddNodeCommand implements CanvasCommand {
  readonly message: string;
  private current: MemoryNode | null = null;

  constructor(
    private ctx: CanvasCommandContext,
    private draft: NodeDraft,
    message = "添加节点",
  ) {
    this.message = message;
  }

  get createdNode(): MemoryNode | null {
    return this.current;
  }

  async redo(): Promise<void> {
    const base = this.current ?? this.draft;
    const created = await safeIpc("add node", () => this.ctx.addNodeIpc(base));
    if (!created) return;
    if (this.current && this.current.id !== created.id) {
      this.ctx.remap.set(this.current.id, created.id);
    }
    this.current = created;
    this.ctx.nodes.value = upsertById(this.ctx.nodes.value, created);
  }

  async undo(): Promise<void> {
    if (!this.current) return;
    const id = resolveRemappedId(this.ctx.remap, this.current.id);
    this.ctx.nodes.value = removeFromList(this.ctx.nodes.value, id);
    this.ctx.selectedIds.value.delete(id);
    await safeIpc("delete node (undo add)", () => this.ctx.deleteNodeIpc(id));
  }
}

/** 新增连线：redo 前先经 remap 解析两端节点 id。 */
export class AddEdgeCommand implements CanvasCommand {
  readonly message: string;
  private current: MemoryEdge | null = null;

  constructor(
    private ctx: CanvasCommandContext,
    private draft: EdgeDraft,
    message = "添加连线",
  ) {
    this.message = message;
  }

  get createdEdge(): MemoryEdge | null {
    return this.current;
  }

  async redo(): Promise<void> {
    const base = this.current ?? this.draft;
    const resolved: EdgeDraft = {
      ...base,
      sourceNodeId: resolveRemappedId(this.ctx.remap, base.sourceNodeId),
      targetNodeId: resolveRemappedId(this.ctx.remap, base.targetNodeId),
    };
    const created = await safeIpc("add edge", () => this.ctx.addEdgeIpc(resolved));
    if (!created) return;
    if (this.current && this.current.id !== created.id) {
      this.ctx.remap.set(this.current.id, created.id);
    }
    this.current = created;
    this.ctx.edges.value = upsertById(this.ctx.edges.value, created);
  }

  async undo(): Promise<void> {
    if (!this.current) return;
    const id = resolveRemappedId(this.ctx.remap, this.current.id);
    this.ctx.edges.value = removeFromList(this.ctx.edges.value, id);
    await safeIpc("delete edge (undo add)", () => this.ctx.deleteEdgeIpc(id));
  }
}

/** 删除连线（不随节点级联的场景）。 */
export class RemoveEdgeCommand implements CanvasCommand {
  readonly message: string;
  private edge: MemoryEdge;

  constructor(
    private ctx: CanvasCommandContext,
    edge: MemoryEdge,
    message = "删除连线",
  ) {
    this.message = message;
    this.edge = edge;
  }

  async redo(): Promise<void> {
    const id = resolveRemappedId(this.ctx.remap, this.edge.id);
    this.ctx.edges.value = removeFromList(this.ctx.edges.value, id);
    await safeIpc("delete edge", () => this.ctx.deleteEdgeIpc(id));
  }

  async undo(): Promise<void> {
    const draft: EdgeDraft = {
      ...this.edge,
      sourceNodeId: resolveRemappedId(this.ctx.remap, this.edge.sourceNodeId),
      targetNodeId: resolveRemappedId(this.ctx.remap, this.edge.targetNodeId),
    };
    const created = await safeIpc("re-add edge (undo remove)", () => this.ctx.addEdgeIpc(draft));
    if (!created) return;
    if (created.id !== this.edge.id) this.ctx.remap.set(this.edge.id, created.id);
    this.edge = created;
    this.ctx.edges.value = upsertById(this.ctx.edges.value, created);
  }
}

/** 删除节点：级联删除其连线，撤销时整体重建（节点 id 经 remap 回填到连线端点）。 */
export class RemoveNodeCommand implements CanvasCommand {
  readonly message: string;
  private node: MemoryNode;
  private connected: MemoryEdge[];

  constructor(
    private ctx: CanvasCommandContext,
    node: MemoryNode,
    connectedEdges: MemoryEdge[],
    message = "删除节点",
  ) {
    this.message = message;
    this.node = node;
    this.connected = connectedEdges;
  }

  async redo(): Promise<void> {
    const nodeId = resolveRemappedId(this.ctx.remap, this.node.id);
    const edgeIds = this.connected.map((e) => resolveRemappedId(this.ctx.remap, e.id));
    this.ctx.edges.value = this.ctx.edges.value.filter((e) => !edgeIds.includes(e.id));
    for (const edgeId of edgeIds) {
      await safeIpc("delete edge (with node)", () => this.ctx.deleteEdgeIpc(edgeId));
    }
    this.ctx.nodes.value = removeFromList(this.ctx.nodes.value, nodeId);
    this.ctx.selectedIds.value.delete(nodeId);
    await safeIpc("delete node", () => this.ctx.deleteNodeIpc(nodeId));
  }

  async undo(): Promise<void> {
    const createdNode = await safeIpc("re-add node (undo remove)", () =>
      this.ctx.addNodeIpc(this.node),
    );
    if (!createdNode) return;
    if (createdNode.id !== this.node.id) this.ctx.remap.set(this.node.id, createdNode.id);
    this.node = createdNode;
    this.ctx.nodes.value = upsertById(this.ctx.nodes.value, createdNode);

    const restored: MemoryEdge[] = [];
    for (const edge of this.connected) {
      const draft: EdgeDraft = {
        ...edge,
        sourceNodeId: resolveRemappedId(this.ctx.remap, edge.sourceNodeId),
        targetNodeId: resolveRemappedId(this.ctx.remap, edge.targetNodeId),
      };
      const createdEdge = await safeIpc("re-add edge (undo remove node)", () =>
        this.ctx.addEdgeIpc(draft),
      );
      if (!createdEdge) continue;
      if (createdEdge.id !== edge.id) this.ctx.remap.set(edge.id, createdEdge.id);
      restored.push(createdEdge);
    }
    this.connected = restored;
    this.ctx.edges.value = restored.reduce(upsertById, this.ctx.edges.value);
  }
}

/** 移动节点：连续同节点拖拽在时间窗内合并为一条历史记录。 */
export class MoveNodeCommand implements CanvasCommand {
  readonly message: string;
  readonly coalesceKey: string;
  private to: { x: number; y: number };

  constructor(
    private ctx: CanvasCommandContext,
    private nodeId: string,
    private from: { x: number; y: number },
    to: { x: number; y: number },
    message = "移动节点",
  ) {
    this.message = message;
    this.coalesceKey = `move:${nodeId}`;
    this.to = to;
  }

  private apply(position: { x: number; y: number }): void {
    const id = resolveRemappedId(this.ctx.remap, this.nodeId);
    const node = this.ctx.nodes.value.find((n) => n.id === id);
    if (!node) return;
    if (node.positionX === position.x && node.positionY === position.y) return;
    const next = { ...node, positionX: position.x, positionY: position.y };
    this.ctx.nodes.value = upsertById(this.ctx.nodes.value, next);
    this.ctx.scheduleNodeWrite(next.id);
  }

  redo(): Promise<void> {
    this.apply(this.to);
    return Promise.resolve();
  }

  undo(): Promise<void> {
    this.apply(this.from);
    return Promise.resolve();
  }

  absorb(later: CanvasCommand): void {
    if (later instanceof MoveNodeCommand) this.to = { ...later.to };
  }
}

/** 更新节点 payload/摘要：编辑便签、换色、缩放、图片地址等。 */
export class UpdatePayloadCommand implements CanvasCommand {
  readonly message: string;

  constructor(
    private ctx: CanvasCommandContext,
    private nodeId: string,
    private before: { payloadJson: string; summary: string | null },
    private after: { payloadJson: string; summary: string | null },
    message: string,
  ) {
    this.message = message;
  }

  private apply(state: { payloadJson: string; summary: string | null }): void {
    const id = resolveRemappedId(this.ctx.remap, this.nodeId);
    const node = this.ctx.nodes.value.find((n) => n.id === id);
    if (!node) return;
    const next = { ...node, payloadJson: state.payloadJson, summary: state.summary };
    this.ctx.nodes.value = upsertById(this.ctx.nodes.value, next);
    this.ctx.scheduleNodeWrite(next.id);
  }

  redo(): Promise<void> {
    this.apply(this.after);
    return Promise.resolve();
  }

  undo(): Promise<void> {
    this.apply(this.before);
    return Promise.resolve();
  }
}

/** 批量操作打包：undo 逆序、redo 正序。 */
export class MacroCommand implements CanvasCommand {
  readonly message: string;

  constructor(
    private commands: CanvasCommand[],
    message: string,
  ) {
    this.message = message;
  }

  async undo(): Promise<void> {
    for (const command of [...this.commands].reverse()) await command.undo();
  }

  async redo(): Promise<void> {
    for (const command of this.commands) await command.redo();
  }
}

/** 命令历史栈：push 执行命令（含合并），undo/redo 移动栈指针。 */
export class CanvasHistory {
  // 命令对象持有 ctx（内含 ref），不能被深度响应式代理：shallowRef 只追踪栈整体替换
  readonly undoStack = shallowRef<CanvasCommand[]>([]);
  readonly redoStack = shallowRef<CanvasCommand[]>([]);
  private lastPushAt = 0;

  get canUndo(): boolean {
    return this.undoStack.value.length > 0;
  }

  get canRedo(): boolean {
    return this.redoStack.value.length > 0;
  }

  /** 栈顶命令描述（可用于撤销菜单）。 */
  get lastMessage(): string | null {
    return this.undoStack.value[this.undoStack.value.length - 1]?.message ?? null;
  }

  async push(command: CanvasCommand): Promise<void> {
    const now = Date.now();
    const top = this.undoStack.value[this.undoStack.value.length - 1];
    const coalesce =
      command.coalesceKey !== undefined &&
      top !== undefined &&
      top.coalesceKey === command.coalesceKey &&
      now - this.lastPushAt <= COMMAND_COALESCE_MS;
    await command.redo();
    if (coalesce && top?.absorb) {
      top.absorb(command);
    } else {
      this.undoStack.value = [...this.undoStack.value.slice(-(MAX_HISTORY_ENTRIES - 1)), command];
    }
    this.lastPushAt = now;
    this.redoStack.value = [];
  }

  async undo(): Promise<void> {
    const command = this.undoStack.value[this.undoStack.value.length - 1];
    if (!command) return;
    this.undoStack.value = this.undoStack.value.slice(0, -1);
    await command.undo();
    this.redoStack.value = [...this.redoStack.value, command];
  }

  async redo(): Promise<void> {
    const command = this.redoStack.value[this.redoStack.value.length - 1];
    if (!command) return;
    this.redoStack.value = this.redoStack.value.slice(0, -1);
    await command.redo();
    this.undoStack.value = [...this.undoStack.value, command];
  }

  clear(): void {
    this.undoStack.value = [];
    this.redoStack.value = [];
    this.lastPushAt = 0;
  }
}
