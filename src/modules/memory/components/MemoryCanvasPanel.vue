<script setup lang="ts">
/**
 * MemoryCanvasPanel：Agent 工作记忆无限画布。
 *
 * 基于 vue-flow 实现真无限画布：
 * - 节点按 (x,y) 坐标空间定位，支持拖拽移动
 * - 缩放平移（鼠标滚轮 + 拖拽空白区域）
 * - 边连线表示节点关系（支持拖拽端点重连）
 * - 自定义节点卡片展示类型/摘要/状态/缩略图
 * - 拖拽节点后自动持久化位置，画布视口自动恢复
 * - 右键菜单（节点与空白处）、Ctrl+V 粘贴图片、沉浸模式
 */
import { onMounted, onUnmounted, ref, watch, computed, nextTick } from "vue";
import { VueFlow, useVueFlow } from "@vue-flow/core";
import type { EdgeChange, NodeChange } from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import { MiniMap } from "@vue-flow/minimap";
import {
  AlignStartVertical,
  Redo2,
  Undo2,
  X,
  LoaderCircle,
  ImageIcon,
  StickyNote,
  Maximize,
  Maximize2,
  Trash2,
  Rows3,
  CircleHelp,
  Download,
} from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";

import { useToast } from "../../../shared/ui/useToast";
import type { MemoryNode, MemoryEdge } from "../../../bridge/memoryCanvas";
import {
  addNode,
  addEdge,
  updateNode,
  createCanvas,
  listCanvases,
  listEdges,
  listNodes,
  getViewport,
  saveViewport,
  deleteNode as deleteNodeRpc,
  deleteEdge as deleteEdgeRpc,
} from "../../../bridge/memoryCanvas";

/** 右键菜单动作触发的节点级回调（经 flowNodes.data 注入卡片）。 */
type NodeMenuAction = "clone" | "preview" | "download" | "color";

const NOTE_COLORS = ["#fbbf24", "#60a5fa", "#34d399", "#f472b6"];

import CanvasNodeCard from "./CanvasNodeCard.vue";

// ─── Props / Emits ───

