<script setup lang="ts">
/**
 * CreativeMemoryPanel：创意记忆管理面板。
 *
 * 展示和管理用户的风格偏好、负面偏好、品牌规范等记忆。
 * 支持记忆的增删改查、暂停/恢复、置信度确认。
 */
import { ref } from "vue";
import {
  Brain,
  Palette,
  XCircle,
  Shield,
  Workflow,
  FileText,
  Clock,
  Plus,
  Pause,
  Play,
  Trash2,
  CheckCircle,
  Star,
  ChevronDown,
  ChevronRight,
} from "@lucide/vue";

import { useCreativeMemory, type MemoryType } from "../composables/useCreativeMemory";

const props = defineProps<{
  userId: string;
}>();

const {
  memories,
  loading,
  error,
  saving,
  memoriesByType,
  activeMemories,
  loadMemories,
  saveMemory,
  pauseMemory,
  resumeMemory,
  removeMemory,
  confirmMemoryById,
  memoryTypeLabel,
  memoryStatusLabel,
} = useCreativeMemory({ userId: props.userId, autoLoad: true });

const showAddForm = ref(false);
const newMemoryType = ref<MemoryType>("style_preference");
const newSummary = ref("");
const newContent = ref("");
const expandedTypes = ref<Set<MemoryType>>(new Set(["style_preference"]));

const memoryTypes: MemoryType[] = [
  "style_preference",
  "negative_preference",
  "brand_rule",
  "workflow_habit",
  "prompt_pattern",
  "review_history",
];

function typeIcon(type: MemoryType) {
  switch (type) {
    case "style_preference":
      return Palette;
    case "negative_preference":
      return XCircle;
    case "brand_rule":
      return Shield;
    case "workflow_habit":
      return Workflow;
    case "prompt_pattern":
      return FileText;
    case "review_history":
      return Clock;
    default:
      return Brain;
  }
}

function toggleType(type: MemoryType) {
  if (expandedTypes.value.has(type)) {
    expandedTypes.value.delete(type);
  } else {
    expandedTypes.value.add(type);
  }
}

async function handleAddMemory() {
  if (!newSummary.value.trim() || !newContent.value.trim()) return;
  try {
    const content = JSON.parse(newContent.value);
    await saveMemory(newMemoryType.value, content, newSummary.value.trim());
    showAddForm.value = false;
    newSummary.value = "";
    newContent.value = "";
  } catch {
    // JSON 解析失败，提示用户
    alert("记忆内容必须是合法的 JSON 格式");
  }
}

function confidenceLabel(confidence: number): string {
  if (confidence >= 0.8) return "高置信";
  if (confidence >= 0.5) return "中置信";
  return "低置信";
}

function confidenceColor(confidence: number): string {
  if (confidence >= 0.8) return "var(--color-success)";
  if (confidence >= 0.5) return "var(--color-warning)";
  return "var(--color-text-disabled)";
}
</script>

<template>
  <div class="creative-memory-panel">
    <!-- 头部 -->
    <div class="creative-memory-panel__header">
      <div class="creative-memory-panel__title">
        <Brain :size="16" />
        <span>创意记忆</span>
        <span class="creative-memory-panel__count">{{ activeMemories.length }}</span>
      </div>
      <div class="creative-memory-panel__actions">
        <button
          type="button"
          class="creative-memory-panel__refresh"
          :disabled="loading"
          @click="loadMemories()"
        >
          刷新
        </button>
        <button
          type="button"
          class="creative-memory-panel__add"
          @click="showAddForm = !showAddForm"
        >
          <Plus :size="12" />
          <span>添加记忆</span>
        </button>
      </div>
    </div>

    <!-- 添加记忆表单 -->
    <Transition name="slide">
      <div v-if="showAddForm" class="creative-memory-panel__form">
        <div class="form-row">
          <label class="form-label">类型</label>
          <select v-model="newMemoryType" class="form-select">
            <option v-for="type in memoryTypes" :key="type" :value="type">
              {{ memoryTypeLabel(type) }}
            </option>
          </select>
        </div>
        <div class="form-row">
          <label class="form-label">摘要</label>
          <input
            v-model="newSummary"
            class="form-input"
            placeholder="简短描述这条记忆..."
            maxlength="500"
          />
        </div>
        <div class="form-row">
          <label class="form-label">内容（JSON）</label>
          <textarea
            v-model="newContent"
            class="form-textarea"
            placeholder='{"color_palette": ["warm", "muted"], "lighting": "natural"}'
            rows="4"
          />
        </div>
        <div class="form-actions">
          <button type="button" class="form-cancel" @click="showAddForm = false">取消</button>
          <button
            type="button"
            class="form-save"
            :disabled="saving || !newSummary.trim() || !newContent.trim()"
            @click="handleAddMemory"
          >
            {{ saving ? "保存中..." : "保存" }}
          </button>
        </div>
      </div>
    </Transition>

    <!-- 错误提示 -->
    <div v-if="error" class="creative-memory-panel__error">
      <XCircle :size="14" />
      <span>{{ error }}</span>
    </div>

    <!-- 加载状态 -->
    <div v-if="loading && memories.length === 0" class="creative-memory-panel__loading">
      <div class="loading-spinner" />
      <span>加载创意记忆...</span>
    </div>

    <!-- 空状态 -->
    <div v-else-if="memories.length === 0 && !showAddForm" class="creative-memory-panel__empty">
      <Brain :size="32" />
      <p>暂无创意记忆</p>
      <p class="empty-hint">添加您的风格偏好、不喜欢的风格或品牌规范，让系统越用越懂您。</p>
    </div>

    <!-- 按类型分组展示 -->
    <div v-else class="creative-memory-panel__groups">
      <div
        v-for="type in memoryTypes"
        v-show="(memoriesByType[type] ?? []).length > 0"
        :key="type"
        class="memory-group"
      >
        <button type="button" class="memory-group__header" @click="toggleType(type)">
          <component :is="typeIcon(type)" :size="14" />
          <span class="memory-group__label">{{ memoryTypeLabel(type) }}</span>
          <span class="memory-group__count">{{ (memoriesByType[type] ?? []).length }}</span>
          <ChevronDown v-if="expandedTypes.has(type)" :size="14" class="memory-group__chevron" />
          <ChevronRight v-else :size="14" class="memory-group__chevron" />
        </button>

        <Transition name="slide">
          <div v-if="expandedTypes.has(type)" class="memory-group__list">
            <div v-for="memory in memoriesByType[type] ?? []" :key="memory.id" class="memory-item">
              <div class="memory-item__header">
                <span class="memory-item__summary">{{ memory.summary }}</span>
                <div class="memory-item__badges">
                  <span
                    class="memory-item__confidence"
                    :style="{ color: confidenceColor(memory.confidence) }"
                    :title="`置信度: ${(memory.confidence * 100).toFixed(0)}%`"
                  >
                    <Star :size="10" />
                    {{ confidenceLabel(memory.confidence) }}
                  </span>
                  <span
                    class="memory-item__status"
                    :class="`memory-item__status--${memory.status}`"
                  >
                    {{ memoryStatusLabel(memory.status) }}
                  </span>
                </div>
              </div>

              <div class="memory-item__content">
                <pre class="memory-item__json">{{ memory.contentJson }}</pre>
              </div>

              <div class="memory-item__actions">
                <button
                  type="button"
                  class="memory-action"
                  title="确认（增加置信度）"
                  @click="confirmMemoryById(memory.id)"
                >
                  <CheckCircle :size="12" />
                </button>
                <button
                  v-if="memory.status === 'active'"
                  type="button"
                  class="memory-action"
                  title="暂停"
                  @click="pauseMemory(memory.id)"
                >
                  <Pause :size="12" />
                </button>
                <button
                  v-else-if="memory.status === 'paused'"
                  type="button"
                  class="memory-action"
                  title="恢复"
                  @click="resumeMemory(memory.id)"
                >
                  <Play :size="12" />
                </button>
                <button
                  type="button"
                  class="memory-action memory-action--danger"
                  title="删除"
                  @click="removeMemory(memory.id)"
                >
                  <Trash2 :size="12" />
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>
  </div>
