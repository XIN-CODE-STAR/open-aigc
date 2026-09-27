<script setup lang="ts">
/**
 * ConversationHistoryPanel：历史对话面板（自包含）。
 *
 * - 按工作目录分组展示对话，目录可折叠
 * - 右键菜单：重命名 / 删除（删除带二次确认）
 * - 监听全局刷新事件，与 GenerationsPage 保持同步
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  ChevronRight,
  Folder,
  LoaderCircle,
  MessageSquareText,
  Pencil,
  RefreshCcw,
  Trash2,
} from "@lucide/vue";
import { RouterLink, useRoute, useRouter } from "vue-router";

import {
  deleteConversation,
  listConversations,
  renameConversation,
  type ConversationRecord,
} from "../../bridge/agent";
import { useToast } from "../../shared/ui/useToast";
import { useConversationWorkspaces } from "../composables/useConversationWorkspaces";

const route = useRoute();
const router = useRouter();
const toast = useToast();
const conversationWorkspaces = useConversationWorkspaces();

const REFRESH_EVENT = "aigc-agent-conversations-updated";

interface ConversationHistoryGroup {
  id: string;
  label: string;
  conversations: ConversationRecord[];
}

const conversations = ref<ConversationRecord[]>([]);
const phase = ref<"idle" | "loading" | "ready" | "error">("idle");
const errorText = ref<string | null>(null);

const groups = computed<ConversationHistoryGroup[]>(() => {
  const map = new Map<string, ConversationRecord[]>();
  const sorted = [...conversations.value].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  for (const conversation of sorted) {
    const label = groupLabel(conversation);
    const items = map.get(label);
    if (items) items.push(conversation);
    else map.set(label, [conversation]);
  }
  return [...map.entries()].map(([label, list]) => ({
    id: label,
    label,
    conversations: list,
  }));
});

const hasHistory = computed(() => groups.value.length > 0);
const activeConversationId = computed(() =>
  typeof route.query.conversation === "string" ? route.query.conversation : null,
);

/** 目录分组展开状态。默认收起；激活会话所在目录自动展开。 */
const expandedGroups = ref<Record<string, boolean>>({});

function isExpanded(groupId: string): boolean {
  return expandedGroups.value[groupId] === true;
}

function toggleGroup(groupId: string): void {
  expandedGroups.value[groupId] = !isExpanded(groupId);
}

function groupLabel(conversation: ConversationRecord): string {
  return conversationWorkspaces.getWorkspaceName(conversation.id) ?? "OPEN AIGC";
}

watch(
  () => [activeConversationId.value, conversations.value] as const,
  () => {
    const activeId = activeConversationId.value;
    if (!activeId) return;
    const active = conversations.value.find((item) => item.id === activeId);
    if (!active) return;
    expandedGroups.value[groupLabel(active)] = true;
  },
  { immediate: true },
);

async function refresh(): Promise<void> {
  phase.value = "loading";
  errorText.value = null;
  try {
    conversations.value = await listConversations();
    phase.value = "ready";
    const expanded = { ...expandedGroups.value };
    for (const item of conversations.value) {
      expanded[groupLabel(item)] = true;
    }
    expandedGroups.value = expanded;
  } catch (error) {
    conversations.value = [];
    phase.value = "error";
    errorText.value = error instanceof Error ? error.message : "历史对话加载失败。";
  }
}

function onRefreshEvent(): void {
  void refresh();
}

/* ── 右键菜单：重命名 / 删除 ── */

interface MenuState {
  x: number;
  y: number;
  conversation: ConversationRecord;
  mode: "menu" | "confirm-delete";
}

const menu = ref<MenuState | null>(null);
const renamingId = ref<string | null>(null);
const renamingTitle = ref("");
const renameInput = ref<HTMLInputElement | null>(null);

const menuLeft = computed(() => {
  const x = menu.value?.x ?? 0;
  return Math.max(8, Math.min(x, window.innerWidth - 190));
});

const menuTop = computed(() => {
  const y = menu.value?.y ?? 0;
  return Math.max(8, Math.min(y, window.innerHeight - 150));
});

function openMenu(event: MouseEvent, conversation: ConversationRecord): void {
  event.preventDefault();
  event.stopPropagation();
  menu.value = { x: event.clientX, y: event.clientY, conversation, mode: "menu" };
}

