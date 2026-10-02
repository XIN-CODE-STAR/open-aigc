<script setup lang="ts">
/**
 * CanvasNodeCard：画布节点卡片。
 *
 * vue-flow 自定义节点类型，展示单个工作记忆节点：
 * - 类型图标 + 类型标签
 * - 摘要文字
 * - 状态徽标 (pending/generating/succeeded/failed)
 * - 图片类型可显示缩略图
 * - 右下角 resize 手柄：拖拽改宽高，松手后经 onResize 持久化
 * - 右键菜单统一交给父级（data.onContextMenu 上报坐标）
 */
import { computed, ref, watch } from "vue";
import { Handle, Position, useVueFlow } from "@vue-flow/core";

import { clampNodeSize } from "../canvasNodeSize";
import { thumbnailFor } from "../imageCompress";

const props = defineProps<{
  id: string;
  data: {
    nodeType: string;
    summary: string;
    payloadJson: string;
    assetId?: string | null;
    /** 紧凑模式：只显示头部与摘要，隐藏缩略图与提示词。 */
    compact?: boolean;
    /** 节点显式尺寸（来自 payload.size）；缺省高度由内容撑开。 */
    size?: { width: number; height?: number };
    /** 右键菜单统一由面板打开：上报屏幕坐标即可。 */
    onContextMenu?: (clientX: number, clientY: number) => void;
    /** 双击缩略图放大预览（图片节点）。 */
    onPreview?: () => void;
    /** 面板菜单触发「编辑」时的信号（递增计数）。 */
    editSignal?: number;
    onDelete?: () => void;
    onSaveSummary?: (text: string) => void;
    /** 拖拽 resize 结束时回调；height 未拖动时为 undefined（保持自适应）。 */
    onResize?: (width: number, height?: number) => void;
  };
}>();

const isNote = computed(() => props.data.nodeType === "note");

const rootRef = ref<HTMLElement | null>(null);
const { viewport } = useVueFlow();

function onNodeContextMenu(event: MouseEvent): void {
  props.data.onContextMenu?.(event.clientX, event.clientY);
}

