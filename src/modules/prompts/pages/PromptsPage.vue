<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { AlertCircle, BookOpenText, Pencil, Plus, Search, Trash2 } from "@lucide/vue";

import EmptyState from "../../../shared/ui/EmptyState.vue";
import ModalDialog from "../../../shared/ui/ModalDialog.vue";

/**
 * 提示词库页面（主导航"内容与知识"分组，资源库下方）。
 *
 * 当前为前端最小实装：数据用 localStorage 持久化，
 * 内置几条教学场景的示例提示词，支持新建 / 修改 / 删除 / 分类筛选 / 关键词搜索。
 * 后续接入后端 Prompt 实体后，可将数据层替换为 IPC 调用，UI 结构保持不变。
 */

type PromptCategory = "image" | "video" | "text" | "design" | "other";

interface PromptItem {
  id: string;
  title: string;
  category: PromptCategory;
  tags: string[];
  content: string;
  updatedAt: string;
}

const STORAGE_KEY = "aigc-studio.prompts";

const categoryLabels: Record<PromptCategory, string> = {
  image: "图像生成",
  video: "视频生成",
  text: "文本创作",
  design: "设计素材",
  other: "其他",
};

const categoryOptions: { value: PromptCategory | "all"; label: string }[] = [
  { value: "all", label: "全部分类" },
  { value: "image", label: "图像生成" },
  { value: "video", label: "视频生成" },
  { value: "text", label: "文本创作" },
  { value: "design", label: "设计素材" },
  { value: "other", label: "其他" },
];

const categoryEditOptions: { value: PromptCategory; label: string }[] = [
  { value: "image", label: "图像生成" },
  { value: "video", label: "视频生成" },
  { value: "text", label: "文本创作" },
  { value: "design", label: "设计素材" },
  { value: "other", label: "其他" },
];

/** 初始示例数据：覆盖常见教学创作场景。 */
const seedPrompts: PromptItem[] = [
  {
    id: "prompt-portrait-realistic",
    title: "写实人物肖像",
    category: "image",
    tags: ["人物", "写实", "肖像"],
    content:
      "生成一张写实风格的半身人物肖像，柔和的自然光，浅景深，皮肤质感细腻，眼神有故事感，4K 高清。",
    updatedAt: new Date("2026-07-10T09:00:00").toISOString(),
  },
  {
    id: "prompt-landscape-watercolor",
    title: "水彩山水风景",
    category: "image",
    tags: ["风景", "水彩", "传统"],
    content: "用水彩风格绘制江南山水，远山如黛，近水含烟，留白意境，带有中国传统画的气韵。",
    updatedAt: new Date("2026-07-11T14:30:00").toISOString(),
  },
  {
    id: "prompt-courseware-cover",
    title: "课件封面设计",
    category: "design",
    tags: ["课件", "封面", "教育"],
    content:
      "为高中语文课件《赤壁赋》设计封面，主色调使用墨蓝与宣纸白，配以苏轼人物剪影和浪花元素，风格典雅。",
    updatedAt: new Date("2026-07-12T10:15:00").toISOString(),
  },
  {
    id: "prompt-video-explain-script",
    title: "课堂讲解视频脚本",
    category: "video",
    tags: ["脚本", "讲解", "语文"],
    content:
      "为高中语文《赤壁赋》生成 3 分钟课堂讲解视频脚本，包含开篇引入、原文朗读、意境赏析、总结升华四个段落。",
    updatedAt: new Date("2026-07-13T16:45:00").toISOString(),
  },
  {
    id: "prompt-text-essay-feedback",
    title: "作文点评模板",
    category: "text",
    tags: ["作文", "点评", "模板"],
    content:
      "请以高中语文教师视角点评以下学生作文，从立意、结构、语言、文采四个维度给出具体建议，总分 100 分。",
    updatedAt: new Date("2026-07-14T11:20:00").toISOString(),
  },
];

