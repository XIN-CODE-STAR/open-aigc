<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Archive,
  Bell,
  BookOpenText,
  Bot,
  ChevronRight,
  Folder,
  HardDrive,
  Images,
  ListTodo,
  LoaderCircle,
  MessageSquareText,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Plus,
  RefreshCcw,
  Settings,
  Trash2,
  Waypoints,
} from "@lucide/vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";

import appIconUrl from "../../assets/app-icon.svg";
import {
  deleteConversation,
  listConversations,
  renameConversation,
  type ConversationRecord,
} from "../bridge/agent";
import AuroraCanvas from "../shared/visual/AuroraCanvas.vue";
import ToastViewport from "../shared/ui/ToastViewport.vue";
import { useToast } from "../shared/ui/useToast";
import { usePreferencesStore } from "./stores/preferences";
import { useProjectDirectory } from "./stores/projectDirectory";
import { useWorkspaceStore } from "./stores/workspace";
import { useConversationWorkspaces } from "./composables/useConversationWorkspaces";
import { useMemoryCanvasStore } from "./stores/memoryCanvas";

const route = useRoute();
const router = useRouter();
const toast = useToast();
const preferences = usePreferencesStore();
const workspace = useWorkspaceStore();
const projectDir = useProjectDirectory();
const conversationWorkspaces = useConversationWorkspaces();
const memoryCanvas = useMemoryCanvasStore();

const isGenerationsRoute = computed(() => route.path === "/generations");
const pageTitle = computed(() => String(route.meta.title ?? "OPEN AIGC"));
/** 创意工坊（/generations）页面自带内部滚动，内容区需要零内边距与侧栏贴合。 */
const isFlushRoute = computed(() => route.path === "/generations");
const sidebarConversations = ref<ConversationRecord[]>([]);
const sidebarHistoryPhase = ref<"idle" | "loading" | "ready" | "error">("idle");
const sidebarHistoryError = ref<string | null>(null);

const SIDEBAR_CONVERSATION_REFRESH_EVENT = "aigc-agent-conversations-updated";

const primaryNavigationItems = computed(() => [
  {
    path: "/assets",
    label: "资源库",
    icon: Images,
  },
  {
    path: "/prompts",
    label: "提示词库",
    icon: BookOpenText,
  },
  {
    path: "/models",
    label: "模型",
    icon: Bot,
  },
  {
    path: "/backup",
    label: "备份",
    icon: Archive,
  },
]);

interface ConversationHistoryGroup {
  id: string;
  label: string;
  conversations: ConversationRecord[];
}

const localStatus = computed(() => {
  if (!workspace.isReady) {
    return workspace.errorMessage ? "本地异常" : "初始化中";
  }
  return projectDir.isCustom ? projectDir.displayName : "本地就绪";
});

function routeToConversation(conversation: ConversationRecord) {
  return {
    path: "/generations",
    query: { conversation: conversation.id },
  };
}

function conversationGroupLabel(conversation: ConversationRecord): string {
  // 优先从 localStorage 映射获取工作目录名
  const mapped = conversationWorkspaces.getWorkspaceName(conversation.id);
  if (mapped) return mapped;
  return "OPEN AIGC";
}

const conversationHistoryGroups = computed<ConversationHistoryGroup[]>(() => {
  const groups = new Map<string, ConversationRecord[]>();
  const sorted = [...sidebarConversations.value].sort((a, b) =>
    b.updatedAt.localeCompare(a.updatedAt),
  );
  for (const conversation of sorted) {
    const label = conversationGroupLabel(conversation);
    const items = groups.get(label);
    if (items) {
      items.push(conversation);
    } else {
      groups.set(label, [conversation]);
    }
  }
  return [...groups.entries()].map(([label, conversations]) => ({
    id: label,
    label,
    conversations,
  }));
});

const hasConversationHistory = computed(() => conversationHistoryGroups.value.length > 0);
const activeConversationId = computed(() =>
  typeof route.query.conversation === "string" ? route.query.conversation : null,
);

/** 目录分组的展开状态。默认全部收起，避免对话过多时一次性铺开。 */
const expandedHistoryGroups = ref<Record<string, boolean>>({});

