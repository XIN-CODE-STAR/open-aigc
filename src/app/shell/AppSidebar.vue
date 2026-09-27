<script setup lang="ts">
/**
 * AppSidebar：左侧导航栏。
 *
 * 布局：品牌头 → 分组导航（工作区/资源）→ 新建对话 → 历史对话 → 底部状态。
 * 默认 68px 图标窄轨，钉住展开为完整面板（preferences.navigationExpanded）。
 */
import { computed } from "vue";
import {
  Archive,
  BookOpenText,
  Bot,
  HardDrive,
  Images,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Puzzle,
  Settings,
  Sparkles,
} from "@lucide/vue";
import { RouterLink } from "vue-router";

import appIconUrl from "../../../assets/app-icon.svg";
import ConversationHistoryPanel from "./ConversationHistoryPanel.vue";
import { usePreferencesStore } from "../stores/preferences";
import { useProjectDirectory } from "../stores/projectDirectory";
import { useWorkspaceStore } from "../stores/workspace";

const preferences = usePreferencesStore();
const workspace = useWorkspaceStore();
const projectDir = useProjectDirectory();

const expanded = computed(() => preferences.navigationExpanded);

const navGroups = [
  {
    label: "工作区",
    items: [
      { path: "/generations", label: "创意工坊", icon: Sparkles },
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
        <RouterLink
          v-for="item in group.items"
          :key="item.path"
          class="nav-item"
          :to="item.path"
          :title="expanded ? undefined : item.label"
        >
          <component :is="item.icon" :size="17" :stroke-width="1.8" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </div>
    </nav>

    <div class="sidebar__action">
      <RouterLink
        class="new-conversation-btn"
        to="/generations"
        :title="expanded ? undefined : '新建对话'"
      >
        <Plus :size="15" :stroke-width="2" />
        <span>新建对话</span>
      </RouterLink>
    </div>

    <ConversationHistoryPanel v-if="expanded" />

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
.sidebar__nav {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2) var(--space-2) 0;
  flex-direction: column;
  gap: var(--space-2);
  overflow: visible;
}

.nav-group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.nav-group__title {
  height: 20px;
  margin: 0 0 2px;
  padding: 0 var(--space-2);
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 10px;
  font-weight: var(--font-weight-medium);
  line-height: 20px;
  letter-spacing: 0.06em;
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
  border-radius: var(--radius-control);
  font-size: var(--text-subhead);
  font-weight: var(--font-weight-regular);
  line-height: 20px;
  text-decoration: none;
  white-space: nowrap;
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

/* 激活态：低饱和选中面 + 左侧品牌指示条 */
.nav-item.router-link-active {
  color: var(--color-text);
  background: var(--color-surface-hover);
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

/* —— 新建对话 —— */
.sidebar__action {
  display: flex;
  flex: 0 0 auto;
  padding: var(--space-2);
}

.new-conversation-btn {
  display: flex;
  width: 100%;
  height: 34px;
  padding: 0 var(--space-2);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-on-accent);
  border: none;
  border-radius: var(--radius-control);
  background: linear-gradient(135deg, var(--color-accent), var(--aurora-cyan));
  font-size: var(--text-subhead);
  font-weight: var(--font-weight-semibold);
  text-decoration: none;
  cursor: pointer;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.new-conversation-btn:hover {
  opacity: 0.92;
  box-shadow: 0 2px 10px var(--aurora-glow-1);
}

.new-conversation-btn:active {
  opacity: 0.82;
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

/* —— 通用图标按钮 —— */
.icon-button {
  display: inline-grid;
  height: 30px;
  width: 30px;
  flex: 0 0 30px;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: 8px;
  background: transparent;
  cursor: pointer;
  text-decoration: none;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.icon-button:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
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