/** resize 手柄拖拽：除以 zoom 换算回 flow 坐标，实时改节点 wrapper 尺寸，松手持久化。 */
function startResize(event: PointerEvent): void {
  const root = rootRef.value;
  if (!root || !props.data.onResize) return;
  event.preventDefault();
  event.stopPropagation();

  const wrapper = root.closest(".vue-flow__node") as HTMLElement | null;
  const startX = event.clientX;
  const startY = event.clientY;
  const startWidth = root.offsetWidth;
  const startHeight = root.offsetHeight;
  const zoom = viewport.value.zoom || 1;
  let heightTouched = false;
  let nextWidth = startWidth;
  let nextHeight = startHeight;

  const onMove = (moveEvent: PointerEvent): void => {
    const dx = (moveEvent.clientX - startX) / zoom;
    const dy = (moveEvent.clientY - startY) / zoom;
    if (Math.abs(moveEvent.clientY - startY) > 4) heightTouched = true;
    const size = clampNodeSize(startWidth + dx, heightTouched ? startHeight + dy : undefined);
    nextWidth = size.width;
    if (size.height !== undefined) nextHeight = size.height;
    if (wrapper) {
      wrapper.style.width = `${nextWidth}px`;
      if (heightTouched) wrapper.style.height = `${nextHeight}px`;
    }
  };
  const onUp = (): void => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    props.data.onResize?.(nextWidth, heightTouched ? nextHeight : undefined);
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

const editing = ref(false);
const editText = ref("");

function startEdit(): void {
  if (!isNote.value || !props.data.onSaveSummary) return;
  editText.value = props.data.summary;
  editing.value = true;
}

// 菜单里的「编辑」由面板经 editSignal 递增触发
watch(
  () => props.data.editSignal,
  (value, previous) => {
    if (value && value !== previous) startEdit();
  },
);

function saveEdit(): void {
  if (!editing.value) return;
  editing.value = false;
  const text = editText.value.trim();
  if (text && text !== props.data.summary) props.data.onSaveSummary?.(text);
}

const status = computed(() => {
  try {
    return JSON.parse(props.data.payloadJson).status || null;
  } catch {
    return null;
  }
});

const prompt = computed(() => {
  try {
    return JSON.parse(props.data.payloadJson).prompt || null;
  } catch {
    return null;
  }
});

const description = computed(() => {
  try {
    return JSON.parse(props.data.payloadJson).description || null;
  } catch {
    return null;
  }
});

const thumbnailUrl = computed(() => {
  try {
    const p = JSON.parse(props.data.payloadJson);
    return p.dataUrl || p.thumbnailUrl || p.imageUrl || null;
  } catch {
    return null;
  }
});

const nodeColor = computed(() => {
  // 节点类型色系：读 tokens.css 的 --canvas-node-*，随主题联动
  const colors: Record<string, string> = {
    fact: "var(--canvas-node-note)",
    note: "var(--canvas-node-image)",
    image: "var(--canvas-node-script)",
    video: "var(--canvas-node-video)",
    document: "var(--canvas-node-audio)",
    upload: "var(--canvas-node-default)",
  };
  return colors[props.data.nodeType] || "var(--canvas-node-mark)";
});

const statusLabel = computed(() => {
  const labels: Record<string, string> = {
    pending: "等待中",
    generating: "生成中",
    succeeded: "已完成",
    failed: "失败",
  };
  return status.value ? labels[status.value] || status.value : null;
});

const showThumbnail = computed(() => {
  return (
    !props.data.compact &&
    thumbnailUrl.value &&
    (props.data.nodeType === "image" || props.data.nodeType === "upload") &&
    !thumbnailUrl.value.startsWith("pending://")
  );
});

const TYPE_LABELS: Record<string, string> = {
  fact: "事实",
  note: "便签",
  image: "图片",
  video: "视频",
  document: "文档",
  upload: "素材",
};
const typeLabel = computed(() => TYPE_LABELS[props.data.nodeType] ?? props.data.nodeType);

/** 便签纸质感：payload.color 调色（缺省琥珀纸色）+ 细横纹纹理。 */
const noteColor = computed(() => {
  if (!isNote.value) return "var(--canvas-node-mark)";
  try {
    const color = JSON.parse(props.data.payloadJson).color;
    return typeof color === "string" && color.startsWith("#") ? color : "var(--canvas-node-mark)";
  } catch {
    return "var(--canvas-node-mark)";
  }
});

const rootStyle = computed(() => {
  const style: Record<string, string> = {};
  if (isNote.value) {
    style.background = [
      `linear-gradient(165deg, color-mix(in srgb, ${noteColor.value} 20%, var(--color-surface-subtle)), var(--color-surface-subtle) 72%)`,
      "repeating-linear-gradient(0deg, rgb(255 255 255 / 2.5%) 0 1px, transparent 1px 3px)",
    ].join(", ");
  }
  return style;
});

// Phase 5：大图渲染缩略图（Canvas 生成），小图直接用原图
const displaySrc = ref<string | null>(null);
let displaySrcToken = 0;

async function resolveDisplaySrc(): Promise<void> {
  const token = ++displaySrcToken;
  const source = thumbnailUrl.value;
  if (!source) {
    displaySrc.value = null;
    return;
  }
  const result = await thumbnailFor(source, `${props.id}:${source.length}`);
  if (token === displaySrcToken) displaySrc.value = result;
}

watch(
  () => props.data.payloadJson,
  () => void resolveDisplaySrc(),
  { immediate: true },
);
</script>

<template>
  <div
    ref="rootRef"
    class="canvas-node"
    :style="rootStyle"
    @contextmenu.stop.prevent="onNodeContextMenu($event)"
  >
    <!-- Handles for edge connections -->
    <Handle type="target" :position="Position.Left" :style="{ background: nodeColor }" />
    <Handle type="source" :position="Position.Right" :style="{ background: nodeColor }" />

    <button
      v-if="data.onDelete"
      class="canvas-node__delete"
      type="button"
      title="删除节点"
      aria-label="删除节点"
      @click.stop="data.onDelete?.()"
    >
      ×
    </button>

    <!-- Header: icon + type -->
    <div class="canvas-node__header">
      <span class="canvas-node__type" :style="{ color: nodeColor }">{{ typeLabel }}</span>
      <span v-if="statusLabel" class="canvas-node__status" :class="`is-${status}`">
        {{ statusLabel }}
      </span>
    </div>

    <!-- Thumbnail (for image/upload nodes with dataUrl) -->
    <div
      v-if="showThumbnail"
      class="canvas-node__thumb"
      title="双击放大预览"
      @dblclick.stop="data.onPreview?.()"
    >
      <img
        :src="displaySrc ?? thumbnailUrl!"
        alt=""
        loading="lazy"
        decoding="async"
        @error="($event.target as HTMLImageElement).style.display = 'none'"
      />
    </div>

    <!-- Note editing（双击便签编辑内容） -->
    <div v-if="isNote && editing" class="canvas-node__edit">
      <textarea
        v-model="editText"
        rows="3"
        @keydown.enter.exact.prevent="saveEdit"
        @keydown.esc.prevent="editing = false"
        @blur="saveEdit"
      />
    </div>
    <div
      v-else-if="!editing"
      class="canvas-node__summary"
      :title="data.summary"
      @dblclick="startEdit"
    >
      {{ data.summary }}
    </div>

    <!-- Prompt preview (truncated) -->
    <div v-if="prompt && !props.data.compact" class="canvas-node__prompt" :title="prompt">
      {{ prompt.slice(0, 60) }}{{ prompt.length > 60 ? "..." : "" }}
    </div>

    <!-- Description preview -->
    <div v-if="description && !prompt" class="canvas-node__prompt" :title="description">
      {{ description.slice(0, 60) }}{{ description.length > 60 ? "..." : "" }}
    </div>

    <!-- Resize handle（右下角拖拽改宽高；拦截 mousedown/touchstart 避免 vue-flow 同时拖动节点） -->
    <div
      class="canvas-node__resize"
      title="拖拽调整大小"
      @pointerdown.stop="startResize($event)"
      @mousedown.stop.prevent
      @touchstart.stop
    />
  </div>
</template>

<style scoped>
.canvas-node {
  position: relative;
  display: flex;
  flex-direction: column;
  /* 宽高由 vue-flow 节点 wrapper 控制（含 resize 后的显式值），卡片填满即可 */
  width: 100%;
  height: 100%;
  overflow: hidden;
  border: 1px solid rgb(255 255 255 / 7%);
  border-radius: 12px;
  background: var(--color-surface-subtle);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.24);
  padding: 10px 12px;
  font-size: 12px;
  cursor: grab;
  /* 不过渡 transform：拖拽时 vue-flow 逐帧更新位置，过渡会造成视觉拖影/卡顿 */
  transition:
    box-shadow 0.18s ease,
    border-color 0.18s ease;
}
.canvas-node:hover {
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
}