function isHistoryGroupExpanded(groupId: string): boolean {
  return expandedHistoryGroups.value[groupId] === true;
}

function toggleHistoryGroup(groupId: string): void {
  expandedHistoryGroups.value[groupId] = !isHistoryGroupExpanded(groupId);
}

// 当前激活会话所在目录自动展开：深链接进入或从主面板切换会话时能看到选中项。
watch(
  () => [activeConversationId.value, sidebarConversations.value] as const,
  () => {
    const activeId = activeConversationId.value;
    if (!activeId) return;
    const active = sidebarConversations.value.find((item) => item.id === activeId);
    if (!active) return;
    expandedHistoryGroups.value[conversationGroupLabel(active)] = true;
  },
  { immediate: true },
);

async function refreshSidebarConversations(): Promise<void> {
  sidebarHistoryPhase.value = "loading";
  sidebarHistoryError.value = null;
  try {
    sidebarConversations.value = await listConversations();
    sidebarHistoryPhase.value = "ready";
  } catch (error) {
    sidebarConversations.value = [];
    sidebarHistoryPhase.value = "error";
    sidebarHistoryError.value = error instanceof Error ? error.message : "历史对话加载失败。";
  }
}

function handleNewConversation(): void {
  void router.push("/generations");
}

function handleConversationRefreshEvent(): void {
  void refreshSidebarConversations();
}

/* ── 历史对话右键菜单：重命名 / 删除 ── */

interface HistoryMenuState {
  x: number;
  y: number;
  conversation: ConversationRecord;
  /** menu = 功能列表；confirm-delete = 删除二次确认。 */
  mode: "menu" | "confirm-delete";
}

const historyMenu = ref<HistoryMenuState | null>(null);
const renamingConversationId = ref<string | null>(null);
const renamingTitle = ref("");
const renameInput = ref<HTMLInputElement | null>(null);

const historyMenuLeft = computed(() => {
  const x = historyMenu.value?.x ?? 0;
  return Math.max(8, Math.min(x, window.innerWidth - 190));
});

const historyMenuTop = computed(() => {
  const y = historyMenu.value?.y ?? 0;
  return Math.max(8, Math.min(y, window.innerHeight - 150));
});

function openHistoryMenu(event: MouseEvent, conversation: ConversationRecord): void {
  event.preventDefault();
  event.stopPropagation();
  historyMenu.value = { x: event.clientX, y: event.clientY, conversation, mode: "menu" };
}

function closeHistoryMenu(): void {
  historyMenu.value = null;
}

function onWindowContextMenu(event: MouseEvent): void {
  // 列表项的右键已 preventDefault 并重新打开菜单（openHistoryMenu），此时不关闭。
  if (!event.defaultPrevented) {
    closeHistoryMenu();
  }
}

function onHistoryMenuKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    closeHistoryMenu();
  }
}

// 仅在菜单开/关切换时挂载全局关闭监听，避免重复绑定。
watch(
  () => historyMenu.value !== null,
  (open) => {
    if (open) {
      window.addEventListener("click", closeHistoryMenu);
      window.addEventListener("contextmenu", onWindowContextMenu);
      window.addEventListener("keydown", onHistoryMenuKeydown);
      window.addEventListener("resize", closeHistoryMenu);
    } else {
      window.removeEventListener("click", closeHistoryMenu);
      window.removeEventListener("contextmenu", onWindowContextMenu);
      window.removeEventListener("keydown", onHistoryMenuKeydown);
      window.removeEventListener("resize", closeHistoryMenu);
    }
  },
);

function beginDeleteConfirm(): void {
  if (!historyMenu.value) return;
  historyMenu.value = { ...historyMenu.value, mode: "confirm-delete" };
}

async function confirmDeleteConversation(): Promise<void> {
  const target = historyMenu.value?.conversation;
  closeHistoryMenu();
  if (!target) return;
  try {
    await deleteConversation(target.id);
    conversationWorkspaces.remove(target.id);
    if (activeConversationId.value === target.id) {
      void router.push({ path: "/generations" });
    }
    await refreshSidebarConversations();
    // 通知 GenerationsPage 同步会话列表；若当前选中会话被删除会一并清理。
    window.dispatchEvent(new CustomEvent(SIDEBAR_CONVERSATION_REFRESH_EVENT));
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "删除对话失败。");
  }
}

