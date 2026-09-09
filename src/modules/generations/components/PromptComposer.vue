<script setup lang="ts">
/**
 * PromptComposer：创意工坊底部创作器。
 *
 * 包含：
 * - 模式芯片行（主模式 + "更多"下拉）
 * - 参考图条（ReferenceAssetStrip）
 * - 大输入框（支持文件拖拽）
 * - 底部工具栏：语音、参数芯片、模型选择、字符计数、发送按钮
 * - 发送错误提示
 *
 * Phase 1 拆分目标：只通过 props/emit 通信，不直接调 bridge / store。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  CheckCircle2,
  ChevronDown,
  FileUp,
  FolderOpen,
  LoaderCircle,
  Mic,
  RotateCcw,
  Send,
  Settings2,
  Sparkles,
} from "@lucide/vue";
import type { CredentialRecord } from "../../../bridge/credentials";
import type { ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import CreationParameterBar from "./CreationParameterBar.vue";
import ProviderModelSelector from "./ProviderModelSelector.vue";
import ReferenceAssetStrip from "./ReferenceAssetStrip.vue";

/** 创作模式项（与 CreativeEmptyState 中的 CreationModeItem 字段保持一致）。 */
export interface ComposerModeItem {
  id: "agent" | "image" | "video" | "music" | "voiceover" | "digital-human" | "motion";
  label: string;
  description: string;
  icon: unknown;
}

const props = defineProps<{
  /** 双向绑定的 prompt 内容。 */
  modelValue: string;
  /** 主创作模式。 */
  creationMode: string;
  /** 主模式列表（铺在第一行的芯片）。 */
  primaryModes: ComposerModeItem[];
  /** 扩展模式列表（"更多"下拉里的选项）。 */
  extraModes: ComposerModeItem[];
  /** 当前模式的展示信息（用于 "更多" 按钮在选中扩展模式时显示对应 label）。 */
  currentMode: ComposerModeItem;
  /** "更多" 下拉是否展开。 */
  showModeMenu: boolean;
  /** prompt 占位符。 */
  placeholder: string;
  /** 是否处于正在发送状态。 */
  isSending: boolean;
  /** 是否可发送（综合判断）。 */
  canSend: boolean;
  /** 是否有已配置模型（用于控制 send-hint 文案）。 */
  hasCredentials: boolean;
  /** 是否处于 Agent 模式（用于控制空状态 CTA）。 */
  isAgentMode: boolean;
  /** 已上传的参考图/附件列表。 */
  referenceImages: { name: string; mimeType: string }[];
  /** 当前是否处于文件拖拽悬停态。 */
  isDraggingFile: boolean;
  /** 凭据列表。 */
  credentials: CredentialRecord[];
  /** 当前选中的凭据 id。 */
  selectedCredentialId: string;
  /** 资源账号列表（jimeng 等）。 */
  resourceAccounts?: ResourceAccountRecord[];
  /** 当前选中的资源账号 id。 */
  selectedAccountId?: string;
  /** 创作参数：比例。 */
  aspectRatio: string;
  /** 创作参数：时长。 */
  videoDuration: string;
  /** 创作参数：分辨率。 */
  resolution: string;
  /** 创作参数：帧率。 */
  frameRate: string;
  /** 字符上限。 */
  charLimit: number;
  /** 发送错误（红字提示）。 */
  sendError: string | null;
  /** 当前模式是否为扩展模式（用于"更多"按钮高亮）。 */
  isExtraMode: boolean;
  /** 偏好面板是否展开（用于偏好按钮高亮态）。 */
  showPreferences: boolean;
  /** 项目输出目录路径（null 表示使用默认工作区）。 */
  projectDirectory: string | null;
  /** 项目显示名称（文件夹名或 "OPEN AIGC"）。 */
  projectDisplayName: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  "update:creationMode": [value: string];
  "update:showModeMenu": [value: boolean];
  "update:isDraggingFile": [value: boolean];
  "update:aspectRatio": [value: string];
  "update:videoDuration": [value: string];
  "update:resolution": [value: string];
  "update:frameRate": [value: string];
  send: [];
  "remove-reference": [index: number];
  "select-credential": [credentialId: string];
  "select-account": [accountId: string];
  "prompt-keydown": [event: KeyboardEvent];
  /** Agent 模式下开始新会话（原顶部栏按钮迁移至此）。 */
  "start-new-agent-conversation": [];
  /** 切换偏好面板（原顶部栏按钮迁移至此）。 */
  "toggle-preferences": [];
  "select-directory": [];
  "reset-directory": [];
  /** 文件拖入 / 粘贴 / 选择。 */
  "drop-files": [files: FileList];
}>();

