<script setup lang="ts">
/**
 * AppSidebar：左侧导航栏。
 *
 * 布局：品牌头 → 分组导航（工作区/资源）→ 新建对话 → 底部状态。
 * 历史对话不再挂在侧栏，已集中到「任务」页查看（见 modules/tasks）。
 * 默认 68px 图标窄轨，钉住展开为完整面板（preferences.navigationExpanded）。
 */
import { computed, onMounted, onUnmounted, type Component } from "vue";
import {
  Archive,
  BookOpenText,
  Bot,
  Folder,
  HardDrive,
  Images,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Puzzle,
  Settings,
  Sparkles,
  Waypoints,
} from "@lucide/vue";
import { RouterLink, useRoute } from "vue-router";

import appIconUrl from "../../../assets/app-icon.svg";
import type { ConversationRecord } from "../../bridge/agent";
import { usePreferencesStore } from "../stores/preferences";
import { useProjectDirectory } from "../stores/projectDirectory";
import { useWorkspaceStore } from "../stores/workspace";
import { useShellActionsStore } from "../stores/shellActions";
import { useMemoryCanvasStore } from "../stores/memoryCanvas";
import { useConversationWorkspaces } from "../composables/useConversationWorkspaces";
import { useConversationList } from "../composables/useConversationList";
import { formatRelativeTime } from "../../shared/utils/time";

const preferences = usePreferencesStore();
const workspace = useWorkspaceStore();
const projectDir = useProjectDirectory();
const shellActions = useShellActionsStore();
const memoryCanvas = useMemoryCanvasStore();
const conversationWorkspaces = useConversationWorkspaces();
const {
  conversations: recentConversations,
  refresh: refreshConversations,
  ensureLoaded,
} = useConversationList();

const expanded = computed(() => preferences.navigationExpanded);
const route = useRoute();

/**
 * 是否为「当前正在查看的任务」。
 * 注意：任务项都指向 /generations（仅 query 不同），router-link-active 会
 * 按路径前缀把每一行都判为激活，导致所有行被染色 —— 因此按 query 显式判断。
 */
function isCurrentTask(conversationId: string): boolean {
  return route.path === "/generations" && route.query.conversation === conversationId;
}

/* ──────────────────────────────────────────────
 *  最近任务列表（对齐参考设计：任务 / 空间 分组）
 * ────────────────────────────────────────────── */

/** 未登记工作目录的对话归入应用自身（与任务页同一口径）。 */
const DEFAULT_GROUP = "OPEN AIGC";
/** 侧栏空间有限：任务最多 6 条，每个空间最多 4 条子项。 */
const MAX_LOOSE_TASKS = 6;
const MAX_SPACE_TASKS = 4;

/** 是否为「未归属任何工作目录」的对话（而非恰好叫该名字的真实空间）。 */
function isLooseTask(conversation: ConversationRecord): boolean {
  return conversationWorkspaces.getWorkspaceName(conversation.id) === null;
}

const taskGroups = computed(() => {
  const map = new Map<string, ConversationRecord[]>();
  for (const item of [...recentConversations.value].sort((a, b) =>
    b.updatedAt.localeCompare(a.updatedAt),
  )) {
    const label = conversationWorkspaces.getWorkspaceName(item.id) ?? DEFAULT_GROUP;
    const list = map.get(label);
    if (list) list.push(item);
    else map.set(label, [item]);
  }
  return [...map.entries()].map(([label, items]) => ({ label, items }));
});

const looseTasks = computed(() =>
  [...recentConversations.value]
    .filter(isLooseTask)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .slice(0, MAX_LOOSE_TASKS),
);

const spaceGroups = computed(() =>
  taskGroups.value
    // label 为 DEFAULT_GROUP 但整体都是未分组对话时，是「默认区」而非真实空间
    .filter((group) => group.label !== DEFAULT_GROUP || !group.items.every(isLooseTask))
    .map((group) => ({ ...group, items: group.items.slice(0, MAX_SPACE_TASKS) })),
);

/**
 * 会话列表在别处发生变化时（创意工坊新建、任务页改名/删除）重新拉取。
 * 列表是共享单例，刷新后侧栏与任务页同时更新，不会再各说各话。
 */
function onConversationsUpdated(): void {
  refreshConversations().catch(() => {
    // 侧栏列表是辅助信息，读取失败静默降级，不影响导航
  });
}

onMounted(() => {
  void ensureLoaded();
  window.addEventListener("aigc-agent-conversations-updated", onConversationsUpdated);
});

onUnmounted(() => {
  window.removeEventListener("aigc-agent-conversations-updated", onConversationsUpdated);
});

