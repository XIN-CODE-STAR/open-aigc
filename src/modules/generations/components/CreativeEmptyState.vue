<script setup lang="ts">
/**
 * CreativeEmptyState：创意工坊居中欢迎页。
 *
 * 参考通义千问风格的简约设计：
 * - 问候语（根据时段动态）
 * - 居中大输入框（带语音、图片上传图标）
 * - 创作模式胶囊按钮
 * - 发现区域（示例提示词卡片）
 *
 * 空状态时作为主页面居中展示，替代传统的"无内容"提示。
 */
import { computed } from "vue";
import { ChevronDown, FolderOpen, Image, RotateCcw, Send, Sparkles, X } from "@lucide/vue";

import type { CredentialRecord } from "../../../bridge/credentials";
import CreationParameterBar from "./CreationParameterBar.vue";
import MemoryStatusBadge from "./MemoryStatusBadge.vue";

/** 创作模式项（原定义于 CreativeWorkspaceHeader，顶部栏移除后迁移至此）。 */
export interface CreationModeItem {
  id: "agent" | "image" | "video" | "music" | "voiceover" | "digital-human" | "motion";
  label: string;
  description: string;
  badge?: string;
}

type CreationMode = CreationModeItem["id"];

export interface ExamplePrompt {
  id: string;
  title: string;
  prompt: string;
  mode: CreationMode;
  icon: typeof Sparkles;
}

const props = defineProps<{
  currentMode: CreationModeItem;
  examples: ExamplePrompt[];
  isAgentMode: boolean;
  hasCredentials: boolean;
  /** 输入框内容（双向绑定）。 */
  promptText: string;
  /** 输入框占位符。 */
  placeholder: string;
  /** 是否正在发送。 */
  isSending: boolean;
  /** 是否可发送。 */
  canSend: boolean;
  /** 主创作模式列表。 */
  primaryModes: { id: string; label: string; icon: unknown; description: string }[];
  /** 扩展模式列表。 */
  extraModes: { id: string; label: string; icon: unknown; description: string }[];
  /** 当前选中的创作模式 id。 */
  creationMode: string;
  /** "更多"下拉是否展开。 */
  showModeMenu: boolean;
  /** 凭据列表。 */
  credentials: CredentialRecord[];
  /** 当前选中的凭据 id。 */
  selectedCredentialId: string;
  /** 创作参数：比例。 */
  aspectRatio: string;
  /** 创作参数：时长。 */
  videoDuration: string;
  /** 创作参数：分辨率。 */
  resolution: string;
  /** 创作参数：帧率。 */
  frameRate: string;
  /** 记忆服务状态。 */
  memoryStatus?: "active" | "starting" | "inactive" | "unconfigured";
  /** 项目输出目录路径。 */
  projectDirectory: string | null;
  /** 项目显示名称。 */
  projectDisplayName: string;
  /** 已上传的参考图/附件列表。 */
  referenceImages?: { name: string; dataUrl: string; mimeType: string }[];
  /** 是否正在拖拽文件（由父组件 document 级别事件驱动）。 */
  isDraggingFile?: boolean;
}>();

const emit = defineEmits<{
  pick: [example: ExamplePrompt];
  "configure-credentials": [];
  "update:prompt-text": [value: string];
  send: [];
  "prompt-keydown": [event: KeyboardEvent];
  "update:creation-mode": [value: string];
  "update:show-mode-menu": [value: boolean];
  "select-credential": [id: string];
  "update:aspect-ratio": [value: string];
  "update:video-duration": [value: string];
  "update:resolution": [value: string];
  "update:frame-rate": [value: string];
  "drop-files": [files: FileList];
  "remove-reference": [index: number];
  "select-directory": [];
  "reset-directory": [];
}>();

/** 是否正在拖拽文件（由父组件 document 级别事件驱动，不需要本地处理）。 */
const isDragging = computed(() => props.isDraggingFile ?? false);

function removeReference(index: number): void {
  emit("remove-reference", index);
}