</template>

<style scoped>
.creative-memory-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}

.creative-memory-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.creative-memory-panel__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.creative-memory-panel__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 50%;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 10px;
  font-weight: 600;
}

.creative-memory-panel__actions {
  display: flex;
  gap: 6px;
}

.creative-memory-panel__refresh,
.creative-memory-panel__add {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
}

.creative-memory-panel__add {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

/* 表单 */
.creative-memory-panel__form {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.form-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.form-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.form-input,
.form-select,
.form-textarea {
  padding: 6px 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  color: var(--color-text);
  font-size: 12px;
  font-family: inherit;
}

.form-textarea {
  font-family: var(--font-mono);
  font-size: 11px;
  resize: vertical;
}

.form-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.form-cancel,
.form-save {
  padding: 4px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  font-size: 11px;
  cursor: pointer;
}

.form-cancel {
  background: transparent;
  color: var(--color-text-secondary);
}

.form-save {
  background: var(--color-accent);
  color: var(--color-on-accent);
  border-color: var(--color-accent);
}

.form-save:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 错误/加载/空状态 */
.creative-memory-panel__error {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  background: var(--color-danger-soft);
  color: var(--color-danger);
  font-size: 12px;
}

.creative-memory-panel__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 20px;
  font-size: 12px;
  color: var(--color-text-tertiary);
}

.creative-memory-panel__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 20px;
  text-align: center;
  color: var(--color-text-tertiary);
}

.empty-hint {
  font-size: 11px;
  color: var(--color-text-disabled);
  max-width: 240px;
}

/* 记忆分组 */
.creative-memory-panel__groups {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.memory-group__header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.memory-group__header:hover {
  background: var(--color-surface-hover);
}

.memory-group__label {
  flex: 1;
}

.memory-group__count {
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.memory-group__chevron {
  color: var(--color-text-disabled);
}

.memory-group__list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 20px;
}

/* 记忆条目 */
.memory-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.memory-item__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.memory-item__summary {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text);
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.memory-item__badges {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.memory-item__confidence {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: 10px;
  font-weight: 500;
}

.memory-item__status {
  font-size: 10px;
  font-weight: 500;
  padding: 1px 4px;
  border-radius: var(--radius-control);
}

.memory-item__status--active {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.memory-item__status--paused {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.memory-item__status--archived {
  background: var(--color-surface-subtle);
  color: var(--color-text-disabled);
}

.memory-item__content {
  max-height: 60px;
  overflow: hidden;
}

.memory-item__json {
  margin: 0;
  padding: 4px 6px;
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--color-text-secondary);
  white-space: pre-wrap;
  word-break: break-all;
  overflow: hidden;
}

.memory-item__actions {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
}

.memory-action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.memory-action:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.memory-action--danger:hover {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

/* 过渡动画 */
.slide-enter-active,
.slide-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-height var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
  overflow: hidden;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
  transform: translateY(-8px);
}

.slide-enter-to,
.slide-leave-from {
  opacity: 1;
  max-height: 400px;
  transform: translateY(0);
}

.loading-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--color-border-subtle);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