function startRenameConversation(conversation: ConversationRecord): void {
  closeHistoryMenu();
  renamingConversationId.value = conversation.id;
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
  const id = renamingConversationId.value;
  if (!id) return;
  renamingConversationId.value = null;
  const original = sidebarConversations.value.find((item) => item.id === id);
  const title = renamingTitle.value.trim();
  if (!original || !title || title === original.title) return;
  try {
    const updated = await renameConversation(id, title);
    sidebarConversations.value = sidebarConversations.value.map((item) =>
      item.id === id ? updated : item,
    );
    window.dispatchEvent(new CustomEvent(SIDEBAR_CONVERSATION_REFRESH_EVENT));
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "重命名失败。");
  }
}

function cancelRename(): void {
  renamingConversationId.value = null;
}

onMounted(() => {
  window.addEventListener(SIDEBAR_CONVERSATION_REFRESH_EVENT, handleConversationRefreshEvent);
  void workspace.ensureReady();
  void refreshSidebarConversations();
});

onUnmounted(() => {
  window.removeEventListener(SIDEBAR_CONVERSATION_REFRESH_EVENT, handleConversationRefreshEvent);
});
</script>

<template>
  <div class="app-shell">
    <aside
      class="app-sidebar material-vibrancy"
      :class="{ 'is-collapsed': !preferences.navigationExpanded }"
      aria-label="主导航"
    >
      <div class="sidebar-header">
        <div
          class="brand"
          :title="preferences.navigationExpanded ? undefined : projectDir.workspaceName"
        >
          <img :src="appIconUrl" alt="" />
          <span class="brand-name">OPEN AIGC</span>
        </div>
        <button
          class="icon-button sidebar-toggle"
          type="button"
          :aria-label="preferences.navigationExpanded ? '折叠导航' : '展开导航'"
          :title="preferences.navigationExpanded ? '折叠导航' : '展开导航'"
          @click="preferences.toggleNavigation"
        >
          <PanelLeftClose v-if="preferences.navigationExpanded" :size="17" />
          <PanelLeftOpen v-else :size="17" />
        </button>
      </div>

      <nav class="navigation" aria-label="主功能">
        <RouterLink
          v-for="item in primaryNavigationItems"
          :key="item.path"
          class="nav-item"
          :to="item.path"
          :title="preferences.navigationExpanded ? undefined : item.label"
        >
          <component :is="item.icon" :size="18" :stroke-width="1.8" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </nav>

      <div class="sidebar-action">
        <button
          class="new-conversation-btn"
          type="button"
          :title="preferences.navigationExpanded ? undefined : '新建对话'"
          @click="handleNewConversation"
        >
          <Plus :size="16" :stroke-width="2" />
          <span>新建对话</span>
        </button>
      </div>

      <section class="history-panel" aria-label="历史对话">
        <div class="history-panel__header">
          <h2 class="nav-group__title">历史对话</h2>
          <button
            class="history-refresh"
            type="button"
            title="刷新历史对话"
            aria-label="刷新历史对话"
            :disabled="sidebarHistoryPhase === 'loading'"
            @click="refreshSidebarConversations"
          >
            <RefreshCcw :size="14" :class="{ 'is-spinning': sidebarHistoryPhase === 'loading' }" />
          </button>
        </div>

        <div
          v-if="sidebarHistoryPhase === 'loading' && !hasConversationHistory"
          class="history-empty"
        >
          <LoaderCircle class="is-spinning" :size="15" />
          <span>正在加载历史...</span>
        </div>
        <div v-else-if="sidebarHistoryPhase === 'error'" class="history-empty is-error">
          <MessageSquareText :size="15" />
          <span>{{ sidebarHistoryError }}</span>
        </div>
        <div v-else-if="!hasConversationHistory" class="history-empty">
          <MessageSquareText :size="15" />
          <span>暂无历史对话</span>
        </div>

        <div v-for="group in conversationHistoryGroups" :key="group.id" class="history-group">
          <button
            class="history-project"
            type="button"
            :aria-expanded="isHistoryGroupExpanded(group.id)"
            :title="group.label"
            @click="toggleHistoryGroup(group.id)"
          >
            <Folder :size="16" :stroke-width="1.8" />
            <span class="history-project__label">{{ group.label }}</span>
            <span class="history-project__count">{{ group.conversations.length }}</span>
            <ChevronRight
              :size="14"
              class="history-project__chevron"
              :class="{ 'is-expanded': isHistoryGroupExpanded(group.id) }"
            />
          </button>
          <template v-if="isHistoryGroupExpanded(group.id)">
            <template v-for="conversation in group.conversations" :key="conversation.id">
              <div
                v-if="renamingConversationId === conversation.id"
                class="history-item is-editing"
              >
                <input
                  :ref="bindRenameInput"
                  v-model="renamingTitle"
                  class="history-rename-input"
                  type="text"
                  maxlength="200"
                  aria-label="重命名对话"
                  @keydown.enter.prevent="commitRename"
                  @keydown.esc.prevent="cancelRename"
                  @blur="commitRename"
                />
              </div>
              <RouterLink
                v-else
                class="history-item"
                :class="{ 'is-active': activeConversationId === conversation.id }"
                :to="routeToConversation(conversation)"
                :title="conversation.title"
                @contextmenu="openHistoryMenu($event, conversation)"
              >
                <span>{{ conversation.title }}</span>
              </RouterLink>
            </template>
          </template>
        </div>
      </section>

      <div class="sidebar-footer">
        <div class="local-status" :title="preferences.navigationExpanded ? undefined : localStatus">
          <HardDrive :size="16" :stroke-width="1.8" />
          <span>{{ localStatus }}</span>
        </div>
        <div class="sidebar-footer__actions">
          <RouterLink class="icon-button" to="/tasks" aria-label="任务中心" title="任务中心">
            <ListTodo :size="16" :stroke-width="1.8" />
          </RouterLink>
          <RouterLink class="icon-button" to="/notifications" aria-label="通知" title="通知">
            <Bell :size="16" :stroke-width="1.8" />
          </RouterLink>
          <RouterLink
            class="icon-button sidebar-settings-button"
            to="/settings"
            aria-label="设置"
            title="设置"
          >
            <Settings :size="16" :stroke-width="1.8" />
          </RouterLink>
        </div>
      </div>
    </aside>

    <section class="app-workspace">
      <AuroraCanvas class="app-workspace__aurora" />
      <header class="topbar material-vibrancy">
        <div class="topbar__context">
          <h1 class="topbar__title">{{ pageTitle }}</h1>
        </div>
        <button
          v-if="isGenerationsRoute"
          class="topbar-canvas-btn"
          :class="{ 'is-active': memoryCanvas.visible }"
          title="工作记忆画布"
          @click="memoryCanvas.toggle()"
        >
          <Waypoints :size="16" :stroke-width="1.8" />
        </button>
      </header>

      <main class="page-scroll" :class="{ 'is-flush': isFlushRoute }" tabindex="-1">
        <RouterView />
      </main>
    </section>

    <!-- 历史对话右键菜单（重命名 / 删除），fixed 定位跟随鼠标出现 -->
    <div
      v-if="historyMenu"
      class="history-menu"
      role="menu"
      :style="{ left: `${historyMenuLeft}px`, top: `${historyMenuTop}px` }"
      @click.stop
      @contextmenu.prevent.stop
    >
      <template v-if="historyMenu.mode === 'menu'">
        <button
          type="button"
          class="history-menu__item"
          role="menuitem"
          @click.stop="startRenameConversation(historyMenu.conversation)"
        >
          <Pencil :size="14" :stroke-width="1.8" />
          <span>重命名</span>
        </button>
        <button
          type="button"
          class="history-menu__item is-danger"
          role="menuitem"
          @click.stop="beginDeleteConfirm"
        >
          <Trash2 :size="14" :stroke-width="1.8" />
          <span>删除对话</span>
        </button>
      </template>
      <template v-else>
        <p class="history-menu__hint">删除后不可恢复，确认删除该对话？</p>
        <div class="history-menu__actions">
          <button
            type="button"
            class="history-menu__confirm is-danger"
            @click.stop="confirmDeleteConversation"
          >
            删除
          </button>
          <button type="button" class="history-menu__confirm" @click.stop="closeHistoryMenu">
            取消
          </button>
        </div>
      </template>
    </div>

    <ToastViewport />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  width: 100%;
  height: 100%;
  color: var(--color-text);
  background: var(--color-canvas);
  --sidebar-width: 300px;
}

