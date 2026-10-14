<script setup lang="ts">
/**
 * TasksPage：「任务」页——历史对话的集中查看入口。
 *
 * 口径（对齐产品设计）：
 * - 左列：全部历史对话（按工作目录分组）+ 搜索 / 目录筛选
 * - 右列：所选对话的**只读回放**，阅读时停留在本页，不跳回创意工坊
 * - 只有点「继续对话」才回到 /generations 接着聊
 * - 页面明确说明历史保存范围，避免用户误以为已删除内容可以找回
 *
 * 管理口径（2026-10-14 起）：任务/空间中的历史对话只读 —— 可回放、可
 * 「继续对话」追加新内容，但**不提供删除 / 清空消息 / 重命名**。历史是
 * 日志而非工作区，删除路径收在别处，本页避免误触不可逆操作。
 */
import { computed, onMounted, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import {
  ChevronRight,
  Folder,
  Info,
  LoaderCircle,
  MessageSquareText,
  Play,
  Plus,
  RefreshCcw,
  Search,
  X,
} from "@lucide/vue";

import {
  listInvocations,
  listMessages,
  type ConversationRecord,
  type MessageRecord,
  type ToolInvocationRecord,
} from "../../../bridge/agent";
import EmptyState from "../../../shared/ui/EmptyState.vue";
import { useConversationWorkspaces } from "../../../app/composables/useConversationWorkspaces";
import { useConversationList } from "../../../app/composables/useConversationList";
import { useShellActionsStore } from "../../../app/stores/shellActions";
import { formatDateTime, formatRelativeTime } from "../../../shared/utils/time";
import {
  isVideoUrl,
  parseUserMessageContent,
  renderRichContent,
} from "../../../shared/utils/richText";

const router = useRouter();
const conversationWorkspaces = useConversationWorkspaces();
const shellActions = useShellActionsStore();

/** 默认分组名：未登记工作目录的对话归入应用自身。 */
const DEFAULT_GROUP = "OPEN AIGC";
/** 目录筛选的「全部」哨兵值。 */
const ALL_WORKSPACES = "__all__";

const TOOL_LABELS: Record<string, string> = {
  image_generation: "图片生成",
  video_generation: "视频生成",
  list_credentials: "查看凭据",
  current_time: "当前时间",
  analyze_reference_image: "分析参考图",
  canvas_search: "画布检索",
  canvas_add_note: "画布便签",
  canvas_connect: "画布连线",
};

/* ──────────────────────────────────────────────
 *  历史对话列表
 * ────────────────────────────────────────────── */

/**
 * 会话列表来自共享单例（与侧栏同一份），本页的增删改会即时反映到侧栏，
 * 侧栏刷新也会即时反映到本页 —— 不再各持一份、各刷各的。
 */
const { conversations, refresh: refreshConversationList } = useConversationList();
const listPhase = ref<"idle" | "loading" | "ready" | "error">("idle");
const listError = ref<string | null>(null);

const keyword = ref("");
const workspaceFilter = ref<string>(ALL_WORKSPACES);

function groupLabel(conversation: ConversationRecord): string {
  return conversationWorkspaces.getWorkspaceName(conversation.id) ?? DEFAULT_GROUP;
}

/** 是否为「未归属任何工作目录」的对话（而非恰好叫该名字的真实空间）。 */
function isLooseTask(conversation: ConversationRecord): boolean {
  return conversationWorkspaces.getWorkspaceName(conversation.id) === null;
}

const workspaceOptions = computed<string[]>(() => {
  const set = new Set<string>();
  // 只要有未分组任务，就提供「OPEN AIGC」选项，否则按空间筛选后
  // 这一组会整体消失、连选项都没有。
  if (conversations.value.some(isLooseTask)) set.add(DEFAULT_GROUP);
  for (const item of conversations.value) set.add(groupLabel(item));
  return [ALL_WORKSPACES, ...[...set].sort((a, b) => a.localeCompare(b))];
});

/** 经过搜索 / 目录筛选后、按工作目录分组的对话。 */
const groups = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  const filtered = conversations.value.filter((item) => {
    if (workspaceFilter.value !== ALL_WORKSPACES && groupLabel(item) !== workspaceFilter.value) {
      return false;
    }
    if (kw && !item.title.toLowerCase().includes(kw)) return false;
    return true;
  });

  const map = new Map<string, ConversationRecord[]>();
  for (const item of [...filtered].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))) {
    const label = groupLabel(item);
    const list = map.get(label);
    if (list) list.push(item);
    else map.set(label, [item]);
  }
  return [...map.entries()].map(([label, items]) => ({ label, items }));
});