const props = defineProps<{
  conversationId: string | null;
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const toast = useToast();

// ─── State ───

const nodes = ref<MemoryNode[]>([]);
const edges = ref<MemoryEdge[]>([]);
const canvasId = ref<string | null>(null);
const loading = ref(false);
const selectedIds = ref<Set<string>>(new Set());
const selectedCount = computed(() => selectedIds.value.size);
const compactMode = ref(false);
const helpOpen = ref(false);
const immersive = ref(false);
const canvasMenu = ref<{ x: number; y: number; flowX: number; flowY: number } | null>(null);
const lightboxUrl = ref<string | null>(null);

// ─── Vue Flow setup ───

const {
  fitView,
  setViewport,
  screenToFlowPosition,
  onNodeDragStop,
  onConnect,
  onEdgeDoubleClick,
  onNodesChange,
  onEdgesChange,
  onMoveEnd,
} = useVueFlow({
  id: "memory-canvas",
  defaultEdgeOptions: {
    type: "smoothstep",
    animated: true,
    style: { stroke: "#6366f1", strokeWidth: 2 },
  },
  fitViewOnInit: true,
  snapToGrid: true,
  snapGrid: [20, 20] as [number, number],
});

// ── 历史栈（Undo / Redo） ───

interface CanvasSnapshot {
  nodes: MemoryNode[];
  edges: MemoryEdge[];
}

const undoStack = ref<CanvasSnapshot[]>([]);
const redoStack = ref<CanvasSnapshot[]>([]);
const canUndo = computed(() => undoStack.value.length > 0);
const canRedo = computed(() => redoStack.value.length > 0);

function snapshot(): CanvasSnapshot {
  return {
    nodes: nodes.value.map((n) => ({ ...n })),
    edges: edges.value.map((e) => ({ ...e })),
  };
}

function pushUndo(): void {
  undoStack.value = [...undoStack.value.slice(-49), snapshot()];
  redoStack.value = [];
}

async function restoreSnapshot(snap: CanvasSnapshot): Promise<void> {
  if (!canvasId.value) return;
  const targetNodes = new Map(snap.nodes.map((n) => [n.id, n]));
  const targetEdges = new Map(snap.edges.map((e) => [e.id, e]));

  for (const e of [...edges.value]) {
    if (!targetEdges.has(e.id)) {
      edges.value = edges.value.filter((x) => x.id !== e.id);
      try {
        await deleteEdgeRpc(e.id);
      } catch (err) {
        console.warn("[MemoryCanvas] undo delete edge:", err);
      }
    }
  }
  for (const n of [...nodes.value]) {
    if (!targetNodes.has(n.id)) {
      edges.value = edges.value.filter((e) => e.sourceNodeId !== n.id && e.targetNodeId !== n.id);
      nodes.value = nodes.value.filter((x) => x.id !== n.id);
      try {
        await deleteNodeRpc(n.id);
      } catch (err) {
        console.warn("[MemoryCanvas] undo delete node:", err);
      }
    }
  }
  for (const current of [...nodes.value]) {
    const t = targetNodes.get(current.id);
    if (!t) continue;
    const changed =
      t.nodeType !== current.nodeType ||
      t.positionX !== current.positionX ||
      t.positionY !== current.positionY ||
      t.payloadJson !== current.payloadJson ||
      (t.summary ?? undefined) !== (current.summary ?? undefined);
    if (!changed) continue;
    nodes.value = nodes.value.map((x) => (x.id === current.id ? { ...t } : x));
    try {
      await updateNode(
        current.id,
        current.canvasId,
        t.nodeType,
        t.positionX,
        t.positionY,
        t.payloadJson,
        t.summary ?? undefined,
        t.assetId ?? undefined,
      );
    } catch (err) {
      console.warn("[MemoryCanvas] undo update node:", err);
    }
  }
  const idMap = new Map<string, string>();
  for (const t of snap.nodes) {
    if (nodes.value.some((x) => x.id === t.id)) continue;
    try {
      const created = await addNode(
        t.canvasId,
        t.nodeType,
        t.positionX,
        t.positionY,
        t.payloadJson,
        t.summary ?? undefined,
        t.assetId ?? undefined,
      );
      if (created) {
        idMap.set(t.id, created.id);
        nodes.value = [...nodes.value, created];
      }
    } catch (err) {
      console.warn("[MemoryCanvas] undo add node:", err);
    }
  }
  for (const e of snap.edges) {
    if (edges.value.some((x) => x.id === e.id)) continue;
    const src = idMap.get(e.sourceNodeId) ?? e.sourceNodeId;
    const tgt = idMap.get(e.targetNodeId) ?? e.targetNodeId;
    try {
      const created = await addEdge(e.canvasId, src, tgt, e.edgeType, e.label ?? undefined);
      if (created) {
        idMap.set(e.id, created.id);
        edges.value = [...edges.value, created];
      }
    } catch (err) {
      console.warn("[MemoryCanvas] undo add edge:", err);
    }
  }
}

function undo(): void {
  const prev = undoStack.value[undoStack.value.length - 1];
  if (!prev) return;
  undoStack.value = undoStack.value.slice(0, -1);
  redoStack.value = [...redoStack.value, snapshot()];
  void restoreSnapshot(prev);
}

function redo(): void {
  const next = redoStack.value[redoStack.value.length - 1];
  if (!next) return;
  redoStack.value = redoStack.value.slice(0, -1);
  undoStack.value = [...undoStack.value, snapshot()];
  void restoreSnapshot(next);
}

function onCanvasKeydown(event: KeyboardEvent): void {
  if (!props.visible) return;
  if (!(event.ctrlKey || event.metaKey)) return;
  const key = event.key.toLowerCase();
  if (key === "z") {
    event.preventDefault();
    if (event.shiftKey) redo();
    else undo();
  }
}

onMounted(() => {
  window.addEventListener("keydown", onCanvasKeydown);
  window.addEventListener("paste", onCanvasPaste);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onCanvasKeydown);
  window.removeEventListener("paste", onCanvasPaste);
});

// ── 选中与删除 ───

function applyNodeChanges(changes: NodeChange[]): void {
  for (const change of changes) {
    if (change.type === "select") {
      const next = new Set(selectedIds.value);
      if (change.selected) next.add(change.id);
      else next.delete(change.id);
      selectedIds.value = next;
    } else if (change.type === "remove") {
      void removeNodeById(change.id);
    }
  }
}

function applyEdgeChanges(changes: EdgeChange[]): void {
  for (const change of changes) {
    if (change.type === "select") {
      const next = new Set(selectedIds.value);
      if (change.selected) next.add(change.id);
      else next.delete(change.id);
      selectedIds.value = next;
    } else if (change.type === "remove") {
      void removeEdgeById(change.id);
    }
  }
}

async function removeNodeById(id: string): Promise<void> {
  pushUndo();
  const touched = edges.value.filter((e) => e.sourceNodeId === id || e.targetNodeId === id);
  for (const edge of touched) {
    try {
      await deleteEdgeRpc(edge.id);
    } catch (e) {
      console.warn("[MemoryCanvas] delete edge failed:", e);
    }
  }
  edges.value = edges.value.filter((e) => e.sourceNodeId !== id && e.targetNodeId !== id);
  selectedIds.value.delete(id);
  try {
    await deleteNodeRpc(id);
  } catch (e) {
    console.warn("[MemoryCanvas] delete node failed:", e);
    toast.error("节点删除失败，请重试。");
    return;
  }
  nodes.value = nodes.value.filter((n) => n.id !== id);
}

async function removeEdgeById(id: string): Promise<void> {
  pushUndo();
  edges.value = edges.value.filter((e) => e.id !== id);
  try {
    await deleteEdgeRpc(id);
  } catch (e) {
    console.warn("[MemoryCanvas] delete edge failed:", e);
  }
}