/* —— iOS/macOS 风格 vibrancy 侧栏（apple-design §12 材质与深度）—— */
.app-sidebar {
  display: flex;
  flex: 0 0 var(--sidebar-width);
  width: var(--sidebar-width);
  min-width: 0;
  height: 100%;
  flex-direction: column;
  border-right: 1px solid var(--color-border-subtle);
  background: var(--material-sidebar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  transition:
    flex-basis var(--duration-slow) var(--ease-drawer),
    width var(--duration-slow) var(--ease-drawer);
}

.app-sidebar.is-collapsed {
  flex-basis: var(--sidebar-compact-width);
  width: var(--sidebar-compact-width);
}

/* —— Brand：macOS sidebar header 风格 —— */
.sidebar-header {
  display: flex;
  height: var(--topbar-height);
  min-height: var(--topbar-height);
  padding: 0 var(--space-3) 0 var(--space-5);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  background: var(--material-topbar);
  border-bottom: 1px solid var(--color-border-subtle);
}

.brand {
  display: flex;
  min-width: 0;
  flex: 0 1 auto;
  align-items: center;
  gap: var(--space-3);
  overflow: hidden;
  font-family: var(--font-display);
  font-size: var(--text-headline);
  font-weight: 600;
  letter-spacing: -0.01em;
  white-space: nowrap;
}

.brand img {
  flex: 0 0 28px;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-surface);
  box-shadow: var(--shadow-sm);
}

