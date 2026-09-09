<script setup lang="ts">
/**
 * CanvasNodeCard：画布节点卡片。
 *
 * vue-flow 自定义节点类型，展示单个工作记忆节点：
 * - 类型图标 + 类型标签
 * - 摘要文字
 * - 状态徽标 (pending/generating/succeeded/failed)
 * - 图片类型可显示缩略图
 */
import { computed, ref } from "vue";
import { Handle, Position } from "@vue-flow/core";

const props = defineProps<{
  id: string;
  data: {
    nodeType: string;
    summary: string;
    payloadJson: string;
    assetId?: string | null;
    /** 紧凑模式：只显示头部与摘要，隐藏缩略图与提示词。 */
    compact?: boolean;
    onDelete?: () => void;
    onSaveSummary?: (text: string) => void;
    onMenuAction?: (action: "clone" | "preview" | "download" | "color" | "delete") => void;
  };
}>();

const isNote = computed(() => props.data.nodeType === "note");
const isImage = computed(() => props.data.nodeType === "image" || props.data.nodeType === "upload");
const menuOpen = ref(false);
const menuX = ref(0);
const menuY = ref(0);

function openMenu(event: MouseEvent): void {
  menuX.value = event.clientX;
  menuY.value = event.clientY;
  menuOpen.value = true;
}

function menuAction(action: "clone" | "preview" | "download" | "color" | "delete"): void {
  menuOpen.value = false;
  props.data.onMenuAction?.(action);
}

const hasImage = computed(() => {
  try {
    const p = JSON.parse(props.data.payloadJson);
    return !!(p.dataUrl || p.imageUrl);
  } catch {
    return false;
  }
});
const editing = ref(false);
const editText = ref("");

function startEdit(): void {
  if (!isNote.value || !props.data.onSaveSummary) return;
  editText.value = props.data.summary;
  editing.value = true;
}

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
  const colors: Record<string, string> = {
    fact: "#8b5cf6",
    note: "#3b82f6",
    image: "#22c55e",
    video: "#f59e0b",
    document: "#06b6d4",
    upload: "#ec4899",
  };
  return colors[props.data.nodeType] || "#6b7280";
});

const nodeIcon = computed(() => {
  const icons: Record<string, string> = {
    fact: "📝",
    note: "📋",
    image: "🖼️",
    video: "🎬",
    document: "📄",
    upload: "📎",
  };
  return icons[props.data.nodeType] || "📦";
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
</script>

<template>
  <div
    class="canvas-node"
    :style="{ borderTopColor: nodeColor }"
    @contextmenu.stop.prevent="openMenu($event)"
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
      <span class="canvas-node__icon">{{ nodeIcon }}</span>
      <span class="canvas-node__type" :style="{ color: nodeColor }">{{ data.nodeType }}</span>
      <span v-if="statusLabel" class="canvas-node__status" :class="`is-${status}`">
        {{ statusLabel }}
      </span>
    </div>

    <!-- Thumbnail (for image/upload nodes with dataUrl) -->
    <div v-if="showThumbnail" class="canvas-node__thumb">
      <img
        :src="thumbnailUrl!"
        alt=""
        @error="($event.target as HTMLImageElement).style.display = 'none'"
      />
    </div>

    <teleport to="body">
      <div
        v-if="menuOpen"
        class="canvas-menu-backdrop"
        @click.stop="menuOpen = false"
        @contextmenu.stop.prevent="menuOpen = false"
      >
        <div class="canvas-menu" :style="{ left: `${menuX}px`, top: `${menuY}px` }" @click.stop>
          <button
            v-if="isImage && hasImage"
            class="canvas-menu__item"
            type="button"
            @click="menuAction('preview')"
          >
            查看大图
          </button>
          <button
            v-if="isImage && hasImage"
            class="canvas-menu__item"
            type="button"
            @click="menuAction('download')"
          >
            下载图片
          </button>
          <button
            v-if="isNote"
            class="canvas-menu__item"
            type="button"
            @click="menuAction('color')"
          >
            换个颜色
          </button>
          <button class="canvas-menu__item" type="button" @click="menuAction('clone')">
            克隆节点
          </button>
          <button
            class="canvas-menu__item canvas-menu__item--danger"
            type="button"
            @click="menuAction('delete')"
          >
            删除节点
          </button>
        </div>
      </div>
    </teleport>

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
  </div>
</template>

<style scoped>
.canvas-node {
  position: relative;
}

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
.canvas-node {
  min-width: 180px;
  max-width: 260px;
  border-top: 3px solid #6b7280;
  border-radius: 8px;
  background: var(--color-surface-subtle, #1a1f2e);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  padding: 10px 12px;
  font-size: 12px;
  cursor: grab;
  transition: box-shadow 0.15s ease;
}
.canvas-node:hover {
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
}

.canvas-node__header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.canvas-node__icon {
  font-size: 14px;
}
.canvas-node__type {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: 600;
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
  color: #6366f1;
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

.canvas-node__thumb {
  margin: 6px -2px;
  border-radius: 4px;
  overflow: hidden;
  aspect-ratio: 16/10;
  background: rgba(0, 0, 0, 0.2);
}
.canvas-node__thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.canvas-node__summary {
  color: var(--color-text, #e2e8f0);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.canvas-node__prompt {
  margin-top: 4px;
  color: var(--color-text-tertiary, #64748b);
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