interface NavItem {
  path: string;
  label: string;
  icon: Component;
}

const navGroups: { label: string; items: NavItem[] }[] = [
  {
    label: "工作区",
    items: [
      { path: "/generations", label: "创意工坊", icon: Sparkles },
      { path: "/canvas", label: "工作记忆画布", icon: Waypoints },
      { path: "/assets", label: "资产库", icon: Images },
      { path: "/prompts", label: "提示词库", icon: BookOpenText },
    ],
  },
  {
    label: "资源",
    items: [
      { path: "/models", label: "模型", icon: Bot },
      { path: "/plugins", label: "插件", icon: Puzzle },
      { path: "/backup", label: "备份", icon: Archive },
    ],
  },
];

/** 切换工作区时关闭创意工坊内的画布浮层，避免下次回到创意工坊时残留。 */
function onNavItemClick(): void {
  memoryCanvas.close();
}
const localStatus = computed(() => {
  if (!workspace.isReady) {
    return workspace.errorMessage ? "本地异常" : "初始化中";
  }
  return projectDir.isCustom ? projectDir.displayName : "本地就绪";
});
</script>

<template>
  <aside
    class="sidebar material-vibrancy"
    :class="{ 'is-collapsed': !expanded }"
    aria-label="主导航"
  >
    <div class="sidebar__header">
      <div class="brand" :title="expanded ? undefined : 'OPEN AIGC'">
        <img :src="appIconUrl" alt="" />
        <span class="brand__name">OPEN AIGC</span>
      </div>
      <button
        class="icon-button sidebar__toggle"
        type="button"
        :aria-label="expanded ? '折叠导航' : '展开导航'"
        :title="expanded ? '折叠导航' : '展开导航'"
        @click="preferences.toggleNavigation"
      >
        <PanelLeftClose v-if="expanded" :size="16" />
        <PanelLeftOpen v-else :size="16" />
      </button>
    </div>

    <nav class="sidebar__nav" aria-label="主功能">
      <div v-for="group in navGroups" :key="group.label" class="nav-group">
        <h2 v-if="expanded" class="nav-group__title">{{ group.label }}</h2>
        <!-- 动作型项（工作记忆画布）用 button 而非 RouterLink：
             RouterLink 指向 /generations 会抹掉当前会话的 ?conversation= 参数。
             动态组件保证两类项仍按声明顺序排列。 -->
        <RouterLink
          v-for="item in group.items"
          :key="item.label"
          class="nav-item"
          :to="item.path"
          :title="expanded ? undefined : item.label"
          @click="onNavItemClick"
        >
          <component :is="item.icon" :size="17" :stroke-width="1.8" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </div>
    </nav>

    <!-- 最近任务：对齐参考设计（任务 / 空间 分组 + 相对时间），折叠时隐藏 -->
    <nav v-if="expanded" class="sidebar__tasks" aria-label="最近任务">
      <section v-if="looseTasks.length" class="sidebar-tasks__section">
        <h2 class="sidebar-tasks__title">
          任务 <span class="sidebar-tasks__count">({{ looseTasks.length }})</span>
        </h2>
        <RouterLink
          v-for="item in looseTasks"
          :key="item.id"
          class="sidebar-tasks__item"
          :class="{ 'is-current': isCurrentTask(item.id) }"
          :to="`/generations?conversation=${item.id}`"
          :title="item.title"
        >
          <span class="sidebar-tasks__item-title">{{ item.title }}</span>
          <span class="sidebar-tasks__item-time">{{ formatRelativeTime(item.updatedAt) }}</span>
        </RouterLink>
      </section>

      <section v-if="spaceGroups.length" class="sidebar-tasks__section">
        <h2 class="sidebar-tasks__title">
          空间 <span class="sidebar-tasks__count">({{ spaceGroups.length }})</span>
        </h2>
        <div v-for="group in spaceGroups" :key="group.label" class="sidebar-tasks__space">
          <div class="sidebar-tasks__space-head">
            <Folder :size="15" :stroke-width="1.8" />
            <span class="sidebar-tasks__space-name">{{ group.label }}</span>
          </div>
          <RouterLink
            v-for="item in group.items"
            :key="item.id"
            class="sidebar-tasks__item is-child"
            :class="{ 'is-current': isCurrentTask(item.id) }"
            :to="`/generations?conversation=${item.id}`"
            :title="item.title"
          >
            <span class="sidebar-tasks__item-title">{{ item.title }}</span>
            <span class="sidebar-tasks__item-time">{{ formatRelativeTime(item.updatedAt) }}</span>
          </RouterLink>
        </div>
      </section>

      <RouterLink
        v-if="looseTasks.length || spaceGroups.length"
        class="sidebar-tasks__more"
        to="/tasks"
      >
        查看全部任务
      </RouterLink>
    </nav>

    <div class="sidebar__action">
      <RouterLink
        class="new-conversation-btn"
        to="/generations"
        :title="expanded ? undefined : '新建对话'"
        @click="shellActions.requestNewConversation()"
      >
        <Plus :size="15" :stroke-width="2" />
        <span>新建对话</span>
      </RouterLink>
    </div>

    <div class="sidebar__footer">
      <div class="local-status" :title="expanded ? undefined : localStatus">
        <HardDrive :size="15" :stroke-width="1.8" />
        <span>{{ localStatus }}</span>
      </div>
      <RouterLink
        class="icon-button sidebar__settings"
        to="/settings"
        aria-label="设置"
        title="设置"
      >
        <Settings :size="16" :stroke-width="1.8" />
      </RouterLink>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
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