/* 右下角 resize 手柄 */
.canvas-node__resize {
  position: absolute;
  right: 0;
  bottom: 0;
  width: 16px;
  height: 16px;
  border-bottom-right-radius: 8px;
  cursor: nwse-resize;
  touch-action: none;
  opacity: 0;
  background: linear-gradient(
    135deg,
    transparent 0 46%,
    var(--color-text-tertiary) 46% 54%,
    transparent 54% 64%,
    var(--color-text-tertiary) 64% 72%,
    transparent 72%
  );
  transition: opacity var(--duration-fast) var(--ease-out);
}
.canvas-node:hover .canvas-node__resize {
  opacity: 0.85;
}

.canvas-node__delete {
  position: absolute;
  top: 6px;
  right: 6px;
  display: grid;
  width: 18px;
  height: 18px;
  place-items: center;
  color: var(--color-text-tertiary);
  border: none;
  border-radius: 50%;
  background: rgb(0 0 0 / 35%);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.canvas-node:hover .canvas-node__delete {
  opacity: 1;
}

.canvas-node__delete:hover {
  color: #f87171;
}

.canvas-node__edit textarea {
  width: 100%;
  padding: var(--space-2);
  color: var(--color-text);
  border: 1px solid var(--color-accent);
  border-radius: 6px;
  background: var(--color-surface);
  font-size: var(--text-footnote);
  font-family: inherit;
  line-height: 1.5;
  resize: vertical;
  outline: none;
}

.canvas-node__header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
  flex-shrink: 0;
}
.canvas-node__type {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
}
.canvas-node__status {
  margin-left: auto;
  font-size: 10px;
  font-weight: 500;
  padding: 1px 6px;
  border-radius: 4px;
}
.canvas-node__status.is-pending {
  color: #94a3b8;
  background: rgba(148, 163, 184, 0.1);
}
.canvas-node__status.is-generating {
  color: var(--canvas-node-script);
  background: rgba(99, 102, 241, 0.1);
  animation: pulse 1.5s ease-in-out infinite;
}
.canvas-node__status.is-succeeded {
  color: #22c55e;
  background: rgba(34, 197, 94, 0.1);
}
.canvas-node__status.is-failed {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.1);
}

/* 缩略图自适应：节点有显式高度时占满剩余空间，否则保持 16:10 */
.canvas-node__thumb {
  flex: 1 1 auto;
  min-height: 0;
  margin: 6px -2px;
  border-radius: 4px;
  overflow: hidden;
  aspect-ratio: 16/10;
  background: rgba(0, 0, 0, 0.2);
}
.canvas-node__thumb {
  cursor: zoom-in;
}
.canvas-node__thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 0.25s ease;
}
/* 图片节点 hover 缩放微动画 */
.canvas-node:hover .canvas-node__thumb img {
  transform: scale(1.045);
}

.canvas-node__summary {
  flex-shrink: 1;
  min-height: 0;
  color: var(--color-text);
  line-height: 1.4;
  white-space: pre-wrap;
  word-break: break-word;
  overflow: hidden;
}
.canvas-node__prompt {
  flex-shrink: 0;
  margin-top: 4px;
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.6;
  }
}
</style>
