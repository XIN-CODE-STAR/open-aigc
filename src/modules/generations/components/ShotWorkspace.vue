<script setup lang="ts">
/**
 * ShotWorkspace：镜头工作台布局。
 *
 * 左侧：场景/镜头树
 * 中央：镜头详情（含生成任务）
 */
import { ref, watch } from "vue";

import type { MangaProject, Scene, Shot, CharacterProfile } from "../../../bridge/manga";
import type { GenerationAttempt } from "../../../bridge/queue";
import SceneTree from "./SceneTree.vue";
import ShotDetail from "./ShotDetail.vue";

const props = defineProps<{
  project: MangaProject;
  scenes: Scene[];
  shots: Shot[];
  characters: CharacterProfile[];
  selectedShotId: string | null;
  shotAttempts: GenerationAttempt[];
  generating: boolean;
  projectId: string;
}>();

const emit = defineEmits<{
  "add-scene": [];
  "add-shot": [sceneId: string];
  "select-shot": [shot: Shot];
  "update-prompt": [shotId: string, prompt: string];
  generate: [shotId: string, prompt: string];
  cancel: [attemptId: string];
  retry: [attemptId: string];
  back: [];
}>();

const selectedShot = ref<Shot | null>(null);

// Auto-select first shot when shots change
watch(
  () => props.shots,
  (newShots) => {
    if (newShots.length > 0 && !selectedShot.value) {
      selectedShot.value = newShots[0];
      emit("select-shot", newShots[0]);
    }
    // Clear selection if shot no longer exists
    if (selectedShot.value && !newShots.find((s) => s.id === selectedShot.value?.id)) {
      selectedShot.value = newShots[0] ?? null;
      if (selectedShot.value) {
        emit("select-shot", selectedShot.value);
      }
    }
  },
  { immediate: true },
);

// Sync with parent selectedShotId
watch(
  () => props.selectedShotId,
  (id) => {
    if (id) {
      const shot = props.shots.find((s) => s.id === id);
      if (shot) selectedShot.value = shot;
    }
  },
);

function onSelectShot(shot: Shot): void {
  selectedShot.value = shot;
  emit("select-shot", shot);
}
</script>

<template>
  <div class="shot-workspace">
    <!-- 顶部项目栏 -->
    <header class="shot-workspace__header">
      <button type="button" class="shot-back" @click="emit('back')">← 返回</button>
      <span class="shot-project-title">{{ project.title }}</span>
      <span v-if="project.theme" class="shot-project-theme">{{ project.theme }}</span>
      <div v-if="characters.length > 0" class="shot-characters">
        <span v-for="char in characters.slice(0, 3)" :key="char.id" class="shot-character">
          {{ char.name }}
        </span>
      </div>
    </header>

    <!-- 主体：左树 + 右详情 -->
    <div class="shot-workspace__body">
      <SceneTree
        :scenes="scenes"
        :shots="shots"
        :selected-shot-id="selectedShot?.id ?? null"
        @select-shot="onSelectShot"
        @add-scene="emit('add-scene')"
        @add-shot="(sid) => emit('add-shot', sid)"
      />

      <main class="shot-workspace__main">
        <ShotDetail
          v-if="selectedShot"
          :shot="selectedShot"
          :attempts="shotAttempts"
          :generating="generating"
          :project-id="projectId"
          @update-prompt="(id, p) => emit('update-prompt', id, p)"
          @generate="(id, p) => emit('generate', id, p)"
          @cancel="(id) => emit('cancel', id)"
          @retry="(id) => emit('retry', id)"
        />
        <div v-else class="shot-workspace__empty">
          <p>选择一个镜头开始编辑</p>
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
.shot-workspace {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.shot-workspace__header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 20px;
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.shot-back {
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 13px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: var(--radius-control);
  transition: all var(--duration-fast) var(--ease-out);
}

.shot-back:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.shot-project-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
}

.shot-project-theme {
  font-size: 12px;
  color: var(--color-text-tertiary);
}

.shot-characters {
  display: flex;
  gap: 6px;
  margin-left: auto;
}

.shot-character {
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 11px;
  font-weight: 500;
}

.shot-workspace__body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.shot-workspace__main {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}

.shot-workspace__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--color-text-tertiary);
  font-size: 14px;
}

/* —— 移动端适配 —— */
@media (max-width: 640px) {
  .shot-workspace__body {
    flex-direction: column;
  }

  .shot-workspace__header {
    padding: 8px 14px;
    flex-wrap: wrap;
  }

  .shot-characters {
    margin-left: 0;
    width: 100%;
    margin-top: 8px;
  }

  .shot-workspace__main {
    min-height: 300px;
  }
}
</style>