function closeMenu(): void {
  menu.value = null;
}

function onWindowContextMenu(event: MouseEvent): void {
  if (!event.defaultPrevented) closeMenu();
}

function onMenuKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") closeMenu();
}

watch(
  () => menu.value !== null,
  (open) => {
    if (open) {
      window.addEventListener("click", closeMenu);
      window.addEventListener("contextmenu", onWindowContextMenu);
      window.addEventListener("keydown", onMenuKeydown);
      window.addEventListener("resize", closeMenu);
    } else {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("contextmenu", onWindowContextMenu);
      window.removeEventListener("keydown", onMenuKeydown);
      window.removeEventListener("resize", closeMenu);
    }
  },
);

function beginDeleteConfirm(): void {
  if (!menu.value) return;
  menu.value = { ...menu.value, mode: "confirm-delete" };
}

async function confirmDelete(): Promise<void> {
  const target = menu.value?.conversation;
  closeMenu();
  if (!target) return;
  try {
    await deleteConversation(target.id);
    conversationWorkspaces.remove(target.id);
    if (activeConversationId.value === target.id) {
      void router.push({ path: "/generations" });
    }
    await refresh();
    window.dispatchEvent(new CustomEvent(REFRESH_EVENT));
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
    window.dispatchEvent(new CustomEvent(REFRESH_EVENT));
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "重命名失败。");
  }
}

function cancelRename(): void {
  renamingId.value = null;
}

onMounted(() => {
  window.addEventListener(REFRESH_EVENT, onRefreshEvent);
  void refresh();
});

onUnmounted(() => {
  window.removeEventListener(REFRESH_EVENT, onRefreshEvent);
});
</script>

<template>
  <section class="history" aria-label="历史对话">
    <div class="history__header">
      <h2 class="history__title">历史对话</h2>
      <button
        class="history__refresh"
        type="button"
        title="刷新历史对话"
        aria-label="刷新历史对话"
        :disabled="phase === 'loading'"
        @click="refresh"
      >
        <RefreshCcw :size="13" :class="{ 'is-spinning': phase === 'loading' }" />
      </button>
    </div>

    <div v-if="phase === 'loading' && !hasHistory" class="history__empty">
      <LoaderCircle class="is-spinning" :size="14" />
      <span>正在加载历史…</span>
    </div>
    <div v-else-if="phase === 'error'" class="history__empty is-error">
      <MessageSquareText :size="14" />
      <span>{{ errorText }}</span>
    </div>
    <div v-else-if="!hasHistory" class="history__empty">
      <MessageSquareText :size="14" />
      <span>暂无历史对话</span>
    </div>

    <div v-for="group in groups" :key="group.id" class="history__group">
      <button
        class="history__project"
        type="button"
        :aria-expanded="isExpanded(group.id)"
        :title="group.label"
        @click="toggleGroup(group.id)"
      >
        <Folder :size="15" :stroke-width="1.8" />
        <span class="history__project-label">{{ group.label }}</span>
        <span class="history__project-count">{{ group.conversations.length }}</span>
        <ChevronRight
          :size="13"
          class="history__chevron"
          :class="{ 'is-expanded': isExpanded(group.id) }"
        />
      </button>
      <template v-if="isExpanded(group.id)">
        <template v-for="conversation in group.conversations" :key="conversation.id">
          <div v-if="renamingId === conversation.id" class="history__item is-editing">
            <input
              :ref="bindRenameInput"
              v-model="renamingTitle"
              class="history__rename"
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
            class="history__item"
            :class="{ 'is-active': activeConversationId === conversation.id }"
            :to="{ path: '/generations', query: { conversation: conversation.id } }"
            :title="conversation.title"
            @contextmenu="openMenu($event, conversation)"
          >
            <span>{{ conversation.title }}</span>
          </RouterLink>
        </template>
      </template>
    </div>

    <!-- 右键菜单（重命名 / 删除） -->
    <div
      v-if="menu"
      class="history__menu"
      role="menu"
      :style="{ left: `${menuLeft}px`, top: `${menuTop}px` }"
      @click.stop
      @contextmenu.prevent.stop
    >
      <template v-if="menu.mode === 'menu'">
        <button
          type="button"
          class="history__menu-item"
          role="menuitem"
          @click.stop="startRename(menu.conversation)"
        >
          <Pencil :size="13" :stroke-width="1.8" />
          <span>重命名</span>
        </button>
        <button
          type="button"
          class="history__menu-item is-danger"
          role="menuitem"
          @click.stop="beginDeleteConfirm"
        >
          <Trash2 :size="13" :stroke-width="1.8" />
          <span>删除对话</span>
        </button>
      </template>
      <template v-else>
        <p class="history__menu-hint">删除后不可恢复，确认删除该对话？</p>
        <div class="history__menu-actions">
          <button type="button" class="history__menu-confirm is-danger" @click.stop="confirmDelete">
            删除
          </button>
          <button type="button" class="history__menu-confirm" @click.stop="closeMenu">取消</button>
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.history {
  display: flex;
  min-height: 0;
  padding: var(--space-2) var(--space-3) var(--space-3);
  flex: 1;
  flex-direction: column;
  gap: var(--space-1);
  overflow: hidden auto;
}

