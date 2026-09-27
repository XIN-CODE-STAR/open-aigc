<script setup lang="ts">
/**
 * MemoryCanvasPanel：Agent 工作记忆无限画布。
 *
 * 基于 vue-flow 实现真无限画布：
 * - 工具模式切换（tldraw 式）：选择 V / 便签 N / 图片 I，Escape 回到选择
 * - 节点按 (x,y) 坐标空间定位，支持拖拽移动，右下角手柄拖拽缩放
 * - 缩放平移（鼠标滚轮 + 拖拽空白区域）
 * - 边连线表示节点关系（支持拖拽端点重连）
 * - 自定义节点卡片展示类型/摘要/状态/缩略图
 * - 拖拽节点后自动持久化位置，节点尺寸持久化在 payload_json
 * - 统一右键菜单（节点与空白处）、Ctrl+C/V 复制粘贴节点、Ctrl+A 全选
 * - Ctrl+V 粘贴图片、沉浸模式
 *
 * 状态/历史/持久化集中在 useCanvasStore（命令模式 Undo/Redo，500ms 防抖写库）；
 * 本组件只做渲染和事件转发，不直接操作画布状态。
 */
import { onMounted, onUnmounted, ref, watch, computed, nextTick } from "vue";
import { VueFlow, useVueFlow } from "@vue-flow/core";
import type { GraphNode } from "@vue-flow/core";
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
  MousePointer2,
  Maximize,
  Maximize2,
  Minimize2,
  Trash2,
  Rows3,
  CircleHelp,
  Download,
  Grid3x3,
  AlertCircle,
  EyeOff,
  Map,
} from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";

import { useToast } from "../../../shared/ui/useToast";
import CanvasNodeCard from "./CanvasNodeCard.vue";
import CanvasContextMenu from "./CanvasContextMenu.vue";
import type { CanvasMenuItem } from "./CanvasContextMenu.vue";
import type { MemoryNode } from "../../../bridge/memoryCanvas";
import { getEffectiveSize } from "../canvasNodeSize";
import { useCanvasStore } from "../composables/useCanvasStore";

// ─── Props / Emits ───

const props = defineProps<{
  conversationId: string | null;
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const toast = useToast();

// ─── Store（集中状态：nodes/edges/viewport/selection/history） ───

const {
  nodes,
  edges,
  canvasId,
  loading,
  loadError,
  selectedIds,
  viewport,
  canUndo,
  canRedo,
  loadCanvas,
  flushPendingWrites,
  handleNodeChanges,
  handleEdgeChanges,
  saveViewport,
  addNoteNodeAt,
  addUploadNode,
  addGenerationNode,
  cloneNodes,
  removeNode,
  removeEdge,
  moveNodes,
  deleteSelected,
  connectNodes,
  reconnectEdge,
  updateNoteText,
  cycleNoteColor,
  resizeNode,
  setNodeStatus,
  setNodeImageUrl,
  findRecentNodesByType,
  undo,
  redo,
  autoArrange: arrangeNodes,
} = useCanvasStore();

const selectedCount = computed(() => selectedIds.value.size);
const compactMode = ref(false);
const helpOpen = ref(false);
const immersive = ref(false);
const lightboxUrl = ref<string | null>(null);

// ── 工具模式（tldraw 式：选择 / 便签 / 图片） ───

type CanvasTool = "select" | "note" | "image";
const canvasTool = ref<CanvasTool>("select");

function setTool(tool: CanvasTool): void {
  canvasTool.value = tool;
}

/** 便签模式下点击空白创建后保持模式；图片模式选完文件后回到选择模式。 */
function onPaneClick(event: MouseEvent): void {
  if (canvasTool.value === "select") return;
  const flow = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
  if (canvasTool.value === "note") {
    void addNoteNodeAt(flow.x, flow.y);
    return;
  }
  canvasTool.value = "select";
  void pickAndAddImages();
}

// ── 画布视觉（Phase 4）：网格样式 / 边描线动画 / hover 高亮两端 ───

const MINIMAP_STORAGE_KEY = "memory-canvas-minimap";
const minimapVisible = ref((localStorage.getItem(MINIMAP_STORAGE_KEY) ?? "1") === "1");

function toggleMinimap(): void {
  minimapVisible.value = !minimapVisible.value;
  localStorage.setItem(MINIMAP_STORAGE_KEY, minimapVisible.value ? "1" : "0");
}

type GridMode = "dots" | "lines" | "none";
const GRID_STORAGE_KEY = "memory-canvas-grid";
const GRID_LABELS: Record<GridMode, string> = { dots: "点阵", lines: "方格", none: "无网格" };
const gridMode = ref<GridMode>(
  (localStorage.getItem(GRID_STORAGE_KEY) as GridMode | null) ?? "dots",
);

function cycleGrid(): void {
  const order: GridMode[] = ["dots", "lines", "none"];
  const next = order[(order.indexOf(gridMode.value) + 1) % order.length];
  gridMode.value = next;
  localStorage.setItem(GRID_STORAGE_KEY, next);
}

/** 新建连线的描线动画：短窗口内标记 class，动画结束后恢复常规样式 */
const recentEdgeIds = ref<Set<string>>(new Set());
const knownEdgeIds = new Set<string>();
let edgesHydrated = false;
watch(edges, (list) => {
  if (!edgesHydrated) {
    // 画布刚加载：存量边不做动画
    for (const edge of list) knownEdgeIds.add(edge.id);
    edgesHydrated = true;
    return;
  }
  for (const edge of list) {
    if (knownEdgeIds.has(edge.id)) continue;
    knownEdgeIds.add(edge.id);
    recentEdgeIds.value = new Set([...recentEdgeIds.value, edge.id]);
    setTimeout(() => {
      const next = new Set(recentEdgeIds.value);
      next.delete(edge.id);
      recentEdgeIds.value = next;
    }, 1200);
  }
});

// ── 统一右键菜单（节点 / 空白共用 CanvasContextMenu） ───

const canvasMenu = ref<{ x: number; y: number; flowX: number; flowY: number } | null>(null);
const nodeMenu = ref<{ id: string; x: number; y: number } | null>(null);

const paneMenuItems = computed<CanvasMenuItem[]>(() => [
  { key: "note", label: "新建便签" },
  { key: "image", label: "导入图片" },
  { key: "selectAll", label: "全选", disabled: nodes.value.length === 0 },
  { key: "paste", label: "粘贴节点", disabled: clipboardNodes.value.length === 0 },
]);

const nodeMenuItems = computed<CanvasMenuItem[]>(() => {
  const target = nodeMenu.value;
  if (!target) return [];
  const node = nodes.value.find((n) => n.id === target.id);
  if (!node) return [];
  const items: CanvasMenuItem[] = [];
  if (node.nodeType === "note") items.push({ key: "edit", label: "编辑" });
  items.push({ key: "copy", label: "复制" });
  if (node.nodeType === "note") items.push({ key: "color", label: "换个颜色" });
  if (node.nodeType === "image" || node.nodeType === "upload") {
    if (nodeDataUrl(node)) {
      items.push({ key: "preview", label: "查看大图" });
      items.push({ key: "download", label: "下载图片" });
    }
  }
  items.push({ key: "delete", label: "删除节点", danger: true });
  return items;
});

// ─── Vue Flow setup ───

const {
  fitView,
  setViewport,
  screenToFlowCoordinate,
  addSelectedElements,
  onNodeDragStop,
  onConnect,
  onEdgeDoubleClick,
  onEdgeMouseEnter,
  onEdgeMouseLeave,
  onEdgeUpdate,
  onConnectStart,
  onConnectEnd,
  onNodesChange,
  onEdgesChange,
  onMoveEnd,
} = useVueFlow({
  id: "memory-canvas",
  defaultEdgeOptions: {
    type: "smoothstep",
    animated: true,
    style: { stroke: "var(--canvas-node-script)", strokeWidth: 2 },
  },
  fitViewOnInit: true,
  snapToGrid: true,
  snapGrid: [20, 20] as [number, number],
});

/** hover 连线时高亮其两端节点（Phase 4） */
const edgeHoverEndpoints = ref<Set<string>>(new Set());
onEdgeMouseEnter(({ edge }) => {
  edgeHoverEndpoints.value = new Set([edge.source, edge.target]);
});
onEdgeMouseLeave(() => {
  edgeHoverEndpoints.value = new Set();
});

// ── 键盘快捷键 ───

/** 文本输入中的按键不触发画布快捷键（便签编辑、其他输入框）。 */
function isTextEntryTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.tagName !== "string") return false;
  const tag = el.tagName.toUpperCase();
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}