void props;

const modeMenuRef = ref<HTMLElement | null>(null);
const fileInputRef = ref<HTMLInputElement | null>(null);
const textareaRef = ref<HTMLTextAreaElement | null>(null);

const showCharCount = computed(() => props.modelValue.length > 0);
const showNoCredentialsHint = computed(() => !props.canSend && !props.hasCredentials);

function onInput(event: Event): void {
  const target = event.target as HTMLTextAreaElement;
  emit("update:modelValue", target.value);
}

function onKeydown(event: KeyboardEvent): void {
  emit("prompt-keydown", event);
}

function onSelectPrimaryMode(mode: ComposerModeItem): void {
  emit("update:creationMode", mode.id);
}

function toggleModeMenu(): void {
  emit("update:showModeMenu", !props.showModeMenu);
}

function pickExtraMode(mode: ComposerModeItem): void {
  emit("update:creationMode", mode.id);
  emit("update:showModeMenu", false);
}

function onSend(): void {
  emit("send");
}

function onRemoveReference(index: number): void {
  emit("remove-reference", index);
}

function onSelectCredential(id: string): void {
  emit("select-credential", id);
}

function onPaste(event: ClipboardEvent): void {
  const items = event.clipboardData?.items;
  if (!items) return;
  const fileItems: File[] = [];
  for (const item of Array.from(items)) {
    if (item.kind === "file") {
      const file = item.getAsFile();
      if (file) fileItems.push(file);
    }
  }
  if (fileItems.length > 0) {
    event.preventDefault();
    const dt = new DataTransfer();
    fileItems.forEach((f) => dt.items.add(f));
    emit("drop-files", dt.files);
  }
}

function onFileInputChange(event: Event): void {
  const input = event.target as HTMLInputElement;
  if (input.files && input.files.length > 0) {
    emit("drop-files", input.files);
  }
  input.value = "";
}

function triggerFileSelect(): void {
  fileInputRef.value?.click();
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as Node | null;
  if (modeMenuRef.value && target && !modeMenuRef.value.contains(target)) {
    if (props.showModeMenu) {
      emit("update:showModeMenu", false);
    }
  }
}

function preventDragDefault(event: Event): void {
  event.preventDefault();
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
  // 原生监听：阻止 textarea 的拖拽默认行为（加载文件内容），
  // 但不调用 stopPropagation，让事件冒泡到 document 级别的全局处理。
  const ta = textareaRef.value;
  if (ta) {
    ta.addEventListener("dragover", preventDragDefault);
    ta.addEventListener("drop", preventDragDefault);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
  const ta = textareaRef.value;
  if (ta) {
    ta.removeEventListener("dragover", preventDragDefault);
    ta.removeEventListener("drop", preventDragDefault);
  }
});
</script>

