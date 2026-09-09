<script setup lang="ts">
/**
 * CreationResultGrid：非 Agent 模式的创作结果列表。
 *
 * 每个任务以"用户 prompt + AI 状态回复"的成对气泡呈现，
 * 状态包括：pending / running / succeeded / failed。
 * 成功时自动加载结果资产并展示图片/视频预览。
 */
import { onMounted, ref, watch } from "vue";
import { AlertCircle, CheckCircle2, FileUp, LoaderCircle } from "@lucide/vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listResults, type GenerationTaskRecord } from "../../../bridge/generations";
import { getAsset, type AssetRecord } from "../../../bridge/assets";
import { getWorkspaceRootPath } from "../../../bridge/workspace";

interface ResultAsset {
  asset: AssetRecord;
  displayUrl: string;
}

/** 缓存 managed-files 根目录 */
let managedFilesDir: string | null = null;

async function ensureManagedFilesDir(): Promise<string> {
  if (!managedFilesDir) {
    try {
      const root = await getWorkspaceRootPath();
      managedFilesDir = root.managedFilesDir;
    } catch {
      managedFilesDir = "";
    }
  }
  return managedFilesDir;
}

const props = defineProps<{
  /** 已排序的生成任务列表。 */
  tasks: GenerationTaskRecord[];
  /** 当前正在录入结果的任务 id（用于按钮 spinner 状态）。 */
  recordBusyTaskId: string | null;
  /** 录入结果失败的错误信息。 */
  recordError: string | null;
  /** 时间格式化函数。 */
  formatTime: (iso: string) => string;
}>();

const emit = defineEmits<{
  /** 用户点击"录入结果"按钮。 */
  "attach-result": [task: GenerationTaskRecord];
}>();

/** 缓存已加载的结果资产：taskId → ResultAsset[] */
const resultCache = ref<Map<string, ResultAsset[]>>(new Map());
const loadingResults = ref<Set<string>>(new Set());

function onAttach(task: GenerationTaskRecord): void {
  emit("attach-result", task);
}

/**
 * 为已完成的任务加载结果资产。
 */
async function loadResults(task: GenerationTaskRecord): Promise<void> {
  if (resultCache.value.has(task.id) || loadingResults.value.has(task.id)) return;
  loadingResults.value.add(task.id);
  try {
    const rootDir = await ensureManagedFilesDir();
    const results = await listResults(task.id);
    const assets: ResultAsset[] = [];
    for (const r of results) {
      if (!r.assetId) continue;
      try {
        const asset = await getAsset(r.assetId);
        if (asset) {
          const fullPath = rootDir ? `${rootDir}/${asset.relativePath.replace(/\//g, "\\")}` : "";
          const displayUrl = fullPath ? convertFileSrc(fullPath) : "";
          if (displayUrl) {
            assets.push({ asset, displayUrl });
          }
        }
      } catch (e) {
        console.warn("[ResultGrid] Failed to load asset:", r.assetId, e);
      }
    }
    resultCache.value.set(task.id, assets);
  } catch (e) {
    console.warn("[ResultGrid] Failed to load results for task:", task.id, e);
  } finally {
    loadingResults.value.delete(task.id);
  }
}

// 监听任务变化，自动加载已完成任务的结果
watch(
  () => props.tasks,
  (newTasks) => {
    for (const task of newTasks) {
      if (task.status === "succeeded") {
        void loadResults(task);
      }
    }
  },
  { immediate: true, deep: true },
);

onMounted(() => {
  for (const task of props.tasks) {
    if (task.status === "succeeded") {
      void loadResults(task);
    }
  }
});
</script>

