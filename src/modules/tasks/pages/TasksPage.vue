<script setup lang="ts">
/**
 * TasksPage：「任务」页——历史对话的集中查看入口。
 *
 * 口径（对齐产品设计）：
 * - 左列：全部历史对话（按工作目录分组）+ 搜索 / 目录筛选 + 右键重命名/删除
 * - 右列：所选对话的**只读回放**，阅读时停留在本页，不跳回创意工坊
 * - 只有点「继续对话」才回到 /generations 接着聊
 * - 页面明确说明历史保存范围，避免用户误以为已删除内容可以找回
 *
 * 本页不改写对话内容，只提供重命名 / 删除两项管理操作。
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import {
  ChevronRight,
  Clock,
  Eraser,
  Folder,
  Info,
  LoaderCircle,
  MessageSquareText,
  Pencil,
  Play,
  RefreshCcw,
  Search,
  Trash2,
  X,
} from "@lucide/vue";

import {
  clearConversation,
  deleteConversation,
  listConversations,
  listInvocations,
  listMessages,
  renameConversation,
  type ConversationRecord,
  type MessageRecord,
  type ToolInvocationRecord,
} from "../../../bridge/agent";
import EmptyState from "../../../shared/ui/EmptyState.vue";
import { useToast } from "../../../shared/ui/useToast";
import { useConversationWorkspaces } from "../../../app/composables/useConversationWorkspaces";
import {
  isVideoUrl,
  parseUserMessageContent,
  renderRichContent,
} from "../../../shared/utils/richText";

const router = useRouter();
const toast = useToast();
const conversationWorkspaces = useConversationWorkspaces();

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

const conversations = ref<ConversationRecord[]>([]);
const listPhase = ref<"idle" | "loading" | "ready" | "error">("idle");
const listError = ref<string | null>(null);

const keyword = ref("");
const workspaceFilter = ref<string>(ALL_WORKSPACES);

function groupLabel(conversation: ConversationRecord): string {
  return conversationWorkspaces.getWorkspaceName(conversation.id) ?? DEFAULT_GROUP;
}

const workspaceOptions = computed<string[]>(() => {
  const set = new Set<string>();
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

const hasConversations = computed(() => conversations.value.length > 0);
const hasMatches = computed(() => groups.value.length > 0);

async function refresh(): Promise<void> {
  listPhase.value = "loading";
  listError.value = null;
  try {
    const records = await listConversations();
    conversations.value = records;
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
 *  右键菜单：重命名 / 删除
 *
 *  这两项能力原先挂在侧栏的历史面板上；历史迁到本页后一并搬过来，
 *  避免"能看到历史却无法管理"的功能回退。
 * ────────────────────────────────────────────── */

interface MenuState {
  x: number;
  y: number;
  conversation: ConversationRecord;
  mode: "menu" | "confirm-delete" | "confirm-clear";
}

const menu = ref<MenuState | null>(null);
const renamingId = ref<string | null>(null);
const renamingTitle = ref("");
const renameInput = ref<HTMLInputElement | null>(null);

const menuLeft = computed(() => Math.max(8, Math.min(menu.value?.x ?? 0, window.innerWidth - 200)));
const menuTop = computed(() => Math.max(8, Math.min(menu.value?.y ?? 0, window.innerHeight - 150)));

function openMenu(event: MouseEvent, conversation: ConversationRecord): void {
  event.preventDefault();
  event.stopPropagation();
  menu.value = { x: event.clientX, y: event.clientY, conversation, mode: "menu" };
}

function closeMenu(): void {
  menu.value = null;
}

function onMenuKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") closeMenu();
}

watch(
  () => menu.value !== null,
  (open) => {
    if (open) {
      window.addEventListener("click", closeMenu);
      window.addEventListener("keydown", onMenuKeydown);
      window.addEventListener("resize", closeMenu);
    } else {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("keydown", onMenuKeydown);
      window.removeEventListener("resize", closeMenu);
    }
  },
);