function onCanvasKeydown(event: KeyboardEvent): void {
  if (!props.visible) return;
  if (isTextEntryTarget(event.target)) return;
  const key = event.key.toLowerCase();

  if (key === "escape") {
    if (nodeMenu.value || canvasMenu.value) {
      nodeMenu.value = null;
      canvasMenu.value = null;
    } else if (helpOpen.value) {
      helpOpen.value = false;
    } else if (lightboxUrl.value) {
      lightboxUrl.value = null;
    } else if (canvasTool.value !== "select") {
      canvasTool.value = "select";
    }
    return;
  }

  if (event.ctrlKey || event.metaKey) {
    if (key === "z") {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
    } else if (key === "c") {
      copySelectedNodes(event);
    } else if (key === "a") {
      event.preventDefault();
      selectAllElements();
    }
    return;
  }

  if (event.altKey) return;
  if (key === "v") canvasTool.value = "select";
  else if (key === "n") canvasTool.value = "note";
  else if (key === "i") canvasTool.value = "image";
}

// 页面隐藏/关闭时立即落库（onUnmounted 在窗口直接关闭时不保证触发，
// 防抖窗口内（500ms）的改动靠这两个事件兜底）
function flushOnHide(): void {
  if (document.visibilityState === "hidden") void flushPendingWrites();
}

onMounted(() => {
  window.addEventListener("keydown", onCanvasKeydown);
  window.addEventListener("paste", onCanvasPaste);
  document.addEventListener("visibilitychange", flushOnHide);
  window.addEventListener("pagehide", flushOnHide);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onCanvasKeydown);
  window.removeEventListener("paste", onCanvasPaste);
  document.removeEventListener("visibilitychange", flushOnHide);
  window.removeEventListener("pagehide", flushOnHide);
  void flushPendingWrites(); // 离开画布时立即落库
});

// ── 复制 / 粘贴节点 ───

const clipboardNodes = ref<MemoryNode[]>([]);
let pasteCount = 0;

function copySelectedNodes(event?: KeyboardEvent): void {
  const selected = nodes.value.filter((n) => selectedIds.value.has(n.id));
  if (selected.length === 0) return;
  clipboardNodes.value = selected.map((n) => ({ ...n }));
  pasteCount = 0;
  event?.preventDefault();
  toast.success(`已复制 ${selected.length} 个节点。`);
}