<template>
  <footer class="grok-composer" :class="{ 'is-drag-active': isDraggingFile }">
    <!-- 参考图/附件预览：在 composer-box 外部，不撑高输入框 -->
    <ReferenceAssetStrip :reference-images="referenceImages" @remove="onRemoveReference" />

    <div class="composer-box" :class="{ 'is-drag-over': isDraggingFile }">
      <!-- 拖拽提示 -->
      <div v-if="isDraggingFile" class="drag-hint">松开添加文件</div>
      <!-- 模式芯片平铺 -->
      <div class="mode-row">
        <button
          v-for="mode in primaryModes"
          :key="mode.id"
          type="button"
          class="mode-chip"
          :class="{ 'is-active': creationMode === mode.id }"
          :title="mode.description"
          @click="onSelectPrimaryMode(mode)"
        >
          <component :is="mode.icon" :size="13" />
          <span>{{ mode.label }}</span>
        </button>

        <div ref="modeMenuRef" class="mode-more">
          <button
            type="button"
            class="mode-chip"
            :class="{
              'is-active': isExtraMode,
              'is-open': showModeMenu,
            }"
            title="更多创作类型"
            @click="toggleModeMenu"
          >
            <component :is="isExtraMode ? currentMode.icon : Sparkles" :size="13" />
            <span>{{ isExtraMode ? currentMode.label : "更多" }}</span>
            <ChevronDown :size="11" class="mode-caret" />
          </button>
          <Transition name="menu">
            <div v-if="showModeMenu" class="mode-menu">
              <button
                v-for="mode in extraModes"
                :key="mode.id"
                type="button"
                class="mode-menu-item"
                :class="{ 'is-selected': mode.id === creationMode }"
                @click="pickExtraMode(mode)"
              >
                <component :is="mode.icon" :size="14" />
                <span>{{ mode.label }}</span>
                <CheckCircle2 v-if="mode.id === creationMode" :size="12" class="mode-menu-check" />
              </button>
            </div>
          </Transition>
        </div>

        <!-- 工作台操作按钮（原顶部栏迁移至此）：附件 + 新会话 + 偏好 -->
        <div class="composer-actions">
          <input
            ref="fileInputRef"
            type="file"
            multiple
            accept="image/*,.pdf,.doc,.docx,.txt,.md,.csv,.json"
            class="file-input-hidden"
            @change="onFileInputChange"
          />
          <button
            v-if="isAgentMode"
            type="button"
            class="composer-action-btn"
            title="上传图片或文档"
            @click="triggerFileSelect"
          >
            <FileUp :size="14" />
          </button>
          <button
            v-if="isAgentMode"
            type="button"
            class="composer-action-btn"
            title="开始新的 Agent 会话"
            @click="emit('start-new-agent-conversation')"
          >
            <Sparkles :size="14" />
          </button>
          <button
            type="button"
            class="composer-action-btn"
            :class="{ 'is-active': showPreferences }"
            title="偏好"
            @click="emit('toggle-preferences')"
          >
            <Settings2 :size="14" />
          </button>
        </div>
      </div>

      <!-- 大输入框 -->
      <textarea
        ref="textareaRef"
        class="prompt-input"
        :placeholder="placeholder"
        rows="3"
        :maxlength="charLimit"
        :value="modelValue"
        :disabled="isSending"
        @input="onInput"
        @keydown="onKeydown"
        @paste="onPaste"
      />

      <!-- 底部工具栏 -->
      <div class="toolbar">
        <div class="toolbar-left">
          <!-- 项目目录选择 -->
          <button
            type="button"
            class="tool-btn"
            :title="projectDirectory ? '更换输出目录' : '选择输出目录'"
            @click="emit('select-directory')"
          >
            <FolderOpen :size="14" />
          </button>
          <span v-if="projectDirectory" class="dir-chip" :title="projectDirectory">
            {{ projectDisplayName }}
            <button
              type="button"
              class="dir-chip__reset"
              title="恢复默认目录"
              @click.stop="emit('reset-directory')"
            >
              <RotateCcw :size="10" />
            </button>
          </span>

          <button type="button" class="tool-btn" title="语音输入">
            <Mic :size="14" />
          </button>

          <!-- 参数芯片（与模型选择器同行） -->
          <Transition name="params">
            <CreationParameterBar
              v-if="creationMode === 'image' || creationMode === 'video'"
              :creation-mode="creationMode as any"
              :aspect-ratio="aspectRatio"
              :video-duration="videoDuration"
              :resolution="resolution"
              :frame-rate="frameRate"
              @update:aspect-ratio="(v: string) => emit('update:aspectRatio', v)"
              @update:video-duration="(v: string) => emit('update:videoDuration', v)"
              @update:resolution="(v: string) => emit('update:resolution', v)"
              @update:frame-rate="(v: string) => emit('update:frameRate', v)"
            />
          </Transition>

          <!-- 模型选择 -->
          <ProviderModelSelector
            :credentials="credentials"
            :selected-credential-id="selectedCredentialId"
            :resource-accounts="resourceAccounts"
            :selected-account-id="selectedAccountId"
            :task-type="creationMode === 'video' ? 'video_generation' : 'image_generation'"
            @select="onSelectCredential"
            @select-account="(id: string) => emit('select-account', id)"
          />
        </div>

        <div class="toolbar-right">
          <span v-if="showCharCount" class="char-count"
            >{{ modelValue.length }} / {{ charLimit }}</span
          >
          <span v-if="showNoCredentialsHint" class="send-hint">需先配置模型</span>
          <button
            type="button"
            class="send-btn"
            :disabled="!canSend"
            :aria-label="isSending ? '正在发送' : '发送'"
            @click="onSend"
          >
            <LoaderCircle v-if="isSending" :size="16" class="is-spinning" />
            <Send v-else :size="16" />
          </button>
        </div>
      </div>

      <p v-if="sendError" class="send-error" role="alert">
        {{ sendError }}
      </p>
    </div>
  </footer>
