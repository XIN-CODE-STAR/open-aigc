<script setup lang="ts">
/**
 * SceneTree：左侧项目结构树。
 *
 * 展示场景 → 镜头的层级结构，支持选择和展开。
 */
import { ref } from "vue";
import { ChevronDown, ChevronRight, Clapperboard, Film, Plus, Video } from "@lucide/vue";

import type { Scene, Shot, ShotStatus } from "../../../bridge/manga";

const props = defineProps<{
  scenes: Scene[];
  shots: Shot[];
  selectedShotId: string | null;
}>();

const emit = defineEmits<{
  "select-shot": [shot: Shot];
  "add-scene": [];
  "add-shot": [sceneId: string];
}>();

const expandedScenes = ref<Set<string>>(new Set());

function toggleScene(sceneId: string): void {
  if (expandedScenes.value.has(sceneId)) {
    expandedScenes.value.delete(sceneId);
  } else {
    expandedScenes.value.add(sceneId);
  }
}

function sceneShots(sceneId: string): Shot[] {
  return props.shots.filter((s) => s.sceneId === sceneId);
}

function shotStatusIcon(status: ShotStatus) {
  switch (status) {
    case "completed":
      return "✓";
    case "generating":
      return "⟳";
    case "failed":
      return "✗";
    case "ready":
      return "●";
    default:
      return "○";
  }
}
</script>

<template>
  <aside class="scene-tree">
    <div class="scene-tree__header">
      <Clapperboard :size="14" />
      <span>项目结构</span>
      <button type="button" class="scene-tree__add" title="添加场景" @click="emit('add-scene')">
        <Plus :size="14" />
      </button>
    </div>

    <div v-if="scenes.length === 0" class="scene-tree__empty">
      还没有场景，点击 + 创建第一个场景。
    </div>

    <div v-else class="scene-tree__list">
      <div v-for="scene in scenes" :key="scene.id" class="scene-group">
        <button type="button" class="scene-group__header" @click="toggleScene(scene.id)">
          <component :is="expandedScenes.has(scene.id) ? ChevronDown : ChevronRight" :size="12" />
          <Film :size="13" />
          <span class="scene-group__title">{{ scene.title }}</span>
          <span class="scene-group__count">{{ sceneShots(scene.id).length }}</span>
        </button>

        <Transition name="tree-expand">
          <div v-if="expandedScenes.has(scene.id)" class="scene-group__shots">
            <button
              v-for="shot in sceneShots(scene.id)"
              :key="shot.id"
              type="button"
              class="shot-item"
              :class="{ 'is-selected': shot.id === selectedShotId }"
              @click="emit('select-shot', shot)"
            >
              <span class="shot-item__status" :class="`shot-status--${shot.status}`">
                {{ shotStatusIcon(shot.status) }}
              </span>
              <Video :size="12" />
              <span class="shot-item__label">镜头 {{ shot.index + 1 }}</span>
              <span v-if="shot.shotType" class="shot-item__type">{{ shot.shotType }}</span>
            </button>
            <button
              type="button"
              class="shot-item shot-item--add"
              @click="emit('add-shot', scene.id)"
            >
              <Plus :size="12" />
              <span>添加镜头</span>
            </button>
          </div>
        </Transition>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.scene-tree {
  display: flex;
  flex-direction: column;
  width: 220px;
  min-width: 180px;
  border-right: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
  overflow-y: auto;
}

.scene-tree__header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 12px 14px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  border-bottom: 1px solid var(--color-border-subtle);
}

.scene-tree__add {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  margin-left: auto;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.scene-tree__add:hover {
  background: var(--color-surface-hover);
  color: var(--color-accent);
}

.scene-tree__empty {
  padding: 24px 14px;
  text-align: center;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.scene-tree__list {
  padding: 4px 0;
}

.scene-group__header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 8px 14px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.scene-group__header:hover {
  background: var(--color-surface-hover);
}

.scene-group__title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scene-group__count {
  font-size: 11px;
  color: var(--color-text-tertiary);
}

.scene-group__shots {
  padding-left: 16px;
}

.shot-item {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 6px 14px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.shot-item:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.shot-item.is-selected {
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-weight: 500;
}

.shot-item--add {
  color: var(--color-text-tertiary);
  font-size: 11px;
  padding: 4px 14px;
}

.shot-item--add:hover {
  color: var(--color-accent);
}

.shot-item__status {
  font-size: 10px;
  width: 14px;
  text-align: center;
}

.shot-status--completed {
  color: var(--color-success);
}

.shot-status--generating {
  color: var(--color-accent);
}

.shot-status--failed {
  color: var(--color-danger);
}

.shot-status--ready {
  color: var(--color-warning);
}

.shot-status--draft {
  color: var(--color-text-tertiary);
}

.shot-item__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.shot-item__type {
  font-size: 10px;
  color: var(--color-text-tertiary);
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--color-surface-subtle);
}

/* —— 展开/折叠动画 —— */
.tree-expand-enter-active,
.tree-expand-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-height var(--duration-fast) var(--ease-out);
  overflow: hidden;
}

.tree-expand-enter-from,
.tree-expand-leave-to {
  opacity: 0;
  max-height: 0;
}

.tree-expand-enter-to,
.tree-expand-leave-from {
  opacity: 1;
  max-height: 500px;
}

/* —— 移动端适配 —— */
@media (max-width: 640px) {
  .scene-tree {
    width: 100%;
    min-width: 0;
    max-height: 200px;
    border-right: none;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .scene-group__header {
    padding: 10px 14px;
  }

  .shot-item {
    padding: 8px 14px;
  }
}
</style>