/** 粘贴：指定 at 时以点击位置为粘贴范围左上角，否则相对源位置逐次偏移。 */
async function pasteNodes(at?: { x: number; y: number }): Promise<void> {
  if (!canvasId.value || clipboardNodes.value.length === 0) return;
  pasteCount += 1;
  const sources = clipboardNodes.value;
  const minX = Math.min(...sources.map((n) => n.positionX));
  const minY = Math.min(...sources.map((n) => n.positionY));
  const step = 40 * (((pasteCount - 1) % 5) + 1);
  const dx = at ? at.x - minX : step;
  const dy = at ? at.y - minY : step;
  const created = await cloneNodes(sources, { x: dx, y: dy });
  if (created.length === 0) return;
  selectNodes(created.map((n) => n.id));
  toast.success(`已粘贴 ${created.length} 个节点。`);
}

/** 通过 vue-flow 选择 API 选中节点（stub 只需 id/position），经 select 变更同步 selectedIds。 */
function selectNodes(ids: string[]): void {
  if (ids.length === 0) return;
  const stubs = ids.map((id) => ({
    id,
    type: "canvasCard",
    position: { x: 0, y: 0 },
    data: {},
  }));
  addSelectedElements(stubs as unknown as GraphNode[]);
}

function selectAllElements(): void {
  if (nodes.value.length === 0 && edges.value.length === 0) return;
  const nodeStubs = flowNodes.value.map((n) => ({
    id: n.id,
    type: n.type,
    position: n.position,
    data: {},
  }));
  const edgeStubs = flowEdges.value.map((e) => ({ id: e.id, source: e.source, target: e.target }));
  addSelectedElements([...nodeStubs, ...edgeStubs] as unknown as GraphNode[]);
}

// ── vue-flow 变更转发（状态逻辑在 store） ───

// Delete/Backspace 删除选中（vue-flow 发出 remove 变更）
onNodesChange(handleNodeChanges);
onEdgesChange(handleEdgeChanges);

// 手动连线：拖拽节点边缘即建立 reference 关系；重复/反向给出提示
onConnect(async ({ source, target }) => {
  const result = await connectNodes(source, target);
  if (result === "created") {
    recentlyConnected = true;
    setTimeout(() => {
      recentlyConnected = false;
    }, 0);
    connectionSource.value = null;
  } else if (result === "duplicate") {
    toast.warning("这两个节点之间已经有连线了。");
  } else if (result === "reverse") {
    toast.warning("已存在反方向的连线，无需重复连接。");
  }
});

/** onConnect 成功建线后的短哨兵：避免 connectEnd 误判为「落在空白处」 */
let recentlyConnected = false;

// 连线方式补充：拖到空白处 → 原地创建一个便签并连好线（tldraw 式）
const connectionSource = ref<{ nodeId: string; handleType: string } | null>(null);

onConnectStart(({ nodeId, handleType }) => {
  connectionSource.value = nodeId ? { nodeId, handleType: handleType ?? "source" } : null;
});

onConnectEnd(async (event) => {
  if (!event) return;
  const source = connectionSource.value;
  connectionSource.value = null;
  if (!source || !canvasId.value) return;
  // onConnect 已成功建线（connect 先于 connectEnd 触发）则不再处理
  if (recentlyConnected) return;
  const pointer = "changedTouches" in event ? event.changedTouches[0] : (event as MouseEvent);
  if (!pointer) return;

  // 落点在节点上（放到无效位置）不处理
  const hit = document.elementFromPoint(pointer.clientX, pointer.clientY);
  if (hit?.closest(".canvas-node")) return;

  const flow = screenToFlowCoordinate({ x: pointer.clientX, y: pointer.clientY });
  const note = await addNoteNodeAt(flow.x - 90, flow.y - 20);
  if (!note) return;
  // source 手柄拖出 → 原节点指向新便签；target 手柄拖出 → 新便签指向原节点
  const result =
    source.handleType === "target"
      ? await connectNodes(note.id, source.nodeId)
      : await connectNodes(source.nodeId, note.id);
  if (result === "created") {
    toast.info("已在新位置创建便签并连线。", 2500);
  }
});

// 端点重连：拖动已有连线的端点换目标（保留类型与标签，一条可撤销历史）
onEdgeUpdate(async ({ edge, connection }) => {
  if (!connection?.source || !connection?.target) return;
  const ok = await reconnectEdge(edge.id, connection.source, connection.target);
  if (!ok) {
    toast.warning("无法调整到该位置（不能自连，且目标上已有连线）。");
  }
});

// 双击连线删除（带自动消失的轻提示）
onEdgeDoubleClick(async ({ edge }) => {
  await removeEdge(edge.id);
  toast.info("已删除连线，Ctrl+Z 可撤销。", 2500);
});

// 视口变化持久化
onMoveEnd(({ flowTransform }) => {
  void saveViewport(flowTransform.zoom, flowTransform.x, flowTransform.y);
});

// ── 工具栏动作 ───

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

// ── 画布空白处右键菜单：新建便签 / 导入图片 / 全选 / 粘贴 ──

async function onCanvasContextMenu(event: MouseEvent): Promise<void> {
  const target = event.target as HTMLElement;
  if (
    target.closest(".canvas-node") ||
    target.closest(".canvas-dock") ||
    target.closest(".canvas-topbar")
  ) {
    return;
  }
  event.preventDefault();
  nodeMenu.value = null;
  const flow = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
  canvasMenu.value = { x: event.clientX, y: event.clientY, flowX: flow.x, flowY: flow.y };
}