.history__header {
  display: flex;
  height: 24px;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.history__title {
  flex: 1;
  margin: 0;
  padding-inline: var(--space-2) 0;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  font-weight: var(--font-weight-medium);
  line-height: 20px;
  letter-spacing: 0.02em;
  text-transform: uppercase;
  white-space: nowrap;
}

.history__refresh {
  display: inline-grid;
  width: 24px;
  height: 24px;
  flex: 0 0 24px;
  place-items: center;
  color: var(--color-text-tertiary);
  border: none;
  border-radius: 7px;
  background: transparent;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.history__refresh:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history__refresh:disabled {
  cursor: wait;
  opacity: 0.7;
}

.history__empty {
  display: flex;
  min-height: 36px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  font-size: var(--text-footnote);
  line-height: 18px;
}

.history__empty.is-error {
  color: var(--color-danger);
}

.history__group {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.history__project,
.history__item {
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

.history__project {
  width: 100%;
  height: 30px;
  padding: 0 var(--space-2);
  gap: var(--space-2);
  border: none;
  background: transparent;
  font-family: inherit;
  font-size: var(--text-footnote);
  font-weight: var(--font-weight-medium);
  text-align: left;
  cursor: pointer;
}

.history__project:hover,
.history__item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history__project > svg:first-of-type {
  flex: 0 0 15px;
}

.history__project span,
.history__item span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history__project-label {
  flex: 1;
}

.history__project-count {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-size: var(--text-caption);
  font-weight: var(--font-weight-regular);
}

.history__chevron {
  color: var(--color-text-tertiary);
  transition: transform var(--duration-fast) var(--ease-out);
}

.history__chevron.is-expanded {
  transform: rotate(90deg);
}

.history__item {
  height: 28px;
  margin-left: 24px;
  padding: 0 var(--space-2);
  font-size: var(--text-footnote);
}

.history__item.is-active {
  color: var(--color-text);
  background: var(--color-surface-hover);
  font-weight: var(--font-weight-medium);
}

.history__item.is-editing {
  padding: 0 var(--space-1);
  background: var(--color-surface-hover);
}

.history__rename {
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

/* —— 右键菜单（fixed 跟随鼠标） —— */
.history__menu {
  position: fixed;
  z-index: var(--z-dropdown);
  display: flex;
  min-width: 168px;
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

.history__menu-item {
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

.history__menu-item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history__menu-item.is-danger:hover {
  color: var(--color-danger);
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
}

.history__menu-hint {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  color: var(--color-text-secondary);
  font-size: var(--text-caption);
  line-height: var(--line-height-normal);
}

.history__menu-actions {
  display: flex;
  gap: var(--space-2);
  padding: 0 var(--space-3) var(--space-2);
}

.history__menu-confirm {
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

.history__menu-confirm:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.history__menu-confirm.is-danger {
  color: #ffffff;
  border-color: transparent;
  background: var(--color-danger);
}

.history__menu-confirm.is-danger:hover {
  filter: brightness(1.08);
}

.is-spinning {
  animation: history-spin 900ms linear infinite;
}

@keyframes history-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