/** 未归属任何工作空间的对话 → 顶部「任务」分组（对齐参考设计）。 */
const looseTasks = computed<ConversationRecord[]>(() =>
  [...conversations.value]
    .filter(isLooseTask)
    .filter((item) => {
      if (workspaceFilter.value !== ALL_WORKSPACES && workspaceFilter.value !== DEFAULT_GROUP) {
        return false;
      }
      const kw = keyword.value.trim().toLowerCase();
      return !kw || item.title.toLowerCase().includes(kw);
    })
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
);

/** 已登记工作空间的对话 → 「空间」分组（目录行 + 缩进子项）。 */
const spaceGroups = computed(() =>
  groups.value.filter((group) => group.label !== DEFAULT_GROUP || !isLooseTaskGroup(group)),
);

/** 目录行「OPEN AIGC」是否真的对应一个已登记的真实空间（而非默认区）。 */
function isLooseTaskGroup(group: { label: string; items: ConversationRecord[] }): boolean {
  return group.label === DEFAULT_GROUP && group.items.every(isLooseTask);
}

/** 搜索/筛选后是否命中任何对话（两个分组合起来看）。 */
const hasMatches = computed(() => groups.value.length > 0);

const hasConversations = computed(() => conversations.value.length > 0);

async function refresh(): Promise<void> {
  listPhase.value = "loading";
  listError.value = null;
  try {
    const records = await refreshConversationList();
    listPhase.value = "ready";
    // 首次进入或当前选中项已不存在时，默认打开最近一条，减少一次点击。
    if (!selectedId.value || !records.some((item) => item.id === selectedId.value)) {
      const first = records[0];
      if (first) void select(first.id);
      else resetDetail();
    }
  } catch (error) {
    conversations.value = [];
    listPhase.value = "error";
    listError.value = error instanceof Error ? error.message : "历史对话加载失败。";
  }
}

/* ──────────────────────────────────────────────
 *  只读回放
 * ────────────────────────────────────────────── */

const selectedId = ref<string | null>(null);
const messages = ref<MessageRecord[]>([]);
const invocations = ref<ToolInvocationRecord[]>([]);
const detailPhase = ref<"idle" | "loading" | "ready" | "error">("idle");
const detailError = ref<string | null>(null);

const selectedConversation = computed(
  () => conversations.value.find((item) => item.id === selectedId.value) ?? null,
);

const invocationsByMessage = computed(() => {
  const map = new Map<string, ToolInvocationRecord[]>();
  for (const inv of invocations.value) {
    const list = map.get(inv.messageId);
    if (list) list.push(inv);
    else map.set(inv.messageId, [inv]);
  }
  return map;
});

/**
 * 只读回放条目。
 * 跳过 system（内部提示）与 tool（原始工具结果）消息——工具信息已在助手侧的
 * 工具条与媒体区呈现，直接铺原始 JSON 反而干扰阅读。
 */
const transcript = computed(() =>
  messages.value
    .filter((message) => message.role === "user" || message.role === "assistant")
    .map((message) => ({
      message,
      userContent: message.role === "user" ? parseUserMessageContent(message.content) : null,
      tools: (invocationsByMessage.value.get(message.id) ?? []).map((inv) => ({
        id: inv.id,
        label: TOOL_LABELS[inv.toolName] ?? inv.toolName,
        status: inv.status,
        media: extractMedia(inv.resultJson),
      })),
    })),
);

function resetDetail(): void {
  selectedId.value = null;
  messages.value = [];
  invocations.value = [];
  detailPhase.value = "idle";
  detailError.value = null;
}

async function select(id: string): Promise<void> {
  selectedId.value = id;
  messages.value = [];
  invocations.value = [];
  detailPhase.value = "loading";
  detailError.value = null;
  try {
    const [msgRecords, invRecords] = await Promise.all([listMessages(id), listInvocations(id)]);
    // 期间用户可能已经点了别的对话，丢弃过期结果。
    if (selectedId.value !== id) return;
    messages.value = msgRecords;
    invocations.value = invRecords;
    detailPhase.value = "ready";
  } catch (error) {
    if (selectedId.value !== id) return;
    detailPhase.value = "error";
    detailError.value = error instanceof Error ? error.message : "读取对话内容失败。";
  }
}