function openNodeMenu(id: string, clientX: number, clientY: number): void {
  canvasMenu.value = null;
  nodeMenu.value = { id, x: clientX, y: clientY };
}

async function onPaneMenuSelect(action: string): Promise<void> {
  const menu = canvasMenu.value;
  if (!menu) return;
  if (action === "note") await addNoteNodeAt(menu.flowX, menu.flowY);
  else if (action === "image") void pickAndAddImages();
  else if (action === "selectAll") selectAllElements();
  else if (action === "paste") await pasteNodes({ x: menu.flowX, y: menu.flowY });
}

// ── 双击空白处：在该位置新建便签 ──

function onPaneDblClick(event: MouseEvent): void {
  const flow = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
  void addNoteNodeAt(flow.x, flow.y);
}

// ── 剪贴板粘贴（Ctrl+V）：图片优先，其次粘贴已复制的节点 ──

async function onCanvasPaste(event: ClipboardEvent): Promise<void> {
  if (!props.visible || !canvasId.value) return;
  const files = [...(event.clipboardData?.files || [])].filter((f) => f.type.startsWith("image/"));
  if (files.length > 0) {
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
    return;
  }
  // 正在文本框里输入时不劫持粘贴
  if (isTextEntryTarget(event.target)) return;
  if (clipboardNodes.value.length === 0) return;
  event.preventDefault();
  await pasteNodes();
}

function fitCanvas(): void {
  void fitView({ padding: 0.2 });
}

// ── 自动整理：store 打包为一条 MacroCommand，完成后适应视图 ───

async function autoArrange(): Promise<void> {
  await arrangeNodes();
  setTimeout(() => fitView({ padding: 0.2 }), 80);
}

// ── 右键菜单动作 ───

function nodeDataUrl(node: MemoryNode): string | null {
  try {
    const p = JSON.parse(node.payloadJson);
    return p.dataUrl || p.imageUrl || null;
  } catch {
    return null;
  }
}