</template>

<style scoped>
.grok-composer {
  flex-shrink: 0;
  margin-top: auto;
  padding: 16px 28px 24px;
  border-top: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

/* 附件预览条：在 composer-box 上方，不撑高输入框 */
.grok-composer > :deep(.ref-strip) {
  max-width: 720px;
  margin: 0 auto 6px;
}

/* 拖拽时 footer 需要接收事件 */
.grok-composer.is-drag-active {
  pointer-events: auto;
}

/* 拖拽文件时输入框高亮 */
.composer-box.is-drag-over {
  border-color: var(--color-accent) !important;
  border-style: dashed !important;
  background: color-mix(in srgb, var(--color-accent) 6%, var(--material-sheet)) !important;
}

.drag-hint {
  text-align: center;
  padding: 6px 0;
  font-size: 13px;
  font-weight: 500;
  color: var(--color-accent);
}

.composer-box {
  position: relative;
  max-width: 720px;
  margin: 0 auto;
  background: var(--material-sheet);
  border: 1px solid var(--color-border-subtle);
  border-radius: 16px;
  padding: 14px;
  pointer-events: auto;
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  box-shadow: var(--shadow-lg);
  transition:
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.composer-box:focus-within {
  border-color: var(--color-border);
  box-shadow: var(--shadow-xl);
}

/* ─── 模式芯片行 ─── */
.mode-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-height: 32px;
  padding: 0 0 10px;
}

.mode-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 30px;
  padding: 5px 10px;
  border-radius: var(--radius-control);
  background: transparent;
  border: 1px solid var(--color-border-subtle);
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition:
    background 120ms ease,
    color 120ms ease,
    border-color 120ms ease;
}

.mode-chip:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
  border-color: var(--color-border);
}

.mode-chip.is-active {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.mode-more {
  position: relative;
}

/* ─── 工作台操作按钮（新会话 / 偏好） ─── */
.composer-actions {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
}

.composer-action-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition:
    background 120ms ease,
    color 120ms ease,
    border-color 120ms ease;
}

.composer-action-btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-subtle);
  color: var(--color-text);
}

