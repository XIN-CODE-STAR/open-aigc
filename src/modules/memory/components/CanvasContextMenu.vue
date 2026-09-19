<script lang="ts">
/** 菜单项定义；key 由调用方解释（节点菜单 / 空白菜单共用本组件）。 */
export interface CanvasMenuItem {
  key: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
}
</script>

<script setup lang="ts">
/**
 * CanvasContextMenu：画布统一右键菜单（节点与空白处共用）。
 *
 * 调用方传入屏幕坐标与菜单项列表，组件只负责渲染与关闭语义：
 * 点击/右键背景关闭，选中项后自动关闭；靠近屏幕边缘时自动收进视口。
 */
import { nextTick, onMounted, ref, watch } from "vue";

const props = defineProps<{
  x: number;
  y: number;
  items: CanvasMenuItem[];
}>();

const emit = defineEmits<{
  select: [key: string];
  close: [];
}>();

const menuRef = ref<HTMLElement | null>(null);
const pos = ref({ x: props.x, y: props.y });

onMounted(clampIntoViewport);
watch(
  () => [props.x, props.y],
  () => void clampIntoViewport(),
);

async function clampIntoViewport(): Promise<void> {
  pos.value = { x: props.x, y: props.y };
  await nextTick();
  const el = menuRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  pos.value = {
    x: Math.max(8, Math.min(props.x, window.innerWidth - rect.width - 8)),
    y: Math.max(8, Math.min(props.y, window.innerHeight - rect.height - 8)),
  };
}

function choose(item: CanvasMenuItem): void {
  if (item.disabled) return;
  emit("select", item.key);
  emit("close");
}
</script>

<template>
  <teleport to="body">
    <div class="canvas-menu-backdrop" @click="emit('close')" @contextmenu.prevent="emit('close')">
      <div
        ref="menuRef"
        class="canvas-menu"
        :style="{ left: `${pos.x}px`, top: `${pos.y}px` }"
        @click.stop
      >
        <button
          v-for="item in items"
          :key="item.key"
          class="canvas-menu__item"
          :class="{ 'canvas-menu__item--danger': item.danger }"
          type="button"
          :disabled="item.disabled"
          @click="choose(item)"
        >
          {{ item.label }}
        </button>
      </div>
    </div>
  </teleport>
</template>

<style scoped>
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
  min-width: 148px;
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

.canvas-menu__item:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.canvas-menu__item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.canvas-menu__item--danger {
  color: #f87171;
}
</style>