/** 「继续对话」：回到创意工坊并载入该会话。这是本页唯一会离开的出口。 */
function continueConversation(id: string): void {
  void router.push({ path: "/generations", query: { conversation: id } });
}

/* ──────────────────────────────────────────────
 *  任务管理口径
 *
 *  任务/空间中的历史对话只读：可以点开回放，也可以「继续对话」往同一会话
 *  追加新内容，但不提供删除 / 清空消息 / 重命名 —— 历史是日志，不做整理。
 *  （原右键菜单曾含这三项，迁到本页后按产品口径移除。）
 * ────────────────────────────────────────────── */

/** 新建任务：回到创意工坊并清空当前会话（经 shell 动作总线通知生成页）。 */
function startNewTask(): void {
  shellActions.requestNewConversation();
  void router.push("/generations");
}

/* ──────────────────────────────────────────────
 *  工具函数
 * ────────────────────────────────────────────── */

/** 从工具调用结果 JSON 中提取可回放的媒体地址（图片 / 视频）。 */
function extractMedia(resultJson: string | null): { images: string[]; videos: string[] } {
  const images: string[] = [];
  const videos: string[] = [];
  if (!resultJson) return { images, videos };
  const push = (url: string): void => {
    if (!/^(https?:|asset:)/.test(url)) return;
    if (isVideoUrl(url)) {
      if (!videos.includes(url)) videos.push(url);
    } else if (!images.includes(url)) {
      images.push(url);
    }
  };
  try {
    const data = JSON.parse(resultJson) as {
      imageUrl?: unknown;
      attempt?: { remoteJobId?: unknown };
      localAsset?: { filePath?: unknown };
    };
    if (typeof data.imageUrl === "string") push(data.imageUrl);
    const remoteId = typeof data.attempt?.remoteJobId === "string" ? data.attempt.remoteJobId : "";
    const remoteUrl = remoteId.replace(/^proxy:(image|video):/, "").replace(/^image:/, "");
    if (remoteUrl) push(remoteUrl);
    if (typeof data.localAsset?.filePath === "string") {
      try {
        push(convertFileSrc(data.localAsset.filePath));
      } catch {
        // 路径不可解析时忽略
      }
    }
  } catch {
    // 非 JSON，无媒体可提取
  }
  return { images, videos };
}

onMounted(() => {
  void refresh();
});
</script>