function downloadNodeImage(id: string): void {
  const node = nodes.value.find((n) => n.id === id);
  const url = node ? nodeDataUrl(node) : null;
  if (!node || !url) return;
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${node.summary || "canvas-image"}.png`;
  anchor.click();
}

/** 导出当前画布节点/边为 JSON 文件（含视口，便于离线备份）。 */
function exportCanvasJson(): void {
  const payload = {
    canvasId: canvasId.value,
    nodes: nodes.value,
    edges: edges.value,
    exportedAt: new Date().toISOString(),
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `memory-canvas-${canvasId.value ?? "export"}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

function cloneNode(id: string): void {
  const node = nodes.value.find((n) => n.id === id);
  if (!node) return;
  void cloneNodes(
    [{ ...node, summary: (node.summary || "副本") + " 副本" }],
    { x: 40, y: 40 },
    "复制节点",
  );
}

/** 面板侧统一处理节点菜单动作；「编辑」经 editSignal 通知卡片进入编辑态。 */
async function onNodeMenuSelect(action: string): Promise<void> {
  const target = nodeMenu.value;
  if (!target) return;
  const id = target.id;
  if (action === "edit") {
    editSignals.value = { ...editSignals.value, [id]: (editSignals.value[id] ?? 0) + 1 };
    return;
  }
  if (action === "copy") {
    cloneNode(id);
    return;
  }
  if (action === "color") {
    cycleNoteColor(id);
    return;
  }
  if (action === "preview") {
    const node = nodes.value.find((n) => n.id === id);
    const url = node ? nodeDataUrl(node) : null;
    if (url) lightboxUrl.value = url;
    return;
  }
  if (action === "download") {
    downloadNodeImage(id);
    return;
  }
  if (action === "delete") {
    await removeNode(id);
  }
}

function saveNoteLocal(id: string, text: string): void {
  void updateNoteText(id, text);
}

function removeNodeCb(id: string): void {
  void removeNode(id);
}

// Persist node position on drag end（防抖合并与命令入栈在 store）
onNodeDragStop(({ nodes: draggedNodes }) => {
  const moves: { id: string; x: number; y: number }[] = [];
  for (const fn of draggedNodes) {
    if (!nodes.value.some((n) => n.id === fn.id)) continue;
    moves.push({ id: fn.id, x: Math.round(fn.position.x), y: Math.round(fn.position.y) });
  }
  void moveNodes(moves);
});

// ─── Vue Flow nodes/edges (reactive transform) ───

const editSignals = ref<Record<string, number>>({});

const flowNodes = computed(() =>
  nodes.value.map((n) => {
    const size = getEffectiveSize(n);
    return {
      id: n.id,
      type: "canvasCard" as const,
      position: { x: n.positionX, y: n.positionY },
      class: edgeHoverEndpoints.value.has(n.id) ? "edge-endpoint-highlight" : undefined,
      data: {
        nodeType: n.nodeType,
        summary: n.summary,
        payloadJson: n.payloadJson,
        assetId: n.assetId,
        compact: compactMode.value,
        size,
        editSignal: editSignals.value[n.id] ?? 0,
        onContextMenu: (clientX: number, clientY: number) => openNodeMenu(n.id, clientX, clientY),
        onDelete: () => removeNodeCb(n.id),
        onSaveSummary: (text: string) => saveNoteLocal(n.id, text),
        onResize: (width: number, height?: number) => void resizeNode(n.id, width, height),
      },
      style: {
        width: `${size.width}px`,
        ...(size.height ? { height: `${size.height}px` } : {}),
      },
    };
  }),
);

const flowEdges = computed(() =>
  edges.value.map((e) => {
    const visual = getEdgeVisual(e.edgeType);
    return {
      id: e.id,
      source: e.sourceNodeId,
      target: e.targetNodeId,
      label: (e.label || undefined) as string | undefined,
      type: "bezier" as const,
      // reference 不再用 vue-flow 的 animated（会被渲染成虚线），线型由 style 统一
      animated: false,
      class: recentEdgeIds.value.has(e.id) ? "edge-draw" : undefined,
      style: {
        stroke: visual.stroke,
        strokeWidth: visual.strokeWidth,
        ...(visual.strokeDasharray ? { strokeDasharray: visual.strokeDasharray } : {}),
      },
      markerEnd: { type: "arrowclosed" as const, color: visual.stroke },
    };
  }),
);

/** 按边类型区分颜色与线型（Phase 4 视觉）。 */
function getEdgeVisual(edgeType: string): {
  stroke: string;
  strokeWidth: number;
  strokeDasharray?: string;
} {
  switch (edgeType) {
    case "reference":
      return { stroke: "var(--canvas-node-script)", strokeWidth: 2 };
    case "dependency":
      return { stroke: "var(--canvas-node-default)", strokeWidth: 2.5, strokeDasharray: "10 5" };
    case "link":
      return { stroke: "var(--canvas-node-video)", strokeWidth: 1.5 };
    case "similarity":
      return { stroke: "var(--canvas-node-audio)", strokeWidth: 2, strokeDasharray: "2 5" };
    default:
      return { stroke: "var(--color-text-tertiary)", strokeWidth: 1.5 };
  }
}

// ─── Canvas lifecycle ───

async function initCanvas(): Promise<void> {
  // 画布切换时重置描线动画的存量标记（加载时的边不播动画）
  knownEdgeIds.clear();
  edgesHydrated = false;
  await loadCanvas(props.conversationId);
  const vp = viewport.value;
  if (vp) {
    await nextTick();
    setTimeout(() => setViewport({ zoom: vp.zoom, x: vp.x, y: vp.y }), 120);
  }
  await nextTick();
  if (nodes.value.length > 0) {
    setTimeout(() => fitView({ padding: 0.2 }), 100);
  }
}

watch(
  () => props.conversationId,
  () => void initCanvas(),
  { immediate: true },
);

/** 加载失败后的重试（Phase 5 错误恢复）。 */
function retryLoad(): void {
  void initCanvas();
}

// ─── Exposed methods（GenerationsPage 等外部调用，直接代理 store） ───

defineExpose({
  addGenerationNode,
  addUploadNode,
  addEdgeBetweenNodes: connectNodes,
  setNodeStatus,
  setNodeImageUrl,
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
      <!-- Loading / 错误 / 空状态覆盖层 -->
      <!-- Loading -->
      <div v-if="loading" class="canvas-loading canvas-loading--overlay">
        <LoaderCircle :size="20" class="is-spinning" />
      </div>

      <!-- Load failure（Phase 5 错误恢复）：可重试 -->
      <div v-if="!loading && loadError" class="canvas-error-overlay">
        <AlertCircle :size="26" />
        <span class="canvas-error-overlay__message">画布加载失败：{{ loadError }}</span>
        <button class="canvas-error-overlay__retry" type="button" @click="retryLoad">重试</button>
      </div>

      <!-- Empty state overlay（画布始终渲染，便于手动添加节点） -->
      <div
        v-if="!loading && canvasId && nodes.length === 0"
        class="canvas-empty canvas-empty--overlay"
      >
        <ImageIcon :size="32" />
        <span>画布还是空的：按 N 进入便签模式后点击空白处创建</span>
        <span class="canvas-empty__hint"
          >也支持右键空白处新建、Ctrl+V 粘贴图片；按 ? 或工具栏帮助查看全部快捷键</span
        >
      </div>

      <!-- 画布（topbar/dock 以 panel 为锚） -->
      <!-- Vue Flow Canvas -->
      <VueFlow
        class="memory-canvas"
        :class="`memory-canvas--tool-${canvasTool}`"
        :nodes="flowNodes"
        :edges="flowEdges as any"
        :default-edge-options="{ type: 'default', animated: true }"
        :snap-to-grid="true"
        :snap-grid="[20, 20]"
        :min-zoom="0.2"
        :max-zoom="3"
        :default-viewport="{ zoom: 0.8, x: 0, y: 0 }"
        :edges-updatable="true"
        :zoom-on-scroll="true"
        :zoom-on-pinch="true"
        :pan-on-scroll="false"
        :connection-line-style="{
          stroke: 'var(--canvas-node-audio)',
          strokeWidth: 2,
          strokeDasharray: '6 4',
        }"
        :connection-radius="30"
        :elevate-nodes-on-select="true"
        :only-render-visible-elements="nodes.length > 40"
        :delete-key-code="['Backspace', 'Delete']"
        @pane-click="onPaneClick"
        @pane-double-click="onPaneDblClick"
      >
        <template #node-canvasCard="nodeProps">
          <CanvasNodeCard v-bind="nodeProps" />
        </template>

        <Background
          v-if="gridMode !== 'none'"
          :variant="gridMode"
          :pattern-color="
            gridMode === 'dots' ? 'rgba(148, 163, 248, 0.14)' : 'rgba(148, 163, 248, 0.16)'
          "
          :gap="gridMode === 'lines' ? 28 : 20"
          :size="1"
        />
        <Controls position="bottom-right" />

        <!-- 空白处右键菜单（统一组件） -->
        <CanvasContextMenu
          v-if="canvasMenu"
          :x="canvasMenu.x"
          :y="canvasMenu.y"
          :items="paneMenuItems"
          @select="onPaneMenuSelect"
          @close="canvasMenu = null"
        />

        <!-- 节点右键菜单（统一组件） -->
        <CanvasContextMenu
          v-if="nodeMenu"
          :x="nodeMenu.x"
          :y="nodeMenu.y"
          :items="nodeMenuItems"
          @select="onNodeMenuSelect"
          @close="nodeMenu = null"
        />

        <!-- 快捷键帮助 -->
        <teleport to="body">
          <div v-if="helpOpen" class="canvas-help-backdrop" @click="helpOpen = false">
            <div class="canvas-help" @click.stop>
              <div class="canvas-help__title">画布快捷键与操作</div>

              <div class="canvas-help__section">工具模式</div>
              <div class="canvas-help__row">
                <span>V / N / I</span><span>切换 选择 / 便签 / 图片 模式</span>
              </div>
              <div class="canvas-help__row">
                <span>Esc</span><span>回到选择模式 / 关闭菜单和弹窗</span>
              </div>
              <div class="canvas-help__row">
                <span>便签模式</span><span>点击空白处创建便签（可连续创建）</span>
              </div>
              <div class="canvas-help__row">
                <span>图片模式</span><span>点击空白处打开文件选择器</span>
              </div>

              <div class="canvas-help__section">节点操作</div>
              <div class="canvas-help__row">
                <span>拖拽节点</span><span>调整位置（自动保存）</span>
              </div>
              <div class="canvas-help__row">
                <span>节点右下角手柄</span><span>拖拽调整节点大小（自动保存）</span>
              </div>
              <div class="canvas-help__row">
                <span>双击便签</span><span>编辑内容（Enter 保存 / Esc 取消）</span>
              </div>
              <div class="canvas-help__row">
                <span>右键节点</span><span>编辑 / 复制 / 换色 / 查看大图 / 下载 / 删除</span>
              </div>
              <div class="canvas-help__row">
                <span>拖拽节点边缘圆点</span><span>连线到目标节点（可拖动端点重连）</span>
              </div>
              <div class="canvas-help__row">
                <span>连线拖到空白处</span><span>原地创建便签并自动连线</span>
              </div>
              <div class="canvas-help__row">
                <span>双击连线</span><span>删除连线（有提示，可撤销）</span>
              </div>

              <div class="canvas-help__section">选择与剪贴板</div>
              <div class="canvas-help__row"><span>Shift + 拖拽</span><span>框选多个节点</span></div>
              <div class="canvas-help__row"><span>Ctrl+A</span><span>全选节点和连线</span></div>
              <div class="canvas-help__row">
                <span>Ctrl+C / Ctrl+V</span><span>复制 / 粘贴选中的节点</span>
              </div>
              <div class="canvas-help__row">
                <span>Ctrl+V</span><span>粘贴剪贴板图片到画布</span>
              </div>
              <div class="canvas-help__row">
                <span>Delete / Backspace</span><span>删除选中的节点/连线</span>
              </div>

              <div class="canvas-help__section">画布</div>
              <div class="canvas-help__row">
                <span>右键空白处</span><span>新建便签 / 导入图片 / 全选 / 粘贴节点</span>
              </div>
              <div class="canvas-help__row">
                <span>双击空白处</span><span>在该位置新建便签</span>
              </div>
              <div class="canvas-help__row">
                <span>Ctrl+Z / Ctrl+Shift+Z</span><span>撤销 / 重做</span>
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
          v-if="minimapVisible"
          position="bottom-left"
          :pannable="true"
          :zoomable="true"
          :node-color="(n: any) => getNodeColor(n.data?.nodeType)"
          :mask-color="'rgb(15, 20, 30, 0.7)'"
          class="canvas-minimap"
        />
        <button
          v-if="minimapVisible"
          class="minimap-btn minimap-btn--hide"
          type="button"
          title="隐藏小地图"
          @click="toggleMinimap"
        >
          <EyeOff :size="12" />
        </button>
        <button
          v-else
          class="minimap-btn minimap-btn--show"
          type="button"
          title="显示小地图"
          @click="toggleMinimap"
        >
          <Map :size="13" />
        </button>

        <!-- 顶部悬浮条（玻璃拟态） -->
        <header class="canvas-topbar">
          <span class="canvas-topbar__brand" />
          <span class="canvas-topbar__title">工作记忆画布</span>
          <span class="canvas-topbar__stats"
            >{{ nodes.length }} 节点 · {{ edges.length }} 连线</span
          >
          <span class="canvas-topbar__spacer" />
          <button
            class="canvas-topbar__btn"
            :class="{ 'canvas-topbar__btn--accent': immersive }"
            type="button"
            :title="immersive ? '退出全屏（恢复画布面板）' : '沉浸模式（全窗口画布）'"
            @click="immersive = !immersive"
          >
            <Minimize2 v-if="immersive" :size="14" />
            <Maximize2 v-else :size="14" />
          </button>
          <button class="canvas-topbar__btn" type="button" title="关闭画布" @click="emit('close')">
            <X :size="14" />
          </button>
        </header>

        <!-- 底部工具 dock（tldraw 式，图标 + 悬停提示） -->
        <div v-if="!loading && canvasId" class="canvas-dock">
          <button
            class="canvas-dock__btn"
            :class="{ 'canvas-dock__btn--active': canvasTool === 'select' }"
            type="button"
            title="选择模式（V）：点击选中、拖拽移动"
            @click="setTool('select')"
          >
            <MousePointer2 :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            :class="{ 'canvas-dock__btn--active': canvasTool === 'note' }"
            type="button"
            title="便签模式（N）：点击空白处创建便签，Esc 退出"
            @click="setTool('note')"
          >
            <StickyNote :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            :class="{ 'canvas-dock__btn--active': canvasTool === 'image' }"
            type="button"
            title="图片模式（I）：点击空白处选择图片文件"
            @click="setTool('image')"
          >
            <ImageIcon :size="16" />
          </button>
          <span class="canvas-dock__divider" />
          <button
            class="canvas-dock__btn"
            type="button"
            title="撤销（Ctrl+Z）"
            :disabled="!canUndo"
            @click="undo"
          >
            <Undo2 :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            type="button"
            title="重做（Ctrl+Shift+Z）"
            :disabled="!canRedo"
            @click="redo"
          >
            <Redo2 :size="16" />
          </button>
          <span class="canvas-dock__divider" />
          <button
            class="canvas-dock__btn"
            type="button"
            title="自动整理（按类型分组）"
            @click="autoArrange"
          >
            <AlignStartVertical :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            type="button"
            :title="compactMode ? '详细模式' : '紧凑模式'"
            @click="compactMode = !compactMode"
          >
            <Rows3 :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            type="button"
            :title="`网格：${GRID_LABELS[gridMode]}（点击切换）`"
            @click="cycleGrid"
          >
            <Grid3x3 :size="16" />
          </button>
          <span class="canvas-dock__divider" />
          <button class="canvas-dock__btn" type="button" title="适应视图" @click="fitCanvas">
            <Maximize :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            type="button"
            title="导出画布 JSON"
            @click="exportCanvasJson"
          >
            <Download :size="16" />
          </button>
          <button
            class="canvas-dock__btn"
            type="button"
            title="快捷键说明"
            @click="helpOpen = true"
          >
            <CircleHelp :size="16" />
          </button>
          <span class="canvas-dock__divider" />
          <button
            class="canvas-dock__btn canvas-dock__btn--danger"
            type="button"
            title="删除选中（Delete）"
            :disabled="selectedCount === 0"
            @click="deleteSelected"
          >
            <Trash2 :size="16" />
          </button>
        </div>
      </VueFlow>
    </aside>
  </Transition>
</template>

<script lang="ts">
function getNodeColor(type: string | undefined): string {
  if (!type) return "var(--color-text-tertiary)";
  const colors: Record<string, string> = {
    fact: "var(--canvas-node-note)",
    note: "var(--canvas-node-image)",
    image: "var(--canvas-node-script)",
    video: "var(--canvas-node-video)",
    document: "var(--canvas-node-audio)",
    upload: "var(--canvas-node-default)",
  };
  return colors[type] || "var(--color-text-tertiary)";
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
  background: rgb(15 20 32 / 72%);
  backdrop-filter: blur(12px);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 8px 24px rgb(0 0 0 / 40%);
}

.memory-canvas-panel .vue-flow__controls-button {
  width: 32px;
  height: 32px;
  color: rgb(255 255 255 / 85%);
  border-bottom: 1px solid rgb(255 255 255 / 8%);
  background: transparent;
}

.memory-canvas-panel .vue-flow__controls-button svg {
  width: 17px;
  height: 17px;
  fill: currentcolor;
}

.memory-canvas-panel .vue-flow__controls-button:hover {
  background: rgb(99 102 241 / 28%);
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

/* 选中节点：细 ring + 柔光 */
.memory-canvas .vue-flow__node.selected .canvas-node {
  border-color: rgb(129 140 248 / 55%);
  box-shadow:
    0 0 0 1.5px rgb(129 140 248 / 90%),
    0 0 24px rgb(129 140 248 / 28%),
    0 10px 30px rgb(0 0 0 / 45%);
}

/* 拖拽中：仅阴影加深（不加 transform，避免逐帧位置更新与过渡插值打架造成卡顿） */
.memory-canvas .vue-flow__node.dragging .canvas-node {
  box-shadow: 0 14px 34px rgb(0 0 0 / 50%);
}

/* 提升节点为独立合成层，拖拽时位置更新不触发重排重绘 */
.memory-canvas .vue-flow__node {
  will-change: transform;
}

/* hover 连线时高亮其两端节点（Phase 4） */
.memory-canvas .vue-flow__node.edge-endpoint-highlight .canvas-node {
  box-shadow:
    0 0 0 2px rgb(129 140 248 / 60%),
    0 6px 20px rgb(0 0 0 / 45%);
}

/* 连接点可发现性：悬停节点时手柄放大 + 光圈，提示可拖拽连线 */
.memory-canvas .vue-flow__handle {
  transition:
    transform var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.memory-canvas .vue-flow__node:hover .vue-flow__handle {
  transform: scale(1.35);
  box-shadow: 0 0 0 3px rgb(129 140 248 / 30%);
  cursor: crosshair;
}

/* 新建连线的描线动画（Phase 4）：动画结束后恢复常规线型 */
.memory-canvas .vue-flow__edge.edge-draw .vue-flow__edge-path {
  animation: canvas-edge-draw 0.9s var(--ease-out, ease-out);
}

@keyframes canvas-edge-draw {
  from {
    stroke-dasharray: 240;
    stroke-dashoffset: 240;
  }
  to {
    stroke-dasharray: 240;
    stroke-dashoffset: 0;
  }
}

/* 创建模式下的画布光标提示 */
.memory-canvas--tool-note .vue-flow__pane,
.memory-canvas--tool-image .vue-flow__pane {
  cursor: crosshair;
}

/* ── 底部工具 dock（tldraw 式，玻璃拟态） ── */
.canvas-dock {
  position: absolute;
  bottom: var(--space-4);
  left: 50%;
  z-index: 12;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 5px;
  border: 1px solid rgb(255 255 255 / 8%);
  border-radius: 14px;
  background: rgb(13 18 30 / 80%);
  backdrop-filter: blur(14px);
  box-shadow: 0 12px 32px rgb(0 0 0 / 45%);
  transform: translateX(-50%);
}

.canvas-dock__btn {
  display: grid;
  width: 34px;
  height: 34px;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: 9px;
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.canvas-dock__btn:hover:not(:disabled) {
  background: rgb(255 255 255 / 9%);
  color: #fff;
}

.canvas-dock__btn--active,
.canvas-dock__btn--active:hover:not(:disabled) {
  color: #fff;
  background: var(--color-accent);
  box-shadow: 0 2px 12px rgb(99 102 241 / 50%);
}

.canvas-dock__btn--danger:hover:not(:disabled) {
  color: #f87171;
  background: rgb(248 113 113 / 10%);
}

.canvas-dock__btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.canvas-dock__divider {
  width: 1px;
  height: 20px;
  margin: 0 4px;
  background: rgb(255 255 255 / 10%);
}

.canvas-empty--overlay,
.canvas-loading--overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  background: rgb(10 14 22 / 45%);
  pointer-events: none;
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
  max-height: calc(100vh - 96px);
  overflow-y: auto;
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

.canvas-help__section {
  margin: var(--space-3) 0 var(--space-1);
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
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
  width: 160px;
  height: 112px;
  border: 1px solid rgb(255 255 255 / 8%);
  border-radius: 10px;
  background: rgb(15 20 32 / 72%);
  box-shadow: 0 8px 24px rgb(0 0 0 / 40%);
  backdrop-filter: blur(10px);
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
  background: var(--color-surface);
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

/* 顶部悬浮条（玻璃拟态） */
.canvas-topbar {
  position: absolute;
  top: var(--space-3);
  left: 50%;
  z-index: 12;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  max-width: calc(100% - var(--space-8));
  padding: 6px 10px 6px 14px;
  border: 1px solid rgb(255 255 255 / 8%);
  border-radius: 999px;
  background: rgb(13 18 30 / 78%);
  backdrop-filter: blur(14px);
  box-shadow: 0 8px 24px rgb(0 0 0 / 35%);
  transform: translateX(-50%);
}

.canvas-topbar__brand {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-accent);
  box-shadow: 0 0 8px rgb(99 102 241 / 80%);
}

.canvas-topbar__title {
  color: rgb(255 255 255 / 92%);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.01em;
  white-space: nowrap;
}

.canvas-topbar__stats {
  padding: 2px 9px;
  border-radius: 999px;
  background: rgb(255 255 255 / 6%);
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.canvas-topbar__spacer {
  flex: 1;
}

.canvas-topbar__btn {
  display: grid;
  width: 24px;
  height: 24px;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: 7px;
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.canvas-topbar__btn:hover {
  background: rgb(255 255 255 / 9%);
  color: #fff;
}

.canvas-topbar__btn--accent {
  color: #fff;
  background: rgb(99 102 241 / 35%);
}

.canvas-topbar__btn--accent:hover {
  background: rgb(99 102 241 / 55%);
  color: #fff;
}
/* Loading */
.canvas-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
}

/* 加载失败覆盖层（Phase 5 错误恢复） */
.canvas-error-overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: #f87171;
  background: rgb(10 14 22 / 72%);
  backdrop-filter: blur(4px);
}
.canvas-error-overlay__message {
  max-width: 70%;
  color: var(--color-text-secondary, var(--color-text));
  font-size: 12px;
  text-align: center;
  word-break: break-all;
}
.canvas-error-overlay__retry {
  height: 30px;
  padding: 0 var(--space-5);
  color: #fff;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  font-size: 12px;
  cursor: pointer;
  transition: filter var(--duration-fast) var(--ease-out);
}
.canvas-error-overlay__retry:hover {
  filter: brightness(1.1);
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

.memory-canvas {
  flex: 1;
  width: 100%;
  min-height: 0;
  /* 径向深度渐变：中心微亮，边缘沉下去 */
  background: radial-gradient(1100px 720px at 50% 38%, #151c30 0%, #0d1220 55%, #0a0e19 100%);
}

/* 小地图隐藏/显示按钮 */
.minimap-btn {
  position: absolute;
  z-index: 11;
  display: grid;
  width: 22px;
  height: 22px;
  place-items: center;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: 6px;
  background: rgb(12 16 24 / 88%);
  cursor: pointer;
  opacity: 0.75;
  transition: opacity var(--duration-fast) var(--ease-out);
}
.minimap-btn:hover {
  opacity: 1;
}
.minimap-btn--hide {
  left: 12px;
  bottom: 128px;
}
.minimap-btn--show {
  left: 12px;
  bottom: 12px;
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