function beginDeleteConfirm(): void {
  if (!menu.value) return;
  menu.value = { ...menu.value, mode: "confirm-delete" };
}

function beginClearConfirm(): void {
  if (!menu.value) return;
  menu.value = { ...menu.value, mode: "confirm-clear" };
}

/** 清空消息：删除该对话的消息记录，保留对话本身；随后重新加载详情。 */
async function confirmClear(): Promise<void> {
  const target = menu.value?.conversation;
  closeMenu();
  if (!target) return;
  try {
    await clearConversation(target.id);
    toast.success("已清空该对话的消息。");
    if (selectedId.value === target.id) await select(target.id);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "清空对话失败。");
  }
}

async function confirmDelete(): Promise<void> {
  const target = menu.value?.conversation;
  closeMenu();
  if (!target) return;
  try {
    await deleteConversation(target.id);
    conversationWorkspaces.remove(target.id);
    if (selectedId.value === target.id) resetDetail();
    await refresh();
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "删除对话失败。");
  }
}

function startRename(conversation: ConversationRecord): void {
  closeMenu();
  renamingId.value = conversation.id;
  renamingTitle.value = conversation.title;
  void nextTick(() => {
    renameInput.value?.focus();
    renameInput.value?.select();
  });
}

function bindRenameInput(el: unknown): void {
  renameInput.value = el instanceof HTMLInputElement ? el : null;
}

async function commitRename(): Promise<void> {
  const id = renamingId.value;
  if (!id) return;
  renamingId.value = null;
  const original = conversations.value.find((item) => item.id === id);
  const title = renamingTitle.value.trim();
  if (!original || !title || title === original.title) return;
  try {
    const updated = await renameConversation(id, title);
    conversations.value = conversations.value.map((item) => (item.id === id ? updated : item));
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "重命名失败。");
  }
}