.composer-action-btn.is-active {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.file-input-hidden {
  position: absolute;
  width: 0;
  height: 0;
  opacity: 0;
  pointer-events: none;
}

.mode-caret {
  margin-left: 2px;
  opacity: 0.7;
  transition: transform 160ms ease;
}

.mode-chip.is-open .mode-caret {
  transform: rotate(180deg);
}

.mode-menu {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 160px;
  background: var(--material-sheet);
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  padding: 4px;
  box-shadow: var(--shadow-lg);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  z-index: 30;
}

.mode-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--color-text);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
  transition: background 120ms ease;
}

.mode-menu-item:hover {
  background: var(--color-surface-hover);
}

.mode-menu-item.is-selected {
  background: var(--color-surface-selected);
  color: var(--color-accent);
}

.mode-menu-check {
  margin-left: auto;
  color: var(--color-success);
}

.menu-enter-active,
.menu-leave-active {
  transition:
    opacity 140ms ease,
    transform 140ms ease;
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(4px);
}

/* ─── 大输入框 ─── */
.prompt-input {
  display: block;
  width: 100%;
  min-height: 92px;
  max-height: 280px;
  padding: 12px;
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  color: var(--color-text);
  font-size: 14px;
  line-height: 1.5;
  font-family: inherit;
  resize: vertical;
  outline: none;
  transition:
    background 160ms ease,
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.prompt-input::placeholder {
  color: var(--color-text-tertiary);
}

.prompt-input:focus {
  background: var(--color-surface);
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px var(--color-accent-soft);
}

.composer-box.is-dragover {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  border-style: dashed;
}

.prompt-input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ─── 工具栏 ─── */
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 36px;
  padding: 8px 0 0;
  flex-wrap: wrap;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
  flex-wrap: wrap;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.tool-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition:
    background 120ms ease,
    color 120ms ease;
}

.tool-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.dir-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  background: var(--color-accent-soft);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  color: var(--color-accent);
  font-size: 11px;
  font-weight: 500;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-chip__reset {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--color-accent);
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}

.dir-chip__reset:hover {
  background: var(--color-surface-hover);
}

.char-count {
  font-size: 11px;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.send-hint {
  font-size: 11px;
  color: var(--color-warning);
}

.send-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--radius-control);
  background: var(--color-accent);
  color: var(--color-on-accent);
  cursor: pointer;
  transition:
    opacity 120ms ease,
    transform 120ms ease;
}

.send-btn:hover:not(:disabled) {
  opacity: 1;
  transform: translateY(-1px);
  background: var(--color-accent-hover);
}

.send-btn:disabled {
  background: var(--color-surface-hover);
  color: var(--color-text-disabled);
  cursor: not-allowed;
}

.send-error {
  margin: 6px 4px 0;
  font-size: 11px;
  color: var(--color-danger);
}

.params-enter-active,
.params-leave-active {
  transition:
    opacity 160ms ease,
    max-width 220ms ease;
  overflow: hidden;
}

.params-enter-from,
.params-leave-to {
  opacity: 0;
  max-width: 0;
}

@media (max-width: 640px) {
  .grok-composer {
    padding: 12px;
  }

  .composer-box {
    padding: 10px;
    border-radius: var(--radius-control);
  }

  .mode-row {
    flex-wrap: nowrap;
    overflow-x: auto;
    padding-bottom: 6px;
  }

  .mode-chip {
    flex: 0 0 auto;
  }

  .prompt-input {
    min-height: 86px;
  }

  .toolbar {
    align-items: flex-end;
  }

  .toolbar-left {
    flex-wrap: wrap;
  }

  .toolbar-right {
    margin-left: auto;
  }
}

/* 旋转动画：scoped 样式无法从父页面穿透到本组件内部元素，
   必须在本组件内自定义，否则发送按钮的 LoaderCircle 静止不转。 */
.is-spinning {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