<template>
  <div class="grok-messages">
    <TransitionGroup name="turn">
      <div v-for="task in tasks" :key="task.id" class="turn">
        <!-- 用户 prompt -->
        <div class="turn-user">
          <p class="user-text">{{ task.promptText }}</p>
          <div class="turn-meta">
            <span>{{ formatTime(task.createdAt) }}</span>
            <span class="dot">·</span>
            <span>{{ task.modelName }}</span>
          </div>
        </div>

        <!-- AI 状态回复 -->
        <div class="turn-ai" :class="`turn-ai--${task.status}`">
          <div v-if="task.status === 'pending' || task.status === 'running'" class="ai-status">
            <LoaderCircle :size="13" class="is-spinning" />
            <span>{{ task.status === "pending" ? "排队中…" : "生成中…" }}</span>
          </div>

          <div v-else-if="task.status === 'succeeded'" class="ai-status">
            <CheckCircle2 :size="13" />
            <span class="ai-status-text">生成完成</span>
            <button
              type="button"
              class="ai-inline-btn"
              :disabled="recordBusyTaskId === task.id"
              @click="onAttach(task)"
            >
              <LoaderCircle v-if="recordBusyTaskId === task.id" :size="11" class="is-spinning" />
              <FileUp v-else :size="11" />
              <span>录入结果</span>
            </button>
          </div>

          <div v-else class="ai-status ai-status--fail">
            <AlertCircle :size="13" />
            <span class="ai-status-text">生成失败</span>
            <p v-if="task.errorMessage" class="ai-fail-msg">
              {{ task.errorMessage }}
            </p>
            <p v-else class="ai-fail-msg">AI 凭据可能未配置或调用异常。</p>
            <button
              type="button"
              class="ai-inline-btn"
              :disabled="recordBusyTaskId === task.id"
              @click="onAttach(task)"
            >
              <LoaderCircle v-if="recordBusyTaskId === task.id" :size="11" class="is-spinning" />
              <FileUp v-else :size="11" />
              <span>手动录入</span>
            </button>
          </div>

          <!-- 结果资产预览 -->
          <div
            v-if="resultCache.has(task.id) && resultCache.get(task.id)!.length > 0"
            class="result-assets"
          >
            <div
              v-for="(item, idx) in resultCache.get(task.id)"
              :key="idx"
              class="result-asset-item"
            >
              <img
                v-if="item.asset.assetKind === 'image'"
                :src="item.displayUrl"
                :alt="item.asset.displayName"
                class="result-image"
                loading="lazy"
              />
              <video
                v-else-if="item.asset.assetKind === 'video'"
                :src="item.displayUrl"
                class="result-video"
                controls
                preload="metadata"
              />
              <div v-else class="result-file">
                <span>{{ item.asset.displayName }}</span>
              </div>
            </div>
          </div>
          <div v-else-if="loadingResults.has(task.id)" class="result-loading">
            <LoaderCircle :size="12" class="is-spinning" />
            <span>加载结果中…</span>
          </div>
        </div>
      </div>
    </TransitionGroup>

    <p v-if="recordError && recordBusyTaskId === null" class="record-error" role="alert">
      {{ recordError }}
    </p>
  </div>
</template>

<style scoped>
.grok-messages {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 8px 0 24px;
}

.turn {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.turn-user {
  align-self: flex-end;
  max-width: 80%;
  padding: 8px 12px;
  background: var(--color-accent-soft);
  border: 1px solid var(--color-border-subtle);
  border-radius: 12px 12px 4px 12px;
  color: var(--color-text);
  font-size: 13px;
  line-height: 1.5;
}

.user-text {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.turn-meta {
  margin-top: 4px;
  font-size: 10px;
  color: var(--color-text-tertiary);
  display: flex;
  gap: 6px;
}

.dot {
  opacity: 0.6;
}

.turn-ai {
  align-self: flex-start;
  max-width: 90%;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 0 8px 12px;
  border-left: 2px solid var(--color-border-subtle);
}

.turn-ai--succeeded {
  border-left-color: var(--color-success-soft);
}

.turn-ai--failed {
  border-left-color: var(--color-danger-soft);
}

.ai-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-text-secondary);
  flex-wrap: wrap;
}

.ai-status--fail {
  color: var(--color-danger);
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
}

.ai-fail-msg {
  margin: 0;
  font-size: 11px;
  color: var(--color-text-secondary);
}

.ai-inline-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: 6px;
  background: transparent;
  border: 1px solid var(--color-border-subtle);
  color: var(--color-text);
  font-size: 11px;
  cursor: pointer;
  transition: background 120ms ease;
}

.ai-inline-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.ai-inline-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 结果资产预览 */
.result-assets {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 4px;
}

.result-asset-item {
  border-radius: 8px;
  overflow: hidden;
  background: var(--color-surface);
  border: 1px solid var(--color-border-subtle);
}

.result-image {
  max-width: 320px;
  max-height: 320px;
  display: block;
  object-fit: contain;
}

.result-video {
  max-width: 400px;
  max-height: 360px;
  display: block;
  border-radius: 4px;
}

.result-file {
  padding: 8px 12px;
  font-size: 12px;
  color: var(--color-text-secondary);
}

.result-loading {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--color-text-tertiary);
  margin-top: 4px;
}

.record-error {
  margin: 8px 0 0;
  font-size: 11px;
  color: var(--color-danger);
}

.turn-enter-active,
.turn-leave-active {
  transition:
    opacity 200ms ease,
    transform 200ms ease;
}

.turn-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.turn-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

@keyframes is-spinning {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.is-spinning {
  animation: is-spinning 1s linear infinite;
}
</style>