.brand-name {
  color: var(--color-text);
}

.navigation {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2) var(--space-3);
  flex-direction: column;
  gap: 2px;
  overflow: visible;
}

.nav-group {
  display: flex;
  margin-top: var(--space-4);
  flex-direction: column;
  gap: 2px;
}

/* —— iOS Settings 风格的 section header（caption + 次要色）—— */
.nav-group__title {
  height: 20px;
  margin: 0;
  padding: 0 var(--space-3);
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: 500;
  line-height: 20px;
  letter-spacing: 0.02em;
  text-transform: uppercase;
  white-space: nowrap;
}

/* —— iOS Settings 风格 nav-item：圆角矩形，激活态为 accent pill —— */
.nav-item {
  position: relative;
  display: flex;
  width: 100%;
  height: var(--nav-item-height);
  min-height: var(--nav-item-height);
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-3);
  overflow: hidden;
  color: var(--color-text);
  border-radius: var(--radius-control);
  font-size: var(--text-subhead);
  font-weight: 400;
  line-height: 20px;
  letter-spacing: -0.005em;
  text-decoration: none;
  white-space: nowrap;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

/* —— nav-item hover —— */
.nav-item:hover {
  background: var(--color-surface-hover);
}

.nav-item svg {
  flex: 0 0 18px;
  transition: color var(--duration-fast) var(--ease-out);
}

/* —— 新建对话按钮（sidebar 主操作区） —— */
.sidebar-action {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2) var(--space-3);
}