/** 根据当前时间生成问候语。 */
const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 6) return "夜深了";
  if (hour < 12) return "早上好";
  if (hour < 14) return "中午好";
  if (hour < 18) return "下午好";
  return "晚上好";
});

function onInput(event: Event): void {
  emit("update:prompt-text", (event.target as HTMLTextAreaElement).value);
}

function onKeydown(event: KeyboardEvent): void {
  emit("prompt-keydown", event);
}

function onSend(): void {
  emit("send");
}

function onSelectMode(mode: { id: string }): void {
  emit("update:creation-mode", mode.id);
}

function toggleMore(): void {
  emit("update:show-mode-menu", !props.showModeMenu);
}

function pickExample(example: ExamplePrompt): void {
  emit("pick", example);
}
</script>

<template>
  <div class="welcome">
    <!-- 问候语 -->
    <h1 class="welcome__greeting">
      Hi，{{ greeting }}
      <span class="welcome__sparkle">
        <Sparkles :size="20" />
      </span>
    </h1>

    <!-- 资源栏 + 输入框组合 -->
    <div class="welcome__input-group">
      <!-- 资源栏：文件夹选择 + 已上传图片（同一行） -->
      <div class="welcome__resources">
        <button
          type="button"
          class="welcome__resource-btn"
          :title="projectDirectory ? '更换输出目录' : '选择输出目录'"
          @click="emit('select-directory')"
        >
          <FolderOpen :size="14" />
          <span class="welcome__resource-label">
            {{ projectDirectory ? projectDisplayName : "打开" }}
          </span>
          <button
            v-if="projectDirectory"
            type="button"
            class="welcome__resource-reset"
            title="恢复默认目录"
            @click.stop="emit('reset-directory')"
          >
            <RotateCcw :size="10" />
          </button>
        </button>
        <!-- 已上传图片 chips（与文件夹按钮同行、同尺寸） -->
        <template v-if="referenceImages && referenceImages.length > 0">
          <div v-for="(img, idx) in referenceImages" :key="idx" class="welcome__ref-chip">
            <div v-if="img.mimeType.startsWith('image/')" class="welcome__ref-thumb">
              <img :src="img.dataUrl" :alt="img.name" />
            </div>
            <div v-else class="welcome__ref-thumb welcome__ref-thumb--file">
              <Image :size="12" />
            </div>
            <span class="welcome__ref-name">{{ img.name }}</span>
            <button
              type="button"
              class="welcome__ref-remove"
              title="移除"
              @click.stop="removeReference(idx)"
            >
              <X :size="10" />
            </button>
          </div>
        </template>
      </div>

      <!-- 居中输入框 -->
      <div class="welcome__prompt" :class="{ 'is-dragover': isDragging }">
        <textarea
          class="welcome__input"
          :value="promptText"
          :placeholder="isDragging ? '松开以添加参考图片…' : placeholder"
          rows="1"
          :disabled="isSending"
          @input="onInput"
          @keydown="onKeydown"
        />
        <button type="button" class="welcome__send-btn" :disabled="!canSend" @click="onSend">
          <Send :size="16" />
        </button>
      </div>

      <!-- 拖拽提示（输入框下方） -->
      <Transition name="fade">
        <p v-if="isDragging" class="welcome__drop-hint">
          <Image :size="12" />
          拖入图片或文档作为参考素材
        </p>
      </Transition>
    </div>

    <!-- 创作模式胶囊 -->
    <div class="welcome__modes">
      <button
        v-for="mode in primaryModes"
        :key="mode.id"
        type="button"
        class="welcome__pill"
        :class="{ 'is-active': creationMode === mode.id }"
        @click="onSelectMode(mode)"
      >
        {{ mode.label }}
      </button>

      <div class="welcome__more-wrap">
        <button
          type="button"
          class="welcome__pill"
          :class="{ 'is-active': extraModes.some((m) => m.id === creationMode) }"
          @click="toggleMore"
        >
          更多
          <ChevronDown :size="12" />
        </button>
        <Transition name="menu">
          <div v-if="showModeMenu" class="welcome__more-menu">
            <button
              v-for="mode in extraModes"
              :key="mode.id"
              type="button"
              class="welcome__more-item"
              @click="
                onSelectMode(mode);
                emit('update:show-mode-menu', false);
              "
            >
              <component :is="mode.icon" :size="14" />
              <span>{{ mode.label }}</span>
            </button>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 创作参数（图片/视频模式下显示） -->
    <Transition name="params">
      <CreationParameterBar
        v-if="creationMode === 'image' || creationMode === 'video'"
        :creation-mode="creationMode as any"
        :aspect-ratio="aspectRatio"
        :video-duration="videoDuration"
        :resolution="resolution"
        :frame-rate="frameRate"
        @update:aspect-ratio="(v: string) => emit('update:aspect-ratio', v)"
        @update:video-duration="(v: string) => emit('update:video-duration', v)"
        @update:resolution="(v: string) => emit('update:resolution', v)"
        @update:frame-rate="(v: string) => emit('update:frame-rate', v)"
      />
    </Transition>

    <!-- 发现区域 -->
    <section v-if="examples.length > 0" class="welcome__discover">
      <h2 class="welcome__discover-title">
        <Sparkles :size="14" />
        发现
      </h2>
      <div class="welcome__cards">
        <button
          v-for="example in examples"
          :key="example.id"
          type="button"
          class="welcome__card"
          @click="pickExample(example)"
        >
          <div class="welcome__card-icon">
            <component :is="example.icon" :size="16" />
          </div>
          <span class="welcome__card-title">{{ example.title }}</span>
          <span class="welcome__card-desc">{{ example.prompt.slice(0, 24) }}…</span>
        </button>
      </div>
    </section>

    <!-- 记忆状态 -->
    <MemoryStatusBadge v-if="memoryStatus" :status="memoryStatus" />

    <!-- 未配置凭据提示 -->
    <p v-if="!hasCredentials" class="welcome__hint">
      尚未配置 AI 模型，请前往
      <button type="button" class="welcome__hint-link" @click="emit('configure-credentials')">
        管理中心
      </button>
      配置。
    </p>
  </div>