// ── 手动连线：拖拽节点边缘即建立 reference 关系
onConnect(async ({ source, target }) => {
  if (!canvasId.value || source === target) return;
  await addEdgeBetweenNodes(source, target, "reference");
});

// 双击连线删除
onEdgeDoubleClick(async ({ edge }) => {
  await removeEdgeById(edge.id);
});

// Delete/Backspace 删除选中（vue-flow 发出 remove 变更）
onNodesChange(applyNodeChanges);
onEdgesChange(applyEdgeChanges);

// 视口变化持久化
onMoveEnd(async ({ flow: flowInstance }) => {
  if (!canvasId.value) return;
  try {
    await saveViewport(
      canvasId.value,
      flowInstance.viewport.zoom,
      flowInstance.viewport.x,
      flowInstance.viewport.y,
    );
  } catch (e) {
    console.warn("[MemoryCanvas] save viewport failed:", e);
  }
});

// ── 工具栏动作 ───

async function addNoteNodeAt(flowX: number, flowY: number): Promise<void> {
  if (!canvasId.value) return;
  pushUndo();
  const text = "双击编辑便签";
  const node = await addNode(
    canvasId.value,
    "note",
    flowX,
    flowY,
    JSON.stringify({ text, source: "manual" }),
    text,
  );
  if (node) nodes.value = [...nodes.value, node];
}

async function addNoteNode(): Promise<void> {
  if (!canvasId.value) return;
  pushUndo();
  const last = nodes.value[nodes.value.length - 1];
  const x = last ? last.positionX + 40 : 80;
  const y = last ? last.positionY + 260 : 80;
  await addNoteNodeAt(x, y);
}

async function pickAndAddImages(): Promise<void> {
  if (!canvasId.value) return;
  const selected = (await invoke("plugin:dialog|open", {
    multiple: true,
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  })) as string[] | null;
  if (!selected) return;
  for (const path of selected) {
    try {
      const dataUrl = await invoke<string>("file_read_as_data_url", { path });
      const name = path.split(/[\\/]/).pop() || "图片";
      await addUploadNode("upload", name, dataUrl);
    } catch (e) {
      console.warn("[MemoryCanvas] add image failed:", e);
      toast.error("图片添加失败，请重试。");
    }
  }
}

// ── 画布空白处右键菜单：在光标处新建便签/导入图片 ──

async function onCanvasContextMenu(event: MouseEvent): Promise<void> {
  const target = event.target as HTMLElement;
  if (target.closest(".canvas-node") || target.closest(".canvas-toolbar")) return;
  event.preventDefault();
  const flow = screenToFlowPosition({ x: event.clientX, y: event.clientY });
  canvasMenu.value = { x: event.clientX, y: event.clientY, flowX: flow.x, flowY: flow.y };
}

async function onCanvasMenuAction(action: "note" | "image"): Promise<void> {
  const menu = canvasMenu.value;
  canvasMenu.value = null;
  if (!menu) return;
  if (action === "note") await addNoteNodeAt(menu.flowX, menu.flowY);
  else await pickAndAddImages();
}

// ── 剪贴板粘贴图片（Ctrl+V）──

async function onCanvasPaste(event: ClipboardEvent): Promise<void> {
  if (!props.visible || !canvasId.value) return;
  const files = [...(event.clipboardData?.files || [])].filter((f) => f.type.startsWith("image/"));
  if (files.length === 0) return;
  event.preventDefault();
  for (const file of files) {
    const dataUrl = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.readAsDataURL(file);
    });
    await addUploadNode("upload", file.name || "粘贴的图片", dataUrl);
  }
  toast.success(`已粘贴 ${files.length} 张图片到画布。`);
}

function fitCanvas(): void {
  void fitView({ padding: 0.2 });
}

// ── 自动整理：按类型分组排成竖列 ───

async function autoArrange(): Promise<void> {
  if (!canvasId.value) return;
  pushUndo();
  const order = ["upload", "image", "note", "video", "document", "fact"];
  const groups = new Map<string, MemoryNode[]>();
  for (const n of nodes.value) {
    const list = groups.get(n.nodeType) || [];
    list.push(n);
    groups.set(n.nodeType, list);
  }
  const sortedTypes = [...groups.keys()].sort((a, b) => order.indexOf(a) - order.indexOf(b));
  let column = 0;
  const updates: Promise<unknown>[] = [];
  for (const nodeType of sortedTypes) {
    let row = 0;
    for (const n of groups.get(nodeType) || []) {
      const x = 60 + column * 280;
      const y = 60 + row * 260;
      if (n.positionX !== x || n.positionY !== y) {
        nodes.value = nodes.value.map((old) =>
          old.id === n.id ? { ...old, positionX: x, positionY: y } : old,
        );
        updates.push(
          updateNode(
            n.id,
            n.canvasId,
            n.nodeType,
            x,
            y,
            n.payloadJson,
            n.summary ?? undefined,
            n.assetId ?? undefined,
          ).catch((e) => console.warn("[MemoryCanvas] arrange persist failed:", e)),
        );
      }
      row += 1;
    }
    column += 1;
  }
  await Promise.all(updates);
  setTimeout(() => fitView({ padding: 0.2 }), 80);
}