function loadPrompts(): PromptItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return seedPrompts.slice();
    const parsed = JSON.parse(raw) as PromptItem[];
    if (!Array.isArray(parsed)) return seedPrompts.slice();
    return parsed;
  } catch {
    return seedPrompts.slice();
  }
}

function persistPrompts(items: PromptItem[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
  } catch {
    // localStorage 不可用时静默降级为内存态。
  }
}

const prompts = ref<PromptItem[]>(loadPrompts());

watch(prompts, (value) => persistPrompts(value), { deep: true });

// —— 筛选与搜索 ——
const searchKeyword = ref("");
const categoryFilter = ref<PromptCategory | "all">("all");

const filteredPrompts = computed(() => {
  const keyword = searchKeyword.value.trim().toLowerCase();
  return prompts.value
    .filter((item) => categoryFilter.value === "all" || item.category === categoryFilter.value)
    .filter((item) => {
      if (!keyword) return true;
      return (
        item.title.toLowerCase().includes(keyword) ||
        item.content.toLowerCase().includes(keyword) ||
        item.tags.some((tag) => tag.toLowerCase().includes(keyword))
      );
    })
    .slice()
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
});

const categoryCounts = computed(() => {
  const counts: Record<PromptCategory, number> = {
    image: 0,
    video: 0,
    text: 0,
    design: 0,
    other: 0,
  };
  for (const item of prompts.value) {
    counts[item.category] += 1;
  }
  return counts;
});

// —— 新建 / 修改对话框 ——
const editingPrompt = ref<PromptItem | null>(null);
const isCreating = ref(false);
const editForm = ref({
  title: "",
  category: "image" as PromptCategory,
  tagsText: "",
  content: "",
});
const editError = ref("");

function resetForm(): void {
  editForm.value = {
    title: "",
    category: "image",
    tagsText: "",
    content: "",
  };
  editError.value = "";
}

function openCreate(): void {
  editingPrompt.value = null;
  isCreating.value = true;
  resetForm();
}

function openEdit(prompt: PromptItem): void {
  editingPrompt.value = prompt;
  isCreating.value = false;
  editForm.value = {
    title: prompt.title,
    category: prompt.category,
    tagsText: prompt.tags.join(", "),
    content: prompt.content,
  };
  editError.value = "";
}

function closeEditor(): void {
  editingPrompt.value = null;
  isCreating.value = false;
  resetForm();
}

function parseTags(text: string): string[] {
  return text
    .split(/[,，]/)
    .map((tag) => tag.trim())
    .filter((tag) => tag.length > 0)
    .slice(0, 8);
}

function savePrompt(): void {
  const title = editForm.value.title.trim();
  if (!title) {
    editError.value = "标题不能为空。";
    return;
  }
  const content = editForm.value.content.trim();
  if (!content) {
    editError.value = "提示词内容不能为空。";
    return;
  }
  const tags = parseTags(editForm.value.tagsText);

  if (isCreating.value) {
    const newPrompt: PromptItem = {
      id: `prompt-${Date.now()}`,
      title,
      category: editForm.value.category,
      tags,
      content,
      updatedAt: new Date().toISOString(),
    };
    prompts.value.push(newPrompt);
  } else {
    const target = editingPrompt.value;
    if (!target) {
      editError.value = "提示词不存在或已被删除。";
      return;
    }
    const index = prompts.value.findIndex((item) => item.id === target.id);
    if (index === -1) {
      editError.value = "提示词不存在或已被删除。";
      return;
    }
    prompts.value[index] = {
      ...target,
      title,
      category: editForm.value.category,
      tags,
      content,
      updatedAt: new Date().toISOString(),
    };
  }
  closeEditor();
}

// —— 删除确认 ——
const deletingPrompt = ref<PromptItem | null>(null);

function openDelete(prompt: PromptItem): void {
  deletingPrompt.value = prompt;
}

function closeDelete(): void {
  deletingPrompt.value = null;
}