.sidebar.is-collapsed {
  flex-basis: var(--sidebar-compact-width);
  width: var(--sidebar-compact-width);
}

/* —— 品牌头 —— */
.sidebar__header {
  display: flex;
  height: var(--topbar-height);
  min-height: var(--topbar-height);
  padding: 0 var(--space-2) 0 var(--space-3);
  align-items: center;
  justify-content: space-between;
  gap: var(--space-1);
  background: var(--material-topbar);
  border-bottom: 1px solid var(--color-border-subtle);
}

.brand {
  display: flex;
  min-width: 0;
  flex: 0 1 auto;
  align-items: center;
  gap: var(--space-2);
  overflow: hidden;
  font-family: var(--font-display);
  font-size: var(--text-headline);
  font-weight: var(--font-weight-semibold);
  letter-spacing: -0.01em;
  white-space: nowrap;
}

.brand img {
  flex: 0 0 26px;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  box-shadow: var(--shadow-sm);
}

.brand__name {
  color: var(--color-text);
}

/* —— 分组导航 —— */
/* 历史面板移出侧栏后，由导航自身撑满剩余高度，把底部状态条压到底。 */
.sidebar__nav {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-3) var(--space-2) 0;
  flex-direction: column;
  gap: var(--space-5);
  overflow: visible;
}

/* —— 最近任务（对齐参考设计：任务 / 空间 分组 + 相对时间） —— */
.sidebar__tasks {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  padding: var(--space-4) var(--space-2) var(--space-2);
  flex-direction: column;
  gap: var(--space-3);
  overflow-y: auto;
}