function cancelRename(): void {
  renamingId.value = null;
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

function formatDateTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
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
        <div class="tasks__tools">
          <label class="tasks__search">
            <Search :size="14" :stroke-width="1.8" />
            <input
              v-model="keyword"
              type="search"
              placeholder="搜索对话标题"
              aria-label="搜索对话标题"
            />
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
              {{ option === ALL_WORKSPACES ? "全部目录" : option }}
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
            <span>没有匹配的对话</span>
          </div>

          <div v-for="group in groups" :key="group.label" class="tasks__group">
            <h2 class="tasks__group-title">
              <Folder :size="13" :stroke-width="1.8" />
              <span>{{ group.label }}</span>
              <span class="tasks__group-count">{{ group.items.length }}</span>
            </h2>
            <template v-for="item in group.items" :key="item.id">
              <div v-if="renamingId === item.id" class="tasks__item is-editing">
                <input
                  :ref="bindRenameInput"
                  v-model="renamingTitle"
                  class="tasks__rename"
                  type="text"
                  maxlength="200"
                  aria-label="重命名对话"
                  @click.stop
                  @keydown.enter.prevent="commitRename"
                  @keydown.esc.prevent="cancelRename"
                  @blur="commitRename"
                />
              </div>
              <button
                v-else
                type="button"
                class="tasks__item"
                :class="{ 'is-active': selectedId === item.id }"
                :title="item.title"
                @click="select(item.id)"
                @contextmenu="openMenu($event, item)"
              >
                <span class="tasks__item-title">{{ item.title }}</span>
                <span class="tasks__item-time">
                  <Clock :size="11" :stroke-width="1.8" />
                  {{ formatDateTime(item.updatedAt) }}
                </span>
              </button>
            </template>
          </div>
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

    <!-- 右键菜单：重命名 / 删除（fixed 跟随鼠标） -->
    <div
      v-if="menu"
      class="tasks__menu"
      role="menu"
      :style="{ left: `${menuLeft}px`, top: `${menuTop}px` }"
      @click.stop
      @contextmenu.prevent.stop
    >
      <template v-if="menu.mode === 'menu'">
        <button
          type="button"
          class="tasks__menu-item"
          role="menuitem"
          @click.stop="startRename(menu.conversation)"
        >
          <Pencil :size="13" :stroke-width="1.8" />
          <span>重命名</span>
        </button>
        <button
          type="button"
          class="tasks__menu-item"
          role="menuitem"
          @click.stop="beginClearConfirm"
        >
          <Eraser :size="13" :stroke-width="1.8" />
          <span>清空消息</span>
        </button>
        <button
          type="button"
          class="tasks__menu-item is-danger"
          role="menuitem"
          @click.stop="beginDeleteConfirm"
        >
          <Trash2 :size="13" :stroke-width="1.8" />
          <span>删除对话</span>
        </button>
      </template>
      <template v-else-if="menu.mode === 'confirm-clear'">
        <p class="tasks__menu-hint">
          将删除该对话的全部消息，对话本身保留。此操作不可撤销，确认清空？
        </p>
        <div class="tasks__menu-actions">
          <button type="button" class="tasks__menu-confirm is-danger" @click.stop="confirmClear">
            清空
          </button>
          <button type="button" class="tasks__menu-confirm" @click.stop="closeMenu">取消</button>
        </div>
      </template>
      <template v-else>
        <p class="tasks__menu-hint">删除后不可恢复，确认删除该对话？</p>
        <div class="tasks__menu-actions">
          <button type="button" class="tasks__menu-confirm is-danger" @click.stop="confirmDelete">
            删除
          </button>
          <button type="button" class="tasks__menu-confirm" @click.stop="closeMenu">取消</button>
        </div>
      </template>
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

.tasks__group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.tasks__group-title {
  display: flex;
  height: 26px;
  margin: 0;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  font-weight: var(--font-weight-medium);
  letter-spacing: 0.02em;
}

.tasks__group-title span:first-of-type {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tasks__group-count {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
}

.tasks__item {
  display: flex;
  width: 100%;
  min-height: 38px;
  padding: var(--space-1) var(--space-2);
  flex-direction: column;
  justify-content: center;
  gap: 2px;
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
  overflow: hidden;
  font-size: var(--text-footnote);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tasks__item-time {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-weight: var(--font-weight-regular);
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

/* —— 内联重命名 —— */
.tasks__item.is-editing {
  padding: var(--space-1) var(--space-2);
  background: var(--color-surface-hover);
}

.tasks__rename {
  width: 100%;
  height: 26px;
  padding: 0 var(--space-2);
  color: var(--color-text);
  border: 1px solid var(--color-accent);
  border-radius: 7px;
  background: var(--color-canvas);
  font-family: inherit;
  font-size: var(--text-footnote);
  outline: none;
}

/* —— 右键菜单 —— */
.tasks__menu {
  position: fixed;
  z-index: var(--z-dropdown);
  display: flex;
  min-width: 200px;
  max-width: 280px;
  padding: 4px;
  flex-direction: column;
  gap: 2px;
  background: var(--material-topbar);
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  box-shadow: var(--shadow-menu);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.tasks__menu-item {
  display: flex;
  width: 100%;
  height: 30px;
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border: none;
  border-radius: 7px;
  background: transparent;
  font-family: inherit;
  font-size: var(--text-footnote);
  text-align: left;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.tasks__menu-item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.tasks__menu-item.is-danger:hover {
  color: var(--color-danger);
  background: var(--color-danger-soft);
}

.tasks__menu-hint {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  line-height: var(--line-height-normal);
}

.tasks__menu-actions {
  display: flex;
  gap: var(--space-2);
  padding: 0 var(--space-3) var(--space-2);
}

.tasks__menu-confirm {
  flex: 1;
  height: 26px;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: 7px;
  background: transparent;
  font-family: inherit;
  font-size: var(--text-caption);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.tasks__menu-confirm:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.tasks__menu-confirm.is-danger {
  color: #ffffff;
  border-color: transparent;
  background: var(--color-danger);
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