async function deleteSelected(): Promise<void> {
  for (const id of [...selectedIds.value]) {
    if (nodes.value.some((n) => n.id === id)) await removeNodeById(id);
  }
  for (const id of [...selectedIds.value]) {
    if (edges.value.some((e) => e.id === id)) await removeEdgeById(id);
  }
  selectedIds.value = new Set();
}

// ── 便签编辑持久化 ───

async function saveNoteSummary(id: string, text: string): Promise<void> {
  const node = nodes.value.find((n) => n.id === id);
  if (!node) return;
  pushUndo();
  let payload = node.payloadJson;
  try {
    const parsed = JSON.parse(node.payloadJson);
    parsed.text = text;
    payload = JSON.stringify(parsed);
  } catch {
    payload = JSON.stringify({ text, source: "manual" });
  }
  nodes.value = nodes.value.map((n) =>
    n.id === id ? { ...n, summary: text, payloadJson: payload } : n,
  );
  try {
    await updateNode(
      id,
      node.canvasId,
      node.nodeType,
      node.positionX,
      node.positionY,
      payload,
      text,
      node.assetId ?? undefined,
    );
  } catch (e) {
    console.warn("[MemoryCanvas] save note failed:", e);
  }
}

// ── 右键菜单动作 ───

function nodeDataUrl(id: string): string | null {
  const node = nodes.value.find((n) => n.id === id);
  if (!node) return null;
  try {
    const p = JSON.parse(node.payloadJson);
    return p.dataUrl || p.imageUrl || null;
  } catch {
    return null;
  }
}

function cycleNoteColor(id: string): void {
  const node = nodes.value.find((n) => n.id === id);
  if (!node) return;
  pushUndo();
  let payload: Record<string, unknown> = {};
  try {
    payload = JSON.parse(node.payloadJson);
  } catch {
    payload = {};
  }
  const current = (payload.color as string) || NOTE_COLORS[0];
  const next = NOTE_COLORS[(NOTE_COLORS.indexOf(current) + 1) % NOTE_COLORS.length];
  payload.color = next;
  const payloadJson = JSON.stringify(payload);
  nodes.value = nodes.value.map((n) => (n.id === id ? { ...n, payloadJson } : n));
  void updateNode(
    id,
    node.canvasId,
    node.nodeType,
    node.positionX,
    node.positionY,
    payloadJson,
    node.summary ?? undefined,
    node.assetId ?? undefined,
  ).catch((e) => console.warn("[MemoryCanvas] save color failed:", e));
}