<template>
  <div class="tasks">
    <header class="tasks__header">
      <!-- 页面标题由顶栏统一承载（全站约定），此处只放副标题，避免重复标题 -->
      <p class="tasks__subtitle">
        历史对话集中在这里查看，点开即可回放；需要接着聊时再「继续对话」。
      </p>
      <button
        class="tasks__refresh"
        type="button"
        :disabled="listPhase === 'loading'"
        title="刷新历史对话"
        aria-label="刷新历史对话"
        @click="refresh"
      >
        <RefreshCcw :size="14" :class="{ 'is-spinning': listPhase === 'loading' }" />
      </button>
    </header>

    <p class="tasks__notice">
      <Info :size="14" :stroke-width="1.8" />
      <span>
        历史对话与消息已保存在本机，重启后仍可查看。请注意：已删除的对话无法恢复；输入框中尚未发送的内容不会保存。
      </span>
    </p>

    <div class="tasks__panes">
      <!-- ── 左列：历史对话列表 ── -->
      <section class="tasks__panel" aria-label="历史对话列表">
        <button type="button" class="tasks__new" @click="startNewTask">
          <Plus :size="15" :stroke-width="2" />
          <span>新建任务</span>
        </button>

        <div class="tasks__tools">
          <label class="tasks__search">
            <Search :size="14" :stroke-width="1.8" />
            <input v-model="keyword" type="search" placeholder="搜索任务" aria-label="搜索任务" />
            <button
              v-if="keyword"
              type="button"
              class="tasks__search-clear"
              aria-label="清除搜索"
              @click="keyword = ''"
            >
              <X :size="12" />
            </button>
          </label>
          <select v-model="workspaceFilter" class="tasks__filter" aria-label="按工作目录筛选">
            <option v-for="option in workspaceOptions" :key="option" :value="option">
              {{ option === ALL_WORKSPACES ? "全部空间" : option }}
            </option>
          </select>
        </div>

        <div class="tasks__list">
          <div v-if="listPhase === 'loading' && !hasConversations" class="tasks__list-hint">
            <LoaderCircle class="is-spinning" :size="14" />
            <span>正在加载历史…</span>
          </div>
          <div v-else-if="listPhase === 'error'" class="tasks__list-hint is-error">
            <MessageSquareText :size="14" />
            <span>{{ listError }}</span>
          </div>
          <div v-else-if="!hasConversations" class="tasks__list-hint">
            <MessageSquareText :size="14" />
            <span>暂无历史对话</span>
          </div>
          <div v-else-if="!hasMatches" class="tasks__list-hint">
            <Search :size="14" />
            <span>没有匹配的任务</span>
          </div>

          <!-- 任务：未归属空间的对话，扁平列表（标题 + 相对时间） -->
          <section v-if="looseTasks.length" class="tasks__section">
            <h2 class="tasks__section-title">
              任务 <span class="tasks__section-count">({{ looseTasks.length }})</span>
            </h2>
            <template v-for="item in looseTasks" :key="item.id">
              <button
                type="button"
                class="tasks__item"
                :class="{ 'is-active': selectedId === item.id }"
                :title="`${item.title} · ${formatDateTime(item.updatedAt)}`"
                @click="select(item.id)"
              >
                <span class="tasks__item-title">{{ item.title }}</span>
                <span class="tasks__item-time">{{ formatRelativeTime(item.updatedAt) }}</span>
              </button>
            </template>
          </section>

          <!-- 空间：按工作目录分组，目录行 + 缩进子项 -->
          <section v-if="spaceGroups.length" class="tasks__section">
            <h2 class="tasks__section-title">
              空间 <span class="tasks__section-count">({{ spaceGroups.length }})</span>
            </h2>
            <div v-for="group in spaceGroups" :key="group.label" class="tasks__space">
              <div class="tasks__space-head">
                <Folder :size="15" :stroke-width="1.8" />
                <span class="tasks__space-name">{{ group.label }}</span>
              </div>
              <template v-for="item in group.items" :key="item.id">
                <button
                  type="button"
                  class="tasks__item is-child"
                  :class="{ 'is-active': selectedId === item.id }"
                  :title="`${item.title} · ${formatDateTime(item.updatedAt)}`"
                  @click="select(item.id)"
                >
                  <span class="tasks__item-title">{{ item.title }}</span>
                  <span class="tasks__item-time">{{ formatRelativeTime(item.updatedAt) }}</span>
                </button>
              </template>
            </div>
          </section>
        </div>
      </section>

      <!-- ── 右列：只读回放 ── -->
      <section class="tasks__panel" aria-label="对话详情">
        <template v-if="selectedConversation">
          <header class="tasks__detail-head">
            <div class="tasks__detail-meta">
              <h2 class="tasks__detail-title">{{ selectedConversation.title }}</h2>
              <span class="tasks__detail-sub">
                {{ groupLabel(selectedConversation) }} ·
                {{ formatDateTime(selectedConversation.updatedAt) }}
              </span>
            </div>
            <button
              type="button"
              class="tasks__continue"
              @click="continueConversation(selectedConversation.id)"
            >
              <Play :size="14" :stroke-width="1.8" />
              <span>继续对话</span>
            </button>
          </header>

          <div class="tasks__detail-body">
            <div v-if="detailPhase === 'loading'" class="tasks__list-hint">
              <LoaderCircle class="is-spinning" :size="14" />
              <span>正在读取对话…</span>
            </div>
            <div v-else-if="detailPhase === 'error'" class="tasks__list-hint is-error">
              <MessageSquareText :size="14" />
              <span>{{ detailError }}</span>
            </div>
            <EmptyState
              v-else-if="transcript.length === 0"
              compact
              :icon="MessageSquareText"
              title="这段对话还没有内容"
              description="点「继续对话」回到创意工坊，开始这一轮的创作。"
            />
            <div v-else class="tasks__transcript">
              <div
                v-for="entry in transcript"
                :key="entry.message.id"
                class="tasks__turn"
                :class="`is-${entry.message.role}`"
              >
                <template v-if="entry.userContent">
                  <div class="tasks__bubble">
                    <p v-if="entry.userContent.text" class="tasks__bubble-text">
                      {{ entry.userContent.text }}
                    </p>
                    <div v-if="entry.userContent.images.length > 0" class="tasks__media">
                      <img
                        v-for="(img, idx) in entry.userContent.images"
                        :key="idx"
                        :src="img"
                        class="tasks__image"
                        alt="上传的图片"
                        loading="lazy"
                      />
                    </div>
                  </div>
                  <span class="tasks__time">{{ formatDateTime(entry.message.createdAt) }}</span>
                </template>

                <template v-else>
                  <!-- 内容来自自有 LLM，可信来源；v-html 用于渲染 markdown 媒体语法 -->
                  <!-- eslint-disable vue/no-v-html -->
                  <div
                    v-if="entry.message.content"
                    class="tasks__ai-text"
                    v-html="renderRichContent(entry.message.content)"
                  />
                  <!-- eslint-enable vue/no-v-html -->
                  <div v-if="entry.tools.length > 0" class="tasks__tools-row">
                    <span
                      v-for="tool in entry.tools"
                      :key="tool.id"
                      class="tasks__tool-chip"
                      :class="`is-${tool.status}`"
                    >
                      {{ tool.label }}
                    </span>
                  </div>
                  <div v-for="tool in entry.tools" :key="`media-${tool.id}`" class="tasks__media">
                    <img
                      v-for="(img, idx) in tool.media.images"
                      :key="`i-${idx}`"
                      :src="img"
                      class="tasks__image"
                      alt="生成结果"
                      loading="lazy"
                    />
                    <video
                      v-for="(video, idx) in tool.media.videos"
                      :key="`v-${idx}`"
                      :src="video"
                      class="tasks__video"
                      controls
                      preload="metadata"
                    />
                  </div>
                  <span class="tasks__time">{{ formatDateTime(entry.message.createdAt) }}</span>
                </template>
              </div>
            </div>
          </div>
        </template>

        <EmptyState
          v-else
          :icon="ChevronRight"
          title="选择一段历史对话"
          description="在左侧点开任意对话即可在这里阅读全文，需要接着聊时再点「继续对话」。"
        />
      </section>
    </div>
  </div>