function confirmDelete(): void {
  const target = deletingPrompt.value;
  if (!target) return;
  prompts.value = prompts.value.filter((item) => item.id !== target.id);
  closeDelete();
}

// —— 复制内容到剪贴板 ——
const copiedId = ref<string | null>(null);

async function copyContent(prompt: PromptItem): Promise<void> {
  try {
    await navigator.clipboard.writeText(prompt.content);
    copiedId.value = prompt.id;
    setTimeout(() => {
      if (copiedId.value === prompt.id) copiedId.value = null;
    }, 1500);
  } catch {
    // 剪贴板 API 不可用时静默失败。
  }
}

function formatDate(iso: string): string {
  try {
    const date = new Date(iso);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(
      date.getDate(),
    ).padStart(2, "0")}`;
  } catch {
    return iso;
  }
}

const hasPrompts = computed(() => prompts.value.length > 0);
const editorOpen = computed(() => editingPrompt.value !== null || isCreating.value);
</script>

<template>
  <div class="prompts-page">
    <header class="page-toolbar">
      <div class="toolbar-info">
        <h2>提示词库</h2>
        <p>
          {{ prompts.length }} 条提示词 · 共
          {{
            categoryCounts.image +
            categoryCounts.video +
            categoryCounts.text +
            categoryCounts.design +
            categoryCounts.other
          }}
          条记录
        </p>
      </div>
      <div class="toolbar-actions">
        <button class="primary-button" type="button" @click="openCreate">
          <Plus :size="16" />
          <span>新建提示词</span>
        </button>
      </div>
    </header>

    <div class="filter-bar">
      <form class="search-form" role="search" @submit.prevent>
        <Search :size="16" aria-hidden="true" />
        <input
          v-model="searchKeyword"
          aria-label="搜索提示词"
          maxlength="80"
          placeholder="搜索标题、内容或标签"
          type="search"
        />
      </form>
      <div class="category-tabs" role="tablist">
        <button
          v-for="opt in categoryOptions"
          :key="opt.value"
          class="category-tab"
          role="tab"
          :aria-selected="categoryFilter === opt.value"
          :class="{ 'is-active': categoryFilter === opt.value }"
          type="button"
          @click="categoryFilter = opt.value"
        >
          {{ opt.label }}
          <span v-if="opt.value !== 'all'" class="count">
            {{ categoryCounts[opt.value as PromptCategory] }}
          </span>
        </button>
      </div>
    </div>

    <EmptyState
      v-if="!hasPrompts"
      description="新建第一条提示词后，可在创意工坊中快速复用。"
      :icon="BookOpenText"
      title="暂无提示词"
    >
      <button class="primary-button" type="button" @click="openCreate">
        <Plus :size="16" />
        <span>新建提示词</span>
      </button>
    </EmptyState>

    <EmptyState
      v-else-if="filteredPrompts.length === 0"
      description="尝试更换关键词或切换分类。"
      :icon="Search"
      title="未找到匹配的提示词"
    />

    <div v-else class="prompts-grid">
      <article v-for="prompt in filteredPrompts" :key="prompt.id" class="prompt-card">
        <header class="card-header">
          <div class="card-title-row">
            <h3 class="card-title">{{ prompt.title }}</h3>
            <span class="category-tag" :data-category="prompt.category">
              {{ categoryLabels[prompt.category] }}
            </span>
          </div>
          <div class="card-actions">
            <button
              class="icon-button"
              type="button"
              title="修改"
              :aria-label="`修改提示词 ${prompt.title}`"
              @click="openEdit(prompt)"
            >
              <Pencil :size="14" />
            </button>
            <button
              class="icon-button icon-button--danger"
              type="button"
              title="删除"
              :aria-label="`删除提示词 ${prompt.title}`"
              @click="openDelete(prompt)"
            >
              <Trash2 :size="14" />
            </button>
          </div>
        </header>
        <p class="card-content">{{ prompt.content }}</p>
        <footer class="card-footer">
          <div class="tag-list">
            <span v-for="tag in prompt.tags" :key="tag" class="tag">#{{ tag }}</span>
          </div>
          <div class="card-meta">
            <span class="updated">{{ formatDate(prompt.updatedAt) }}</span>
            <button
              class="copy-button"
              type="button"
              :title="copiedId === prompt.id ? '已复制' : '复制内容'"
              @click="copyContent(prompt)"
            >
              {{ copiedId === prompt.id ? "已复制" : "复制" }}
            </button>
          </div>
        </footer>
      </article>
    </div>

    <!-- 新建 / 修改对话框 -->
    <ModalDialog
      :open="editorOpen"
      :title="isCreating ? '新建提示词' : '修改提示词'"
      :description="
        isCreating ? '填写提示词信息后保存' : `编辑「${editingPrompt?.title ?? ''}」的信息`
      "
      @close="closeEditor"
    >
      <form class="edit-form" @submit.prevent="savePrompt">
        <label class="field">
          <span class="field-label">标题</span>
          <input
            v-model="editForm.title"
            type="text"
            maxlength="60"
            placeholder="如：写实人物肖像"
            aria-label="标题"
          />
        </label>
        <label class="field">
          <span class="field-label">分类</span>
          <select v-model="editForm.category" aria-label="分类">
            <option v-for="opt in categoryEditOptions" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </option>
          </select>
        </label>
        <label class="field">
          <span class="field-label">标签（用逗号分隔，最多 8 个）</span>
          <input
            v-model="editForm.tagsText"
            type="text"
            maxlength="120"
            placeholder="如：人物, 写实, 肖像"
            aria-label="标签"
          />
        </label>
        <label class="field">
          <span class="field-label">提示词内容</span>
          <textarea
            v-model="editForm.content"
            rows="6"
            maxlength="2000"
            placeholder="完整的提示词内容，可包含变量占位符 {{variable}}。"
            aria-label="提示词内容"
          ></textarea>
        </label>
        <p v-if="editError" class="dialog-error" role="alert">
          <AlertCircle :size="16" />
          <span>{{ editError }}</span>
        </p>
      </form>
      <template #footer>
        <button class="secondary-button" type="button" @click="closeEditor">取消</button>
        <button class="primary-button" type="button" @click="savePrompt">保存</button>
      </template>
    </ModalDialog>

    <!-- 删除确认对话框 -->
    <ModalDialog
      :open="deletingPrompt !== null"
      title="删除提示词"
      description="删除后无法恢复，关联的引用将断开。"
      @close="closeDelete"
    >
      <p class="confirm-text">确认删除提示词「{{ deletingPrompt?.title }}」吗？</p>
      <template #footer>
        <button class="secondary-button" type="button" @click="closeDelete">取消</button>
        <button class="danger-button" type="button" @click="confirmDelete">删除</button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.prompts-page {
  width: 100%;
  max-width: 1440px;
  min-height: 100%;
}

.page-toolbar {
  display: flex;
  padding: var(--space-3) 0;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.toolbar-info h2 {
  margin: 0;
  font-size: var(--text-headline);
  font-weight: 600;
}

.toolbar-info p {
  margin: 2px 0 0;
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
}

.primary-button,
.secondary-button,
.danger-button {
  display: inline-flex;
  height: var(--control-height);
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-1);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  font-size: var(--text-subhead);
  font-weight: 500;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}

.primary-button {
  color: var(--color-on-accent);
  background: var(--color-accent);
}

.primary-button:hover {
  background: var(--color-accent-hover);
}

.secondary-button {
  color: var(--color-text);
  border-color: var(--color-border-subtle);
  background: var(--color-surface);
}

.secondary-button:hover {
  background: var(--color-surface-hover);
}

.danger-button {
  color: var(--color-on-accent);
  background: var(--color-danger, #d9342b);
}

.danger-button:hover {
  filter: brightness(0.92);
}

.filter-bar {
  display: flex;
  padding: var(--space-2) 0 var(--space-3);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.search-form {
  display: flex;
  flex: 1 1 240px;
  min-width: 200px;
  max-width: 420px;
  height: var(--control-height);
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-2);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text-secondary);
}

.search-form input {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  color: var(--color-text);
  font-size: var(--text-subhead);
}

.search-form input:focus {
  outline: none;
}

.category-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  padding: 2px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
}

.category-tab {
  display: inline-flex;
  height: calc(var(--control-height) - 6px);
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-1);
  border: none;
  border-radius: calc(var(--radius-control) - 2px);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  font-weight: 500;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.category-tab:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.category-tab.is-active {
  color: var(--color-on-accent);
  background: var(--color-accent);
}

.category-tab .count {
  display: inline-flex;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-pill);
  background: rgba(0, 0, 0, 0.08);
  font-size: 10px;
  font-weight: 600;
}

.category-tab.is-active .count {
  background: rgba(255, 255, 255, 0.24);
}

.prompts-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: var(--space-3);
}

.prompt-card {
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.prompt-card:hover {
  border-color: var(--color-accent);
  box-shadow: var(--shadow-sm);
}

.card-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-2);
}

.card-title-row {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: var(--space-1);
}

.card-title {
  margin: 0;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-subhead);
  font-weight: 600;
  line-height: 20px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.category-tag {
  display: inline-flex;
  width: fit-content;
  height: 18px;
  padding: 0 var(--space-2);
  align-items: center;
  border-radius: var(--radius-pill);
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: var(--text-caption);
  font-weight: 500;
}

.category-tag[data-category="video"] {
  background: rgba(120, 90, 200, 0.12);
  color: rgb(120, 90, 200);
}

.category-tag[data-category="text"] {
  background: rgba(60, 130, 80, 0.12);
  color: rgb(60, 130, 80);
}

.category-tag[data-category="design"] {
  background: rgba(200, 130, 60, 0.12);
  color: rgb(200, 130, 60);
}

.category-tag[data-category="other"] {
  background: var(--color-surface-hover);
  color: var(--color-text-secondary);
}

.card-actions {
  display: flex;
  gap: 2px;
  flex: 0 0 auto;
}

.icon-button {
  display: inline-grid;
  width: 28px;
  height: 28px;
  place-items: center;
  color: var(--color-text-secondary);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.icon-button:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.icon-button--danger:hover {
  color: var(--color-danger, #d9342b);
}

.card-content {
  margin: var(--space-2) 0;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 4;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-footnote);
  line-height: 18px;
  text-overflow: ellipsis;
}

.card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-top: auto;
  padding-top: var(--space-2);
  border-top: 1px solid var(--color-border-subtle);
}

.tag-list {
  display: flex;
  flex: 1;
  flex-wrap: wrap;
  gap: 4px;
  min-width: 0;
}

.tag {
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
}

.card-meta {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 0 0 auto;
}

.updated {
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
}

.copy-button {
  height: 22px;
  padding: 0 var(--space-2);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.copy-button:hover {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.edit-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field-label {
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: 500;
}

.field input,
.field select,
.field textarea {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: var(--text-subhead);
  font-family: inherit;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.field input:focus,
.field select:focus,
.field textarea:focus {
  outline: none;
  border-color: var(--color-accent);
}

.field textarea {
  resize: vertical;
  min-height: 120px;
  font-family: var(--font-mono, inherit);
  line-height: 1.5;
}

.dialog-error {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--color-danger, #d9342b);
  font-size: var(--text-footnote);
}

.confirm-text {
  margin: 0;
  color: var(--color-text);
}

@media (max-width: 720px) {
  .filter-bar {
    flex-direction: column;
    align-items: stretch;
  }

  .search-form {
    max-width: none;
  }

  .category-tabs {
    overflow-x: auto;
    flex-wrap: nowrap;
  }

  .prompts-grid {
    grid-template-columns: 1fr;
  }
}
</style>