.sidebar-tasks__section {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sidebar-tasks__title {
  display: flex;
  height: 24px;
  margin: 0;
  padding: 0 var(--space-2);
  align-items: center;
  gap: 4px;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: var(--font-weight-medium);
}

.sidebar-tasks__count {
  color: var(--color-text-tertiary);
}

.sidebar-tasks__space {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sidebar-tasks__space-head {
  display: flex;
  height: 30px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text);
  font-size: var(--text-footnote);
  font-weight: var(--font-weight-medium);
}

.sidebar-tasks__space-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar-tasks__item {
  display: flex;
  min-height: 30px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border-radius: var(--radius-control);
  text-decoration: none;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

/* 空间子项：相对目录行缩进 */
.sidebar-tasks__item.is-child {
  padding-left: calc(var(--space-2) + 23px);
}

/* 未选中行保持无底色；仅悬停与「当前查看」才有背景 */
.sidebar-tasks__item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.sidebar-tasks__item.is-current {
  color: var(--color-text);
  background: var(--color-surface-selected);
}

.sidebar-tasks__item-title {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  font-size: var(--text-footnote);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar-tasks__item-time {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-size: 10px;
  white-space: nowrap;
}

.sidebar-tasks__more {
  padding: 4px var(--space-2);
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  text-decoration: none;
}

.sidebar-tasks__more:hover {
  color: var(--color-text);
}

.nav-group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

/* Marvis 眉标：等宽 + 全大写 + 正字距 + 板岩色 */
.nav-group__title {
  height: 18px;
  margin: 0 0 var(--space-1);
  padding: 0 var(--space-2);
  overflow: hidden;
  color: var(--color-accent-text);
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 500;
  line-height: 18px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  white-space: nowrap;
}

.nav-item {
  position: relative;
  display: flex;
  width: 100%;
  height: 34px;
  min-height: 34px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  overflow: hidden;
  color: var(--color-text-secondary);
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  font-family: inherit;
  font-size: var(--text-subhead);
  font-weight: var(--font-weight-regular);
  line-height: 20px;
  text-align: left;
  text-decoration: none;
  white-space: nowrap;
  cursor: pointer;
  /* 焦点态由全局 :focus-visible 的 focus-ring 承担，避免 UA 默认黑框残留 */
  outline: none;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.nav-item:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.nav-item svg {
  flex: 0 0 17px;
}

/* 激活态：Marvis 用 accent-soft 着色选中行 + 左侧板岩指示条 */
.nav-item.router-link-active {
  color: var(--color-text);
  background: var(--color-accent-soft);
  font-weight: var(--font-weight-medium);
}

.nav-item.router-link-active::before {
  content: "";
  position: absolute;
  top: 8px;
  bottom: 8px;
  left: 0;
  width: 3px;
  border-radius: 0 2px 2px 0;
  background: var(--color-accent);
}

/* 动作型项已改为独立路由（/canvas），不再需要取消激活态的特殊处理 */

/* —— 新建对话（Marvis：主操作是药丸） —— */
.sidebar__action {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2);
}

.new-conversation-btn {
  display: flex;
  width: 100%;
  height: 36px;
  padding: 0 var(--space-4);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-accent-text);
  border: none;
  border-radius: var(--radius-pill);
  background: var(--color-accent-soft);
  font-family: var(--font-sans);
  font-size: var(--text-subhead);
  font-weight: var(--font-weight-semibold);
  text-decoration: none;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.new-conversation-btn:hover {
  background: color-mix(in oklab, var(--color-accent) 22%, transparent);
}

.new-conversation-btn:active {
  background: color-mix(in oklab, var(--color-accent) 30%, transparent);
}

/* 暗色下大面积纯白填充刺眼 → 保持「安静药丸」：微白底 + 发丝线 + 白字 */
:root[data-theme="dark"] .new-conversation-btn {
  color: var(--color-text);
  background: var(--color-fg-soft);
  border: 1px solid var(--color-border);
}

:root[data-theme="dark"] .new-conversation-btn:hover {
  background: color-mix(in oklab, oklch(0.96 0 0) 14%, transparent);
  border-color: var(--color-border-strong);
}

:root[data-theme="dark"] .new-conversation-btn:active {
  background: color-mix(in oklab, oklch(0.96 0 0) 18%, transparent);
}

.new-conversation-btn svg {
  flex: 0 0 15px;
}

/* —— 底部：状态 + 设置 —— */
.sidebar__footer {
  display: flex;
  min-height: 48px;
  padding: var(--space-2);
  align-items: center;
  gap: var(--space-1);
  border-top: 1px solid var(--color-border-subtle);
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
  color: var(--color-text-tertiary);
  font-size: var(--text-footnote);
  white-space: nowrap;
}

.local-status svg {
  flex: 0 0 15px;
}

.local-status span {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* —— 通用图标按钮（Marvis ICON_BTN：圆形幽灵按钮） —— */
.icon-button {
  display: inline-grid;
  height: 30px;
  width: 30px;
  flex: 0 0 30px;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  cursor: pointer;
  text-decoration: none;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.icon-button:hover {
  color: var(--color-text);
  background: var(--color-fg-soft);
}

.sidebar__settings.router-link-active {
  color: var(--color-accent);
  background: var(--color-accent-soft);
}

/* —— 折叠窄轨态 —— */
.sidebar.is-collapsed .sidebar__header {
  flex-direction: column;
  justify-content: center;
  padding: 0 var(--space-1);
  gap: var(--space-1);
}

.sidebar.is-collapsed .brand {
  display: none;
}

.sidebar.is-collapsed .brand img {
  display: none;
}

.sidebar.is-collapsed .sidebar__nav {
  padding-inline: var(--space-1);
}

.sidebar.is-collapsed .nav-item {
  justify-content: center;
  padding-inline: 0;
}

.sidebar.is-collapsed .nav-item span {
  display: none;
}

.sidebar.is-collapsed .nav-item.router-link-active::before {
  top: 6px;
  bottom: 6px;
  left: -2px;
}

.sidebar.is-collapsed .sidebar__action {
  padding-inline: var(--space-1);
}

.sidebar.is-collapsed .new-conversation-btn {
  padding: 0;
}

.sidebar.is-collapsed .new-conversation-btn span {
  display: none;
}

.sidebar.is-collapsed .local-status {
  flex: 0 0 30px;
  width: 30px;
  justify-content: center;
  padding: 0;
}

.sidebar.is-collapsed .local-status span {
  display: none;
}

/* 窄轨下设置按钮跟随折叠尺寸 */
.sidebar.is-collapsed .sidebar__settings {
  height: 30px;
  width: 30px;
}
</style>