</template>

<style scoped>
.tasks {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: var(--space-3);
}

/* —— 页头 —— */
.tasks__header {
  display: flex;
  flex: 0 0 auto;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-3);
}

.tasks__subtitle {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  line-height: var(--line-height-snug);
}

.tasks__refresh {
  display: inline-grid;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  place-items: center;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.tasks__refresh:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.tasks__refresh:disabled {
  cursor: wait;
  opacity: 0.7;
}

/* —— 保存范围说明 —— */
.tasks__notice {
  display: flex;
  flex: 0 0 auto;
  margin: 0;
  padding: var(--space-2) var(--space-3);
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font-size: var(--text-caption);
  line-height: var(--line-height-normal);
}

.tasks__notice svg {
  flex: 0 0 14px;
  margin-top: 2px;
  color: var(--color-text-tertiary);
}

/* —— 双栏 —— */
.tasks__panes {
  display: grid;
  min-height: 0;
  flex: 1;
  grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
  gap: var(--space-3);
}

.tasks__panel {
  display: flex;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
}

/* —— 左列：搜索 + 筛选 —— */
.tasks__tools {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2);
  flex-direction: column;
  gap: var(--space-2);
  border-bottom: 1px solid var(--color-border-subtle);
}

.tasks__search {
  display: flex;
  height: 30px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
}

.tasks__search:focus-within {
  border-color: var(--color-accent);
}

.tasks__search input {
  min-width: 0;
  flex: 1;
  color: var(--color-text);
  border: none;
  background: transparent;
  font-family: inherit;
  font-size: var(--text-footnote);
  outline: none;
}

/* 焦点反馈由外层容器的 :focus-within 边框承担；输入框自身再套一圈
   全局 focus-ring 会在框内多出一个方形描边（视觉噪音）。 */
.tasks__search input:focus-visible {
  box-shadow: none;
}

.tasks__search input::placeholder {
  color: var(--color-text-tertiary);
}

.tasks__search-clear {
  display: inline-grid;
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  place-items: center;
  color: var(--color-text-tertiary);
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
}

.tasks__search-clear:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.tasks__filter {
  height: 30px;
  padding: 0 var(--space-2);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  font-family: inherit;
  font-size: var(--text-footnote);
  outline: none;
  cursor: pointer;
}

/* —— 左列：列表 —— */
.tasks__list {
  display: flex;
  min-height: 0;
  padding: var(--space-1);
  flex: 1;
  flex-direction: column;
  gap: 2px;
  overflow: hidden auto;
}