</template>

<style scoped>
/* ═══════════════════════════════════════════
   居中欢迎页 · 简约浅色风格
   ═══════════════════════════════════════════ */

.welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--space-8) var(--space-6);
  gap: var(--space-6);
}

/* —— 问候语 —— */
.welcome__greeting {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin: 0;
  font-family: var(--font-display);
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--color-text);
}

.welcome__sparkle {
  display: inline-flex;
  color: var(--color-accent);
}

/* —— 资源栏 + 输入框组合容器 —— */
.welcome__input-group {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  max-width: 780px;
  gap: 6px;
}

/* —— 资源栏：文件夹选择 + 后续扩展区域 —— */
.welcome__resources {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: 0;
}

.welcome__resource-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.welcome__resource-btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border);
  color: var(--color-text);
}

.welcome__resource-label {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.welcome__resource-reset {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}

.welcome__resource-reset:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

/* —— 已上传参考图片（与文件夹按钮同行同高） —— */
.welcome__ref-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 4px 0 0;
  border: 1px solid var(--color-border-subtle);
  border-radius: 6px;
  background: var(--color-surface);
}

.welcome__ref-thumb {
  width: 22px;
  height: 22px;
  border-radius: 5px;
  overflow: hidden;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-accent-soft);
}

.welcome__ref-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.welcome__ref-thumb--file {
  color: var(--color-accent);
}

.welcome__ref-name {
  font-size: 11px;
  color: var(--color-text-secondary);
  max-width: 64px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.welcome__ref-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}

.welcome__ref-remove:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