.new-conversation-btn {
  display: flex;
  width: 100%;
  height: 36px;
  padding: 0 var(--space-3);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-on-accent, #fff);
  border: none;
  border-radius: var(--radius-control);
  background: linear-gradient(135deg, var(--color-accent), var(--aurora-cyan, #6ee7b7));
  font-size: var(--text-subhead);
  font-weight: 600;
  cursor: pointer;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.new-conversation-btn:hover {
  opacity: 0.9;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.new-conversation-btn:active {
  opacity: 0.8;
}

.new-conversation-btn svg {
  flex: 0 0 16px;
}

.sidebar-footer {
  display: flex;
  min-height: 56px;
  padding: var(--space-2) var(--space-3);
  align-items: center;
  gap: var(--space-1);
  border-top: 1px solid var(--color-border-subtle);
}

.sidebar-footer__actions {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-left: auto;
}

.history-panel {
  display: flex;
  min-height: 0;
  padding: var(--space-4) var(--space-3) var(--space-3);
  flex: 1;
  flex-direction: column;
  gap: var(--space-2);
  overflow: hidden auto;
}

.history-panel__header {
  display: flex;
  height: 24px;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.history-panel__header .nav-group__title {
  flex: 1;
  padding-inline: var(--space-3) 0;
}

.history-refresh {
  display: inline-grid;
  width: 24px;
  height: 24px;
  flex: 0 0 24px;
  place-items: center;
  color: var(--color-text-tertiary);
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.history-refresh:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history-refresh:disabled {
  cursor: wait;
  opacity: 0.7;
}

.history-empty {
  display: flex;
  min-height: 36px;
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  font-size: var(--text-footnote);
  line-height: 18px;
}

.history-empty.is-error {
  color: var(--color-danger);
}

.history-group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.history-project,
.history-item {
  display: flex;
  min-width: 0;
  align-items: center;
  color: var(--color-text-secondary);
  border-radius: var(--radius-control);
  text-decoration: none;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.history-project {
  width: 100%;
  height: 32px;
  padding: 0 var(--space-3);
  gap: var(--space-2);
  border: 1px solid transparent;
  background: transparent;
  font-family: inherit;
  font-size: var(--text-footnote);
  font-weight: 500;
  text-align: left;
  cursor: pointer;
}

.history-project:hover,
.history-item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history-project > svg:first-of-type {
  flex: 0 0 16px;
}

.history-project span,
.history-item span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-project__label {
  flex: 1;
}

.history-project__count {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  font-weight: 400;
}

.history-project__chevron {
  color: var(--color-text-tertiary);
  transition: transform var(--duration-fast) var(--ease-out);
}

.history-project__chevron.is-expanded {
  transform: rotate(90deg);
}

.history-item {
  height: 30px;
  margin-left: 28px;
  padding: 0 var(--space-3);
  font-size: var(--text-footnote);
}

.history-item.is-active {
  color: var(--color-text);
  background: var(--color-surface-hover);
  font-weight: 500;
}

.local-status {
  display: flex;
  min-width: 0;
  height: var(--control-height);
  padding: 0 var(--space-2);
  flex: 1;
  align-items: center;
  gap: var(--space-2);
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  white-space: nowrap;
}

/* —— iOS account chip：圆形头像 + 用户名 + 角色 —— */
.user-status {
  display: flex;
  min-width: 0;
  height: var(--control-height);
  padding: 0 var(--space-2) 0 var(--space-1);
  flex: 1;
  align-items: center;
  gap: var(--space-2);
  overflow: hidden;
  border-radius: var(--radius-control);
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  white-space: nowrap;
  transition: background var(--duration-fast) var(--ease-out);
}

.user-status:hover {
  background: var(--color-surface-hover);
}

/* —— Aurora 风格用户头像：紫青渐变圆形 + 首字母 —— */
.user-avatar {
  display: grid;
  width: 28px;
  height: 28px;
  flex: 0 0 28px;
  place-items: center;
  color: #ffffff;
  border-radius: var(--radius-pill);
  background: linear-gradient(135deg, var(--aurora-violet) 0%, var(--aurora-cyan) 100%);
  font-family: var(--font-display);
  font-size: var(--text-footnote);
  font-weight: 600;
  letter-spacing: 0;
}

.user-meta {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
}

.user-name {
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-footnote);
  font-weight: 600;
  line-height: 16px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.user-role {
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  line-height: 14px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* —— iOS nav bar button：无边框，hover 时浅灰 —— */
.icon-button,
.status-button {
  display: inline-grid;
  height: var(--control-height);
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

.icon-button {
  flex: 0 0 var(--control-height);
  width: var(--control-height);
  padding: 0;
}

.icon-button:hover,
.status-button:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.sidebar-toggle,
.sidebar-settings-button {
  flex: 0 0 32px;
  width: 32px;
  height: 32px;
  border-radius: 9px;
}

.sidebar-toggle {
  margin-left: auto;
}

.sidebar-settings-button {
  color: var(--color-text-secondary);
  text-decoration: none;
}

.sidebar-settings-button.router-link-active {
  color: var(--color-accent);
  background: var(--color-accent-soft);
}

/* —— 侧栏折叠态 —— */
.app-sidebar.is-collapsed .sidebar-header {
  justify-content: center;
  padding: 0;
}

.app-sidebar.is-collapsed .brand {
  display: none;
}

.app-sidebar.is-collapsed .brand,
.app-sidebar.is-collapsed .nav-item,
.app-sidebar.is-collapsed .local-status,
.app-sidebar.is-collapsed .user-status {
  justify-content: center;
  padding-inline: 0;
}

.app-sidebar.is-collapsed .brand-name,
.app-sidebar.is-collapsed .nav-group__title,
.app-sidebar.is-collapsed .nav-item span,
.app-sidebar.is-collapsed .local-status span,
.app-sidebar.is-collapsed .user-meta,
.app-sidebar.is-collapsed .user-status .icon-button,
.app-sidebar.is-collapsed .history-panel,
.app-sidebar.is-collapsed .new-conversation-btn span {
  display: none;
}

.app-sidebar.is-collapsed .sidebar-footer {
  flex-direction: column;
  padding: var(--space-2);
}

.app-sidebar.is-collapsed .sidebar-footer__actions {
  display: none !important;
}

.app-sidebar.is-collapsed .sidebar-action {
  padding: var(--space-1) var(--space-2);
}

.app-sidebar.is-collapsed .new-conversation-btn {
  justify-content: center;
  height: var(--control-height);
  padding: 0;
  border-radius: var(--radius-control);
}

.app-sidebar.is-collapsed .sidebar-toggle,
.app-sidebar.is-collapsed .sidebar-settings-button {
  flex: 0 0 var(--control-height);
  width: var(--control-height);
  height: var(--control-height);
}

.app-sidebar.is-collapsed .sidebar-toggle {
  margin-left: 0;
}

.app-sidebar.is-collapsed .local-status,
.app-sidebar.is-collapsed .user-status {
  flex: 0 0 var(--control-height);
  width: var(--control-height);
  padding: 0;
}

.app-sidebar.is-collapsed .user-status:hover {
  background: transparent;
}

.app-workspace {
  position: relative;
  display: grid;
  min-width: 0;
  flex: 1;
  grid-template-rows: var(--topbar-height) minmax(0, 1fr);
  background: var(--color-canvas);
  overflow: hidden;
}

.app-workspace__aurora {
  z-index: 0;
  grid-row: 1 / -1;
  grid-column: 1 / -1;
}

/* —— 导航激活态：Codex 风格的低饱和选中面 —— */
.nav-item.router-link-active {
  color: var(--color-text);
  background: var(--color-surface-hover);
  font-weight: 500;
  box-shadow: none;
}

.nav-item.router-link-active:hover {
  background: var(--color-surface-hover);
}

/* —— iOS Large Title pattern 顶栏 —— */
.topbar {
  position: relative;
  z-index: 1;
  display: flex;
  min-width: 0;
  padding: 0 var(--page-padding);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.topbar__context {
  min-width: 0;
}

.topbar-canvas-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  cursor: pointer;
  flex-shrink: 0;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}

.topbar-canvas-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.topbar-canvas-btn.is-active {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

/* iOS navigation bar eyebrow：caption 风格的上下文信息 */
.topbar__eyebrow {
  display: block;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: 400;
  line-height: 16px;
  letter-spacing: 0.01em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* iOS Large Title：title-3 (20px) semibold + 紧字距 */
.topbar__title {
  margin: 0;
  overflow: hidden;
  font-family: var(--font-display);
  font-size: var(--text-title-3);
  font-weight: 600;
  line-height: 24px;
  letter-spacing: -0.01em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* —— 内容滚动区 —— */
.page-scroll {
  position: relative;
  z-index: 1;
  min-width: 0;
  min-height: 0;
  padding: var(--page-padding);
  overflow: auto;
  background: transparent;
}

/* 创意工坊页面自带内部滚动与背景：去掉内容区留白，让对话与侧栏直接贴合。 */
.page-scroll.is-flush {
  padding: 0;
  overflow: hidden;
}

/* —— 历史对话内联重命名 —— */
.history-item.is-editing {
  padding: 0 var(--space-1);
  background: var(--color-surface-hover);
}

.history-rename-input {
  width: 100%;
  height: 22px;
  padding: 0 var(--space-2);
  color: var(--color-text);
  border: 1px solid var(--color-accent);
  border-radius: 6px;
  background: var(--color-canvas);
  font-size: var(--text-footnote);
  font-family: inherit;
  outline: none;
}

/* —— 历史对话右键菜单 —— */
.history-menu {
  position: fixed;
  z-index: 60;
  display: flex;
  min-width: 168px;
  padding: 4px;
  flex-direction: column;
  gap: 2px;
  background: var(--material-topbar);
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.28);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.history-menu__item {
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
  font-size: var(--text-footnote);
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.history-menu__item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history-menu__item.is-danger:hover {
  color: var(--color-danger);
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
}

.history-menu__hint {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  line-height: 1.5;
}

.history-menu__actions {
  display: flex;
  gap: var(--space-2);
  padding: 0 var(--space-3) var(--space-2);
}

.history-menu__confirm {
  flex: 1;
  height: 26px;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-subtle);
  border-radius: 7px;
  background: transparent;
  font-size: var(--text-caption);
  font-family: inherit;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.history-menu__confirm:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history-menu__confirm.is-danger {
  color: #fff;
  border-color: transparent;
  background: var(--color-danger);
}

.history-menu__confirm.is-danger:hover {
  filter: brightness(1.08);
}

/* 启动占位：authStore.initialize() 完成前显示 loading */
.boot-screen {
  display: flex;
  min-height: 240px;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  color: var(--color-text-secondary);
  font-size: var(--text-subhead);
}

.boot-screen .is-spinning {
  color: var(--color-accent);
  animation: app-shell-spin 900ms linear infinite;
}

@keyframes app-shell-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 1199px) {
  .app-sidebar {
    flex-basis: var(--sidebar-compact-width);
    width: var(--sidebar-compact-width);
  }

  .app-sidebar .sidebar-header {
    justify-content: center;
    padding: 0;
  }

  .app-sidebar .brand {
    display: none;
  }

  .app-sidebar .brand,
  .app-sidebar .nav-item,
  .app-sidebar .local-status,
  .app-sidebar .user-status {
    justify-content: center;
    padding-inline: 0;
  }

  .app-sidebar .brand-name,
  .app-sidebar .nav-group__title,
  .app-sidebar .nav-item span,
  .app-sidebar .local-status span,
  .app-sidebar .user-meta,
  .app-sidebar .user-status .icon-button,
  .app-sidebar .history-panel {
    display: none;
  }

  .app-sidebar .local-status,
  .app-sidebar .user-status {
    flex: 0 0 var(--control-height);
    width: var(--control-height);
  }

  .app-sidebar .sidebar-footer {
    flex-direction: column;
    padding: var(--space-2);
  }

  .app-sidebar .sidebar-footer__actions {
    display: none;
  }

  .app-sidebar .sidebar-action {
    padding: var(--space-1) var(--space-2);
  }

  .app-sidebar .new-conversation-btn {
    justify-content: center;
    height: var(--control-height);
    padding: 0;
    border-radius: var(--radius-control);
  }

  .app-sidebar .new-conversation-btn span {
    display: none;
  }

  .app-sidebar .sidebar-toggle,
  .app-sidebar .sidebar-settings-button {
    flex: 0 0 var(--control-height);
    width: var(--control-height);
    height: var(--control-height);
  }
}
</style>
