<script setup lang="ts">
/**
 * ProjectListView：漫剧项目列表。
 *
 * 设计理念：
 * - 空状态：居中欢迎页，展示"新建项目"和"图片"两个入口卡片
 * - 有项目时：顶部"+"按钮 + 项目列表，支持切换项目
 * - 支持"空白项目"快速入口，跳过项目创建直接使用创意工坊
 */
import { FolderOpen, Image as ImageIcon, Plus, Sparkles, Trash2 } from "@lucide/vue";

import type { MangaProject } from "../../../bridge/manga";

defineProps<{
  projects: MangaProject[];
  loading: boolean;
}>();

const emit = defineEmits<{
  create: [];
  "create-blank": []; // 空白项目：跳过项目创建，直接使用创意工坊
  select: [project: MangaProject];
  delete: [id: string];
}>();

function statusLabel(status: string): string {
  switch (status) {
    case "draft":
      return "草稿";
    case "in-progress":
      return "进行中";
    case "completed":
      return "已完成";
    case "archived":
      return "已归档";
    default:
      return status;
  }
}

function selectProject(project: MangaProject): void {
  emit("select", project);
}

function onProjectCardKeydown(event: KeyboardEvent, project: MangaProject): void {
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  selectProject(project);
}
</script>

<template>
  <div class="project-list">
    <!-- 空状态：居中欢迎页 -->
    <div v-if="!loading && projects.length === 0" class="project-list__welcome">
      <!-- 顶部标签栏 -->
      <div class="welcome-tabs">
        <button type="button" class="welcome-tab welcome-tab--active" @click="emit('create')">
          <Plus :size="14" />
          <span>新建项目</span>
        </button>
        <button type="button" class="welcome-tab" @click="emit('create-blank')">
          <ImageIcon :size="14" />
          <span>图片</span>
        </button>
      </div>

      <!-- 居中内容区 -->
      <div class="welcome-content">
        <h2 class="welcome-title">开始你的第一个项目</h2>
        <p class="welcome-desc">输入一句话，AI 帮你创作漫剧、短视频或宣传片</p>
        <button type="button" class="welcome-create-btn" @click="emit('create')">
          <Plus :size="16" />
          <span>新建项目</span>
        </button>
      </div>

      <!-- 底部提示 -->
      <div class="welcome-footer">
        <button type="button" class="welcome-blank-btn" @click="emit('create-blank')">
          <Sparkles :size="14" />
          <span>或直接使用空白项目</span>
        </button>
      </div>
    </div>

    <!-- 有项目时：顶部按钮 + 项目列表 -->
    <div v-else class="project-list__content">
      <div class="project-list__header">
        <h2 class="project-list__title">
          <Sparkles :size="14" />
          <span>我的项目</span>
          <span class="project-list__count">{{ projects.length }}</span>
        </h2>
        <div class="project-list__header-actions">
          <button type="button" class="project-list__blank-btn" @click="emit('create-blank')">
            空白项目
          </button>
          <button type="button" class="project-list__new-btn" @click="emit('create')">
            <Plus :size="14" />
          </button>
        </div>
      </div>

      <div v-if="loading" class="project-list__loading">
        <div class="project-list__loading-spinner" />
        <span>加载项目...</span>
      </div>

      <div v-else class="project-list__grid">
        <!-- 现有项目卡片 -->
        <div
          v-for="project in projects"
          :key="project.id"
          class="project-card"
          role="button"
          tabindex="0"
          @click="selectProject(project)"
          @keydown="onProjectCardKeydown($event, project)"
        >
          <div class="project-card__icon">
            <FolderOpen :size="18" />
          </div>
          <div class="project-card__info">
            <span class="project-card__title">{{ project.title }}</span>
            <span v-if="project.theme" class="project-card__theme">{{ project.theme }}</span>
          </div>
          <span class="project-card__status">{{ statusLabel(project.status) }}</span>
          <button
            type="button"
            class="project-card__delete"
            title="删除项目"
            @click.stop="emit('delete', project.id)"
          >
            <Trash2 :size="12" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.project-list {
  width: 100%;
  max-width: 720px;
}

/* —— 空状态欢迎页 —— */
.project-list__welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 0;
}

/* 顶部标签栏 */
.welcome-tabs {
  display: flex;
  gap: 2px;
  padding: 4px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  margin-bottom: 40px;
}

.welcome-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 20px;
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.welcome-tab:hover {
  color: var(--color-text);
}

.welcome-tab--active {
  background: var(--color-surface);
  color: var(--color-text);
  box-shadow: var(--shadow-sm);
}

/* 居中内容区 */
.welcome-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 32px;
}

.welcome-title {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  color: var(--color-text);
  letter-spacing: -0.01em;
}

.welcome-desc {
  margin: 0 0 24px;
  font-size: 14px;
  color: var(--color-text-secondary);
  max-width: 320px;
  line-height: 1.5;
}

.welcome-create-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 12px 28px;
  border: none;
  border-radius: var(--radius-pill);
  background: linear-gradient(135deg, var(--color-accent) 0%, var(--aurora-cyan, #3b82f6) 100%);
  color: #ffffff;
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 4px 16px rgba(59, 130, 246, 0.3);
  transition: all var(--duration-fast) var(--ease-out);
}

.welcome-create-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 6px 20px rgba(59, 130, 246, 0.4);
}

.welcome-create-btn:active {
  transform: translateY(0);
}

/* 底部提示 */
.welcome-footer {
  display: flex;
  align-items: center;
  justify-content: center;
}

.welcome-blank-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-tertiary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.welcome-blank-btn:hover {
  border-color: var(--color-border);
  color: var(--color-text-secondary);
  background: var(--color-surface-subtle);
}

/* —— 有项目时的内容区 —— */
.project-list__content {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.project-list__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.project-list__title {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.project-list__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 20px;
  height: 20px;
  padding: 0 5px;
  border-radius: 50%;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 11px;
  font-weight: 600;
}

.project-list__header-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.project-list__blank-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 12px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.project-list__blank-btn:hover {
  border-color: var(--color-border);
  background: var(--color-surface-subtle);
}

.project-list__new-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.project-list__new-btn:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
  background: var(--color-accent-soft);
}

/* —— 加载状态 —— */
.project-list__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 32px;
  font-size: 13px;
  color: var(--color-text-tertiary);
}

.project-list__loading-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--color-border-subtle);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

/* —— 项目卡片网格 —— */
.project-list__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 10px;
}

.project-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 16px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  cursor: pointer;
  text-align: left;
  transition: all var(--duration-fast) var(--ease-out);
}

.project-card:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}

.project-card__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  color: var(--color-accent);
  flex-shrink: 0;
}

.project-card__info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.project-card__title {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-card__theme {
  font-size: 12px;
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-card__status {
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
  font-size: 11px;
  font-weight: 500;
  flex-shrink: 0;
}

.project-card__delete {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  opacity: 0;
  transition: all var(--duration-fast) var(--ease-out);
}

.project-card:hover .project-card__delete {
  opacity: 1;
}

.project-card__delete:hover {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* —— 移动端适配 —— */
@media (max-width: 640px) {
  .project-list {
    max-width: 100%;
  }

  .welcome-tabs {
    margin-bottom: 30px;
  }

  .welcome-tab {
    padding: 6px 16px;
    font-size: 12px;
  }

  .welcome-title {
    font-size: 18px;
  }

  .project-list__grid {
    grid-template-columns: 1fr;
  }

  .project-card {
    padding: 12px 14px;
  }

  .project-card__icon {
    width: 32px;
    height: 32px;
  }

  .project-card__title {
    font-size: 13px;
  }
}
</style>