function downloadNodeImage(id: string): void {
  const url = nodeDataUrl(id);
  if (!url) return;
  const node = nodes.value.find((n) => n.id === id);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${node?.summary || "canvas-image"}.png`;
  anchor.click();
}

function cloneNode(id: string): void {
  const node = nodes.value.find((n) => n.id === id);
  if (!node || !canvasId.value) return;
  pushUndo();
  const x = node.positionX + 40;
  const y = node.positionY + 40;
  addNode(
    canvasId.value,
    node.nodeType,
    x,
    y,
    node.payloadJson,
    (node.summary || "副本") + " 副本",
    node.assetId ?? undefined,
  ).then((created) => {
    if (created) nodes.value = [...nodes.value, created];
  });
}

async function handleNodeMenuAction(id: string, action: NodeMenuAction): Promise<void> {
  if (action === "delete") {
    await removeNodeById(id);
    return;
  }
  if (action === "clone") {
    cloneNode(id);
    return;
  }
  if (action === "preview") {
    const url = nodeDataUrl(id);
    if (url) lightboxUrl.value = url;
    return;
  }
  if (action === "download") {
    downloadNodeImage(id);
    return;
  }
  if (action === "color") {
    cycleNoteColor(id);
  }
}

function saveNoteLocal(id: string, text: string): void {
  void saveNoteSummary(id, text);
}

function removeNodeCb(id: string): void {
  void removeNodeById(id);
}

// Persist node position on drag end
onNodeDragStop(async ({ nodes: draggedNodes }) => {
  if (!canvasId.value) return;
  for (const fn of draggedNodes) {
    const memNode = nodes.value.find((n) => n.id === fn.id);
    if (!memNode) continue;
    const newX = Math.round(fn.position.x);
    const newY = Math.round(fn.position.y);
    if (newX === memNode.positionX && newY === memNode.positionY) continue;
    pushUndo();
    nodes.value = nodes.value.map((n) =>
      n.id === fn.id ? { ...n, positionX: newX, positionY: newY } : n,
    );
    try {
      await updateNode(
        fn.id,
        memNode.canvasId,
        memNode.nodeType,
        newX,
        newY,
        memNode.payloadJson,
        memNode.summary ?? undefined,
        memNode.assetId ?? undefined,
      );
    } catch (e) {
      console.warn("[MemoryCanvas] failed to persist position:", e);
      toast.warning("节点位置保存失败，重开画布后会恢复。");
    }
  }
});

// ─── Vue Flow nodes/edges (reactive transform) ───

const flowNodes = computed(() =>
  nodes.value.map((n) => ({
    id: n.id,
    type: "canvasCard" as const,
    position: { x: n.positionX, y: n.positionY },
    data: {
      nodeType: n.nodeType,
      summary: n.summary,
      payloadJson: n.payloadJson,
      assetId: n.assetId,
      compact: compactMode.value,
      onDelete: () => removeNodeCb(n.id),
      onSaveSummary: (text: string) => saveNoteLocal(n.id, text),
      onMenuAction: (action: NodeMenuAction) => handleNodeMenuAction(n.id, action),
    },
    style: {
      width: n.width ? `${n.width}px` : "220px",
    },
  })),
);

const flowEdges = computed(() =>
  edges.value.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    target: e.targetNodeId,
    label: (e.label || undefined) as string | undefined,
    type: "smoothstep" as const,
    animated: e.edgeType === "reference",
    style: {
      stroke: getEdgeColor(e.edgeType),
      strokeWidth: 2,
    },
    markerEnd: { type: "arrowclosed" as const, color: getEdgeColor(e.edgeType) },
  })),
);

function getEdgeColor(edgeType: string): string {
  const colors: Record<string, string> = {
    reference: "#6366f1",
    dependency: "#f59e0b",
    link: "#22c55e",
    similarity: "#06b6d4",
  };
  return colors[edgeType] || "#94a3b8";
}

// ─── Canvas lifecycle ───

watch(
  () => props.conversationId,
  async (id) => {
    if (!id) {
      nodes.value = [];
      edges.value = [];
      canvasId.value = null;
      return;
    }
    await loadOrCreateCanvas(id);
  },
  { immediate: true },
);

async function loadOrCreateCanvas(conversationId: string): Promise<void> {
  loading.value = true;
  try {
    const canvases = await listCanvases("default");
    const existing = canvases.find((c) => c.name === `conv-${conversationId}`);
    if (existing) {
      canvasId.value = existing.id;
      nodes.value = await listNodes(existing.id);
      edges.value = await listEdges(existing.id);
      try {
        const vp = await getViewport(existing.id);
        if (vp) {
          await nextTick();
          setTimeout(() => setViewport({ zoom: vp.zoom, x: vp.panX, y: vp.panY }), 120);
        }
      } catch {
        /* no viewport saved yet */
      }
    } else {
      const canvas = await createCanvas("default", `conv-${conversationId}`);
      canvasId.value = canvas.id;
      nodes.value = [];
      edges.value = [];
    }
    await nextTick();
    if (nodes.value.length > 0) {
      setTimeout(() => fitView({ padding: 0.2 }), 100);
    }
  } catch (e) {
    console.error("[MemoryCanvas] loadOrCreateCanvas error:", e);
  } finally {
    loading.value = false;
  }
}

// ─── Exposed methods (same API as before) ───

async function addGenerationNode(
  nodeType: string,
  prompt: string,
  assetId?: string,
  name?: string,
): Promise<string | null> {
  if (!canvasId.value) return null;
  pushUndo();
  const lastNode = nodes.value[nodes.value.length - 1];
  const x = lastNode ? lastNode.positionX + 260 : 40;
  const y = lastNode ? lastNode.positionY + (nodes.value.length % 3 === 0 ? -80 : 80) : 40;
  const summary = name || prompt.slice(0, 50);
  const node = await addNode(
    canvasId.value,
    nodeType,
    x,
    y,
    JSON.stringify({ prompt, status: "pending" }),
    summary,
    assetId,
  );
  if (node) {
    nodes.value = [...nodes.value, node];
    return node.id;
  }
  return null;
}

async function addUploadNode(
  nodeType: string,
  name: string,
  dataUrl: string,
  description?: string,
): Promise<string | null> {
  if (!canvasId.value) return null;
  pushUndo();
  const lastNode = nodes.value[nodes.value.length - 1];
  const x = lastNode ? lastNode.positionX + 260 : 40;
  const y = lastNode ? lastNode.positionY : 40;
  // 必须存完整 data URL：截断会让 base64 解码失败，节点图片永远无法渲染
  const payload: Record<string, string> = { source: "user-upload", dataUrl };
  if (description) payload.description = description;
  const node = await addNode(canvasId.value, nodeType, x, y, JSON.stringify(payload), name);
  if (node) {
    nodes.value = [...nodes.value, node];
    return node.id;
  }
  return null;
}

async function addEdgeBetweenNodes(
  sourceId: string,
  targetId: string,
  edgeType = "reference",
): Promise<void> {
  if (!canvasId.value) return;
  pushUndo();
  const exists = edges.value.some(
    (e) => e.sourceNodeId === sourceId && e.targetNodeId === targetId && e.edgeType === edgeType,
  );
  if (exists) return;
  const edge = await addEdge(canvasId.value, sourceId, targetId, edgeType);
  if (edge) {
    edges.value = [...edges.value, edge];
  }
}

function setNodeStatus(nodeId: string, status: string): void {
  const node = nodes.value.find((n) => n.id === nodeId);
  if (!node) return;
  try {
    const payload = JSON.parse(node.payloadJson);
    payload.status = status;
    const newPayload = JSON.stringify(payload);
    nodes.value = nodes.value.map((n) => (n.id === nodeId ? { ...n, payloadJson: newPayload } : n));
  } catch {
    // ignore
  }
}

function findRecentNodesByType(nodeType: string, limit = 3): MemoryNode[] {
  return nodes.value.filter((n) => n.nodeType === nodeType).slice(-limit);
}

defineExpose({
  addGenerationNode,
  addUploadNode,
  addEdgeBetweenNodes,
  setNodeStatus,
  findRecentNodesByType,
});
</script>

<template>
  <Transition name="memory-slide">
    <aside
      v-if="visible"
      class="memory-canvas-panel"
      :class="{ 'memory-canvas-panel--immersive': immersive }"
      @contextmenu="onCanvasContextMenu"
      @paste="onCanvasPaste"
    >
      <!-- Header -->
      <div class="canvas-header">
        <span class="canvas-header__title">工作记忆画布</span>
        <span class="canvas-header__count">{{ nodes.length }} 节点 · {{ edges.length }} 连线</span>
        <button
          class="icon-btn"
          :title="immersive ? '退出沉浸模式' : '沉浸模式（全窗口画布）'"
          @click="immersive = !immersive"
        >
          <Maximize2 :size="14" />
        </button>
        <button class="icon-btn" @click="emit('close')">
          <X :size="14" />
        </button>
      </div>

      <!-- Loading -->
      <div v-if="loading" class="canvas-loading canvas-loading--overlay">
        <LoaderCircle :size="20" class="is-spinning" />
      </div>

      <!-- Empty state overlay（画布始终渲染，便于手动添加节点） -->
      <div
        v-if="!loading && canvasId && nodes.length === 0"
        class="canvas-empty canvas-empty--overlay"
      >
        <ImageIcon :size="32" />
        <span>画布还是空的：右键空白处新建，或用上方工具栏添加</span>
        <span class="canvas-empty__hint"
          >支持 Ctrl+V 粘贴图片、拖拽图片文件进来；Shift+拖拽框选，Delete 删除选中</span
        >
      </div>

      <!-- Canvas toolbar -->
      <div v-if="!loading && canvasId" class="canvas-toolbar">
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="撤销（Ctrl+Z）"
          :disabled="!canUndo"
          @click="undo"
        >
          <Undo2 :size="15" />
        </button>
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="重做（Ctrl+Shift+Z）"
          :disabled="!canRedo"
          @click="redo"
        >
          <Redo2 :size="15" />
        </button>
        <span class="canvas-toolbar__divider" />
        <button class="canvas-toolbar__btn" type="button" title="添加便签" @click="addNoteNode">
          <StickyNote :size="15" />
          <span>便签</span>
        </button>
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="添加图片文件"
          @click="pickAndAddImages"
        >
          <ImageIcon :size="15" />
          <span>图片</span>
        </button>
        <span class="canvas-toolbar__divider" />
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="自动整理（按类型分组）"
          @click="autoArrange"
        >
          <AlignStartVertical :size="15" />
        </button>
        <button
          class="canvas-toolbar__btn"
          type="button"
          :title="compactMode ? '详细模式' : '紧凑模式'"
          @click="compactMode = !compactMode"
        >
          <Rows3 :size="15" />
        </button>
        <span class="canvas-toolbar__divider" />
        <button class="canvas-toolbar__btn" type="button" title="适应视图" @click="fitCanvas">
          <Maximize :size="15" />
        </button>
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="导出画布 JSON"
          @click="exportCanvasJson"
        >
          <Download :size="15" />
        </button>
        <button
          class="canvas-toolbar__btn"
          type="button"
          title="快捷键说明"
          @click="helpOpen = true"
        >
          <CircleHelp :size="15" />
        </button>
        <span class="canvas-toolbar__divider" />
        <button
          class="canvas-toolbar__btn canvas-toolbar__btn--danger"
          type="button"
          title="删除选中（Delete）"
          :disabled="selectedCount === 0"
          @click="deleteSelected"
        >
          <Trash2 :size="15" />
        </button>
      </div>

      <!-- Vue Flow Canvas -->
      <VueFlow
        class="memory-canvas"
        :nodes="flowNodes"
        :edges="flowEdges as any"
        :default-edge-options="{ type: 'smoothstep', animated: true }"
        :snap-to-grid="true"
        :snap-grid="[20, 20]"
        :min-zoom="0.2"
        :max-zoom="3"
        :default-viewport="{ zoom: 0.8, x: 0, y: 0 }"
        :edges-updatable="true"
        :connection-radius="30"
        :elevate-nodes-on-select="true"
        :only-render-visible-elements="nodes.length > 40"
      >
        <template #node-canvasCard="nodeProps">
          <CanvasNodeCard v-bind="nodeProps" />
        </template>

        <Background pattern-color="rgba(99, 102, 241, 0.08)" :gap="20" :size="1" />
        <Controls position="bottom-right" />

        <!-- 画布空白处右键菜单 -->
        <teleport to="body">
          <div
            v-if="canvasMenu"
            class="canvas-menu-backdrop"
            @click="canvasMenu = null"
            @contextmenu.prevent="canvasMenu = null"
          >
            <div
              class="canvas-menu"
              :style="{ left: `${canvasMenu.x}px`, top: `${canvasMenu.y}px` }"
              @click.stop
            >
              <button class="canvas-menu__item" type="button" @click="onCanvasMenuAction('note')">
                在此处新建便签
              </button>
              <button class="canvas-menu__item" type="button" @click="onCanvasMenuAction('image')">
                在此处导入图片
              </button>
            </div>
          </div>
        </teleport>

        <!-- 快捷键帮助 -->
        <teleport to="body">
          <div v-if="helpOpen" class="canvas-help-backdrop" @click="helpOpen = false">
            <div class="canvas-help" @click.stop>
              <div class="canvas-help__title">画布快捷键与操作</div>
              <div class="canvas-help__row">
                <span>拖拽节点</span><span>调整位置（自动保存）</span>
              </div>
              <div class="canvas-help__row">
                <span>拖拽节点边缘圆点</span><span>连线到目标节点（可拖动端点重连）</span>
              </div>
              <div class="canvas-help__row"><span>双击连线</span><span>删除连线</span></div>
              <div class="canvas-help__row">
                <span>双击便签</span><span>编辑内容（Enter 保存 / Esc 取消）</span>
              </div>
              <div class="canvas-help__row">
                <span>右键节点</span><span>查看大图 / 下载 / 克隆 / 换色 / 删除</span>
              </div>
              <div class="canvas-help__row">
                <span>右键空白处</span><span>在此处新建便签 / 导入图片</span>
              </div>
              <div class="canvas-help__row">
                <span>Ctrl+V</span><span>粘贴剪贴板图片到画布</span>
              </div>
              <div class="canvas-help__row"><span>Shift + 拖拽</span><span>框选多个节点</span></div>
              <div class="canvas-help__row">
                <span>Delete</span><span>删除选中的节点/连线</span>
              </div>
              <div class="canvas-help__row">
                <span>Ctrl+Z / Ctrl+Shift+Z</span><span>撤销 / 重做</span>
              </div>
              <div class="canvas-help__row">
                <span>双击空白处</span><span>在该位置新建便签</span>
              </div>
              <button class="canvas-help__close" type="button" @click="helpOpen = false">
                知道了
              </button>
            </div>
          </div>
        </teleport>

        <!-- 图片大图预览 -->
        <teleport to="body">
          <div v-if="lightboxUrl" class="canvas-lightbox" @click="lightboxUrl = null">
            <img :src="lightboxUrl" alt="预览" />
          </div>
        </teleport>

        <MiniMap
          position="bottom-left"
          :pannable="true"
          :zoomable="true"
          :node-color="(n: any) => getNodeColor(n.data?.nodeType)"
          :mask-color="'rgb(15, 20, 30, 0.7)'"
          class="canvas-minimap"
        />
      </VueFlow>
    </aside>
  </Transition>
</template>

<script lang="ts">
function getNodeColor(type: string | undefined): string {
  if (!type) return "#6b7280";
  const colors: Record<string, string> = {
    fact: "#8b5cf6",
    note: "#3b82f6",
    image: "#22c55e",
    video: "#f59e0b",
    document: "#06b6d4",
    upload: "#ec4899",
  };
  return colors[type] || "#6b7280";
}
</script>

<style>
/* vue-flow styles (must be unscoped) */
@import "@vue-flow/core/dist/style.css";
@import "@vue-flow/core/dist/theme-default.css";
@import "@vue-flow/controls/dist/style.css";
@import "@vue-flow/minimap/dist/style.css";

/* vue-flow 控件/选区/选中连线 暗色适配 */
.memory-canvas-panel .vue-flow__controls {
  border: 1px solid rgb(255 255 255 / 8%);
  background: rgb(16 20 30 / 88%);
  backdrop-filter: blur(8px);
  border-radius: var(--radius-control);
  overflow: hidden;
  box-shadow: 0 4px 12px rgb(0 0 0 / 25%);
}

.memory-canvas-panel .vue-flow__controls-button {
  color: rgb(255 255 255 / 70%);
  border-bottom: 1px solid var(--color-border-subtle);
  background: transparent;
}

.memory-canvas-panel .vue-flow__controls-button:hover {
  background: rgb(255 255 255 / 6%);
  color: #fff;
}

.memory-canvas-panel .vue-flow__selection {
  background: rgb(99 102 241 / 8%);
  border: 1.5px dashed #818cf8;
  border-radius: 4px;
}

.memory-canvas-panel .vue-flow__edge.selected .vue-flow__edge-path {
  stroke: #818cf8;
  stroke-width: 3;
  filter: drop-shadow(0 0 4px rgb(129 140 248 / 50%));
}

.canvas-toolbar {
  position: absolute;
  top: 52px;
  left: var(--space-3);
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  box-shadow: 0 4px 12px rgb(0 0 0 / 25%);
}

.canvas-toolbar__btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 var(--space-2);
  color: var(--color-text-secondary, var(--color-text));
  border: none;
  border-radius: 6px;
  background: transparent;
  font-size: 12px;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.canvas-toolbar__btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.canvas-toolbar__btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.canvas-toolbar__btn--danger:hover:not(:disabled) {
  color: #f87171;
}

.canvas-toolbar__divider {
  width: 1px;
  height: 16px;
  margin: 0 2px;
  background: var(--color-border-subtle);
}

.canvas-toolbar__count {
  padding: 0 var(--space-2);
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.canvas-empty--overlay,
.canvas-loading--overlay {
  position: absolute;
  inset: 0;
  z-index: 5;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  background: rgb(10 14 22 / 45%);
  pointer-events: none;
}

/* 节点右键菜单 / 空白右键菜单（teleport 到 body） */
.canvas-menu-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}

.canvas-menu {
  position: fixed;
  z-index: 101;
  display: flex;
  flex-direction: column;
  min-width: 128px;
  padding: 4px;
  border: 1px solid var(--color-border-subtle);
  border-radius: 8px;
  background: var(--color-surface);
  box-shadow: 0 8px 24px rgb(0 0 0 / 40%);
}

.canvas-menu__item {
  padding: 6px 10px;
  color: var(--color-text);
  border: none;
  border-radius: 6px;
  background: transparent;
  font-size: var(--text-footnote);
  text-align: left;
  cursor: pointer;
}

.canvas-menu__item:hover {
  background: var(--color-surface-hover);
}

.canvas-menu__item--danger {
  color: #f87171;
}

/* 快捷键帮助 */
.canvas-help-backdrop {
  position: fixed;
  inset: 0;
  z-index: 110;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 50%);
}

.canvas-help {
  width: 460px;
  max-width: calc(100vw - 48px);
  padding: var(--space-4) var(--space-5);
  border: 1px solid var(--color-border-subtle);
  border-radius: 12px;
  background: var(--color-surface);
  box-shadow: 0 12px 40px rgb(0 0 0 / 50%);
}

.canvas-help__title {
  margin-bottom: var(--space-3);
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
}

.canvas-help__row {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
  padding: 4px 0;
  color: var(--color-text-secondary, var(--color-text));
  font-size: 12px;
}

.canvas-help__row span:first-child {
  color: var(--color-text);
  font-weight: 500;
}

.canvas-help__close {
  height: 30px;
  padding: 0 var(--space-4);
  margin-top: var(--space-3);
  color: #fff;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  font-size: 12px;
  cursor: pointer;
}

/* 图片大图预览 */
.canvas-lightbox {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 78%);
  cursor: zoom-out;
}

.canvas-lightbox img {
  max-width: 92vw;
  max-height: 92vh;
  border-radius: 8px;
  box-shadow: 0 16px 64px rgb(0 0 0 / 60%);
}

/* MiniMap 暗色适配：默认白底在深色主题下像一块游离的白板 */
.canvas-minimap {
  background: rgb(12 16 24 / 90%);
  border: 1px solid rgb(255 255 255 / 8%);
  box-shadow: 0 4px 16px rgb(0 0 0 / 35%);
  backdrop-filter: blur(8px);
  border-radius: 8px;
  overflow: hidden;
}

.canvas-minimap svg {
  background: transparent;
}
</style>

<style scoped>
.memory-canvas-panel {
  position: absolute;
  inset: 0;
  z-index: 30;
  display: flex;
  flex-direction: column;
  background: var(--color-surface, #0f1420);
  overflow: hidden;
}

.memory-canvas-panel--immersive {
  position: fixed;
  inset: 0;
  z-index: 100;
}

/* Slide transition */
.memory-slide-enter-active,
.memory-slide-leave-active {
  transition:
    opacity 200ms var(--ease-out, ease),
    transform 200ms var(--ease-out, ease);
}
.memory-slide-enter-from,
.memory-slide-leave-to {
  opacity: 0;
  transform: translateY(8px);
}

/* Header */
.canvas-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 10px 16px;
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
  flex-shrink: 0;
}
.canvas-header__title {
  margin-right: auto;
  font-size: 13px;
  font-weight: 700;
  color: rgb(255 255 255 / 90%);
  letter-spacing: 0.01em;
}

/* Loading */
.canvas-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
}

/* Empty */
.canvas-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  flex: 1;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}
.canvas-empty__hint {
  font-size: 11px;
  opacity: 0.6;
}

/* Canvas fills remaining space */
.memory-canvas {
  flex: 1;
  width: 100%;
}

.is-spinning {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