/* —— 输入框下方拖拽提示 —— */
.welcome__drop-hint {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  margin: 0;
  padding: var(--space-1) 0;
  color: var(--color-accent);
  font-size: 12px;
  font-weight: 500;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* —— 输入框容器 —— */
.welcome__prompt {
  display: flex;
  align-items: center;
  width: 100%;
  max-width: 780px;
  min-height: 52px;
  padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
  gap: var(--space-2);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  transition:
    border-color var(--duration-fast) var(--ease-out),
    background var(--duration-fast) var(--ease-out);
}

.welcome__prompt:focus-within {
  border-color: var(--color-border);
}

.welcome__prompt.is-dragover {
  border-color: var(--color-accent);
  border-style: dashed;
  background: var(--color-accent-soft);
}

/* —— 输入框 —— */
.welcome__input {
  flex: 1;
  min-width: 0;
  min-height: 24px;
  max-height: 120px;
  padding: 12px 0;
  border: none;
  background: transparent;
  color: var(--color-text);
  font-size: 15px;
  line-height: 1.5;
  resize: none;
  outline: none;
}

.welcome__input::placeholder {
  color: var(--color-text-tertiary);
}

/* —— 发送按钮 —— */
.welcome__send-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  cursor: pointer;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.welcome__send-btn:hover:not(:disabled) {
  opacity: 0.85;
  transform: scale(1.05);
}

.welcome__send-btn:active:not(:disabled) {
  transform: scale(0.95);
}

.welcome__send-btn:disabled {
  background: var(--color-surface-hover);
  color: var(--color-text-disabled);
  cursor: not-allowed;
}

/* —— 创作模式胶囊 —— */
.welcome__modes {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
  justify-content: center;
}

.welcome__pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 32px;
  padding: 0 var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.welcome__pill:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border);
  color: var(--color-text);
}

.welcome__pill.is-active {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

/* —— 更多下拉 —— */
.welcome__more-wrap {
  position: relative;
}

.welcome__more-menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  min-width: 140px;
  padding: var(--space-1);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  box-shadow: var(--shadow-lg);
  z-index: 10;
}

.welcome__more-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text);
  font-size: 13px;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.welcome__more-item:hover {
  background: var(--color-surface-hover);
}

/* —— 发现区域 —— */
.welcome__discover {
  width: 100%;
  max-width: 780px;
}

.welcome__discover-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0 0 var(--space-3);
  color: var(--color-text-secondary);
  font-size: var(--text-footnote);
  font-weight: 500;
}

.welcome__cards {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--space-2);
}

.welcome__card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  cursor: pointer;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
  text-align: left;
}

.welcome__card:hover {
  border-color: var(--color-border);
  box-shadow: var(--shadow-sm);
}

.welcome__card-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-control);
  background: var(--color-accent-soft);
  color: var(--color-accent);
  margin-bottom: var(--space-1);
}

.welcome__card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.welcome__card-desc {
  font-size: 12px;
  color: var(--color-text-tertiary);
  line-height: 1.4;
}

/* —— 未配置提示 —— */
.welcome__hint {
  margin: 0;
  font-size: var(--text-caption);
  color: var(--color-text-tertiary);
}

.welcome__hint-link {
  border: none;
  background: transparent;
  color: var(--color-accent);
  font: inherit;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.welcome__hint-link:hover {
  opacity: 0.8;
}

/* —— 菜单过渡 —— */
.menu-enter-active,
.menu-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(4px);
}

.menu-enter-to,
.menu-leave-from {
  opacity: 1;
  transform: translateX(-50%) translateY(0);
}

/* —— 响应式 —— */
@media (max-width: 640px) {
  .welcome {
    padding: var(--space-6) var(--space-4);
    gap: var(--space-5);
  }

  .welcome__greeting {
    font-size: 22px;
  }

  .welcome__cards {
    grid-template-columns: 1fr;
  }
}

@media (prefers-reduced-motion: reduce) {
  .welcome__card:hover {
    box-shadow: none;
  }
}

/* —— 参数行过渡 —— */
.params-enter-active,
.params-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    max-height var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
  overflow: hidden;
}

.params-enter-from,
.params-leave-to {
  opacity: 0;
  max-height: 0;
  transform: translateY(-4px);
}

.params-enter-to,
.params-leave-from {
  opacity: 1;
  max-height: 80px;
  transform: translateY(0);
}
</style>