.tasks__list-hint {
  display: flex;
  min-height: 40px;
  padding: var(--space-2) var(--space-3);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  font-size: var(--text-footnote);
}

.tasks__list-hint.is-error {
  color: var(--color-danger);
}

/* —— 新建任务（对齐参考设计：图标 + 文字的主操作条） —— */
.tasks__new {
  display: flex;
  width: auto;
  height: 34px;
  margin: var(--space-2) var(--space-2) 0;
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-accent-text);
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  font-family: inherit;
  font-size: var(--text-footnote);
  font-weight: var(--font-weight-medium);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.tasks__new:hover {
  background: color-mix(in oklab, var(--color-accent) 22%, transparent);
}

.tasks__new:active {
  background: color-mix(in oklab, var(--color-accent) 30%, transparent);
}

/* —— 分组：任务 / 空间 —— */
.tasks__section {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.tasks__section + .tasks__section {
  margin-top: var(--space-4);
}

.tasks__section-title {
  display: flex;
  height: 28px;
  margin: 0 0 var(--space-1);
  padding: 0 var(--space-2);
  align-items: center;
  gap: 4px;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: var(--font-weight-medium);
}

.tasks__section-count {
  color: var(--color-text-tertiary);
}

/* 空间：目录行 + 缩进子项 */
.tasks__space {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.tasks__space + .tasks__space {
  margin-top: var(--space-2);
}

.tasks__space-head {
  display: flex;
  height: 32px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text);
  font-size: var(--text-footnote);
  font-weight: var(--font-weight-medium);
}

.tasks__space-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tasks__item {
  display: flex;
  width: 100%;
  min-height: 34px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

/* 空间子项：相对目录行缩进 */
.tasks__item.is-child {
  padding-left: calc(var(--space-2) + 23px);
}

.tasks__item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.tasks__item.is-active {
  color: var(--color-text);
  background: var(--color-surface-selected);
  font-weight: var(--font-weight-medium);
}

.tasks__item-title {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  font-size: var(--text-footnote);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tasks__item-time {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-weight: var(--font-weight-regular);
  white-space: nowrap;
}

/* —— 右列：详情头 —— */
.tasks__detail-head {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-3);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  border-bottom: 1px solid var(--color-border-subtle);
}

.tasks__detail-meta {
  min-width: 0;
}

.tasks__detail-title {
  margin: 0;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-subhead);
  font-weight: var(--font-weight-medium);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tasks__detail-sub {
  display: block;
  margin-top: 2px;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
}

.tasks__continue {
  display: inline-flex;
  height: 32px;
  padding: 0 var(--space-3);
  flex: 0 0 auto;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-on-accent);
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  font-family: inherit;
  font-size: var(--text-footnote);
  font-weight: var(--font-weight-medium);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.tasks__continue:hover {
  background: var(--color-accent-hover);
}

/* —— 右列：回放正文 —— */
.tasks__detail-body {
  min-height: 0;
  flex: 1;
  overflow: hidden auto;
}

.tasks__transcript {
  display: flex;
  padding: var(--space-3);
  flex-direction: column;
  gap: var(--space-4);
}

.tasks__turn {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.tasks__turn.is-user {
  align-items: flex-end;
}

.tasks__bubble {
  max-width: 78%;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-surface);
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
}

.tasks__bubble-text {
  margin: 0;
  color: var(--color-text);
  font-size: var(--text-footnote);
  line-height: var(--line-height-normal);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.tasks__ai-text {
  color: var(--color-text);
  font-size: var(--text-footnote);
  line-height: var(--line-height-normal);
  overflow-wrap: anywhere;
}

.tasks__tools-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.tasks__tool-chip {
  display: inline-flex;
  height: 20px;
  padding: 0 var(--space-2);
  align-items: center;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  font-size: 11px;
}

.tasks__tool-chip.is-succeeded {
  color: var(--color-success);
  border-color: color-mix(in srgb, var(--color-success) 40%, transparent);
}

.tasks__tool-chip.is-failed {
  color: var(--color-danger);
  border-color: color-mix(in srgb, var(--color-danger) 40%, transparent);
}

.tasks__media {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.tasks__image,
.tasks__video {
  max-width: 100%;
  max-height: 320px;
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  object-fit: contain;
}

.tasks__time {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.is-spinning {
  animation: tasks-spin 900ms linear infinite;
}

@keyframes tasks-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
