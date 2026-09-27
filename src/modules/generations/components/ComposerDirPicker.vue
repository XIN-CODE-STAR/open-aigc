<script setup lang="ts">
/**
 * ComposerDirPicker：创作器共用的项目目录选择控件。
 *
 * - 显示当前目录 chip（含恢复默认按钮）或"打开"按钮
 * - 下拉列出最近打开的工作目录 + 「添加新项目」
 * - direction：下拉弹出方向（welcome 页向下 / 对话工具栏向上）
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { FolderOpen, FolderPlus } from "@lucide/vue";

import { projectDirName } from "../../../app/stores/projectDirectory";

const props = withDefaults(
  defineProps<{
    projectDirectory: string | null;
    projectDisplayName: string;
    recentProjects?: string[];
    direction?: "up" | "down";
  }>(),
  { recentProjects: () => [], direction: "up" },
);

const emit = defineEmits<{
  "select-directory": [];
  "select-project": [path: string];
  "reset-directory": [];
}>();

const showMenu = ref(false);
const anchorRef = ref<HTMLElement | null>(null);
const recentList = computed(() => props.recentProjects ?? []);

function toggle(): void {
  if (recentList.value.length === 0) {
    emit("select-directory");
    return;
  }
  showMenu.value = !showMenu.value;
}

function pickRecent(path: string): void {
  showMenu.value = false;
  emit("select-project", path);
}

function addNewProject(): void {
  showMenu.value = false;
  emit("select-directory");
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as Node | null;
  if (anchorRef.value && target && !anchorRef.value.contains(target)) {
    showMenu.value = false;
  }
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
});
</script>

<template>
  <span ref="anchorRef" class="dir-picker">
    <button
      type="button"
      class="dir-picker__btn"
      :title="projectDirectory ? '切换输出目录' : '选择输出目录'"
      @click="toggle"
    >
      <FolderOpen :size="14" />
      <span v-if="projectDirectory" class="dir-picker__name">{{ projectDisplayName }}</span>
      <button
        v-if="projectDirectory"
        type="button"
        class="dir-picker__reset"
        title="恢复默认目录"
        @click.stop="emit('reset-directory')"
      >
        ×
      </button>
    </button>
    <Transition name="dir-picker">
      <div v-if="showMenu" class="dir-picker__menu" :class="`dir-picker__menu--${direction}`">
        <div class="dir-picker__section">最近打开</div>
        <button
          v-for="item in recentList"
          :key="item"
          type="button"
          class="dir-picker__item"
          :title="item"
          @click="pickRecent(item)"
        >
          <FolderOpen :size="13" />
          <span class="dir-picker__item-name">{{ projectDirName(item) }}</span>
        </button>
        <div class="dir-picker__divider" />
        <button type="button" class="dir-picker__item dir-picker__item--add" @click="addNewProject">
          <FolderPlus :size="13" />
          <span class="dir-picker__item-name">添加新项目</span>
        </button>
      </div>
    </Transition>
  </span>
</template>

<style scoped>
.dir-picker {
  position: relative;
  display: inline-flex;
}

.dir-picker__btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.dir-picker__btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border);
  color: var(--color-text);
}

.dir-picker__name {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-picker__reset {
  display: grid;
  width: 14px;
  height: 14px;
  place-items: center;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--color-accent);
  font-size: 12px;
  line-height: 1;
  cursor: pointer;
}

.dir-picker__reset:hover {
  background: var(--color-surface-hover);
}

/* —— 下拉菜单 —— */
.dir-picker__menu {
  position: absolute;
  left: 0;
  min-width: 220px;
  max-width: 320px;
  padding: 6px;
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  background: var(--color-surface);
  box-shadow: var(--shadow-menu);
  z-index: var(--z-dropdown);
}

.dir-picker__menu--up {
  bottom: calc(100% + 8px);
}

.dir-picker__menu--down {
  top: calc(100% + 8px);
}

.dir-picker__section {
  padding: 4px 10px 6px;
  color: var(--color-text-tertiary);
  font-size: 10px;
  font-weight: var(--font-weight-semibold);
  letter-spacing: 0.05em;
}

.dir-picker__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.dir-picker__item:hover {
  background: var(--color-surface-hover);
}

.dir-picker__item--add {
  color: var(--color-accent);
}

.dir-picker__item-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-picker__divider {
  height: 1px;
  margin: 4px 6px;
  background: var(--color-border-subtle);
}

.dir-picker-enter-active,
.dir-picker-leave-active {
  transition:
    opacity 140ms ease,
    transform 140ms var(--ease-out);
}

.dir-picker-enter-from,
.dir-picker-leave-to {
  opacity: 0;
  transform: translateY(var(--dir-picker-shift, 4px));
}

.dir-picker__menu--up {
  --dir-picker-shift: 4px;
}

.dir-picker__menu--down {
  --dir-picker-shift: -4px;
}
</style>
