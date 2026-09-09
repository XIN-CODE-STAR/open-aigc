<script setup lang="ts">
/**
 * AgentStepTimeline：Agent 模式消息流（用户消息 + 助手消息 + 工具调用）。
 *
 * 渲染四类节点：
 * - 用户消息（右对齐浅胶囊）
 * - 助手消息（左侧纯文本 + 工具调用列表 + token 用量）
 * - Agent 工作中占位（根据 loopPhase 显示不同文案）
 * - 会话累计 token 用量
 *
 * Phase 1 拆分目标：只通过 props/emit 通信，不直接调 bridge / store。
 */
import { computed } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { Hash, LoaderCircle, Sparkles } from "@lucide/vue";
import type { PlanRecord } from "../../../bridge/agent";
import type { AgentMessageGroup, AgentLoopPhase } from "../composables/useAgentConversation";
import ExecutionPlanPanel from "./ExecutionPlanPanel.vue";
import ToolInvocationPanel from "./ToolInvocationPanel.vue";

const props = defineProps<{
  /** Agent 模式消息分组列表。 */
  groups: AgentMessageGroup[];
  /** 是否正在发送。 */
  isWorking: boolean;
  /** 当前循环阶段。 */
  loopPhase: AgentLoopPhase;
  /** 当前阶段对应的中文标签。 */
  workingLabel: string;
  /** 会话累计 token 用量。 */
  tokenUsage: { total: number; prompt: number; completion: number };
  /** 流式输出的实时累积文本。 */
  streamingContent: string;
  /** 流式输出是否进行中。 */
  isStreaming: boolean;
  /** 已展开的 invocation id 集合（由父组件 isInvocationExpanded 暴露）。 */
  isInvocationExpanded: (id: string) => boolean;
  /** 工具名 → 中文名映射函数。 */
  toolLabel: (name: string) => string;
  /** JSON 美化函数。 */
  prettyJson: (raw: string) => string;
  /** 时间格式化函数。 */
  formatTime: (iso: string) => string;
  /** 当前轮次的执行计划（内联显示在该轮用户消息之下、模型回复之上）。 */
  currentPlan: PlanRecord | null;
  /** 当前计划所属轮次的用户消息 id；为空时计划退化为时间线末尾渲染。 */
  planRoundMessageId: string | null;
}>();

/**
 * 兜底渲染开关：计划存在但锚点消息 id 缺失或已不在分组列表中时，
 * 仍把计划面板渲染在时间线末尾，保证计划始终可见。
 */
const showPlanAtBottom = computed(() => {
  if (!props.currentPlan) return false;
  const anchor = props.planRoundMessageId;
  if (!anchor) return true;
  return !props.groups.some((group) => group.userMessage?.id === anchor);
});

const emit = defineEmits<{
  /** 切换某个 invocation 的展开状态。 */
  "toggle-invocation": [id: string];
}>();

function onToggleInvocation(id: string): void {
  emit("toggle-invocation", id);
}

/** 双击图片用系统默认程序打开。 */
async function openImageWithSystemViewer(url: string): Promise<void> {
  if (
    typeof window === "undefined" ||
    !(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  ) {
    console.warn("[open-image] Tauri runtime not available");
    return;
  }
  try {
    let target = url;
    // asset:// 或 https://asset.localhost/ 开头的是本地文件，提取真实路径
    if (url.startsWith("asset://")) {
      target = decodeURIComponent(url.replace("asset://localhost/", "").replace("asset://", ""));
    } else if (url.startsWith("https://asset.localhost/")) {
      target = decodeURIComponent(url.replace("https://asset.localhost/", ""));
    }
    await invoke("open_file_with_system_viewer", { path: target });
    console.log("[open-image] Opened:", target);
  } catch (err) {
    console.error("[open-image] Failed to open:", url, err);
  }
}

/** 从工具调用结果 JSON 中提取图片 URL。 */
function extractImageUrls(resultJson: string | null): string[] {
  if (!resultJson) return [];
  try {
    const data = JSON.parse(resultJson);

    // 优先使用远程 URL（始终可用）
    const remoteId = data.attempt?.remoteJobId || "";
    const remoteUrl = remoteId.replace(/^proxy:image:/, "").replace(/^image:/, "");
    if (remoteUrl && remoteUrl.startsWith("http")) {
      return [remoteUrl];
    }

    // 本地资产路径（备用）
    if (data.localAsset?.filePath) {
      try {
        return [convertFileSrc(data.localAsset.filePath)];
      } catch {
        // ignore
      }
    }

    return [];
  } catch {
    return [];
  }
}

/**
 * 解析用户消息内容。
 * 后端将含附件的消息存为 JSON：`{"images": [...], "text": "..."}`。
 * 纯文本消息直接返回。返回值拆开 text 与 images，避免 base64 出现在文字区。
 */
function parseUserContent(raw: string | null): { text: string; images: string[] } {
  if (!raw) return { text: "", images: [] };
  try {
    const data = JSON.parse(raw);
    if (typeof data === "object" && data !== null && typeof data.text === "string") {
      const imgs = Array.isArray(data.images)
        ? data.images.filter((i: unknown) => typeof i === "string" && i.length > 0)
        : [];
      console.log("[parseUserContent] multimodal message", {
        textLen: data.text.length,
        imageCount: imgs.length,
        rawLen: raw.length,
      });
      return { text: data.text, images: imgs };
    }
  } catch {
    // not JSON — plain text
  }
  console.log("[parseUserContent] plain text", { rawLen: raw.length });
  return { text: raw, images: [] };
}

/**
 * 将 data: URL 转为 blob: URL，解决部分 Webview 对 data URL 渲染的兼容问题。
 * 转换失败时回退到原始 data URL。
 */
function renderContent(content: string): string {
  return content.replace(
    /!\[([^\]]*)\]\(([^)]+)\)/g,
    '<img src="$2" alt="$1" style="max-width:100%;max-height:400px;border-radius:8px;margin:8px 0;display:block;object-fit:contain;" />',
  );
}

function dataUrlToBlobUrl(dataUrl: string): string {
  if (!dataUrl.startsWith("data:")) return dataUrl;
  try {
    const [header, base64] = dataUrl.split(",");
    const mime = header.match(/data:([^;]+)/)?.[1] ?? "application/octet-stream";
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    const blob = new Blob([bytes], { type: mime });
    return URL.createObjectURL(blob);
  } catch {
    return dataUrl;
  }
}

/**
 * 图片加载失败时尝试用 blob URL 作为 fallback。
 */
function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement;
  const src = img.src;
  console.warn("[handleImageError] image load failed", {
    src: src.substring(0, 100),
    alt: img.alt,
  });
  // 如果是 data URL 且还没试过 blob fallback，尝试转换
  if (src.startsWith("data:")) {
    const blobUrl = dataUrlToBlobUrl(src);
    if (blobUrl !== src) {
      console.log("[handleImageError] trying blob URL fallback");
      img.src = blobUrl;
      return;
    }
  }
  img.alt = "图片加载失败";
}
</script>

<template>
  <div class="grok-messages">
    <TransitionGroup name="turn">
      <div v-for="group in groups" :key="group.id" class="turn">
        <!-- 用户消息；若该轮是执行计划的锚点轮次，计划面板紧随其下渲染 -->
        <template v-if="group.userMessage">
          <div class="turn-user">
            <p class="user-text">{{ parseUserContent(group.userMessage.content).text }}</p>
            <div
              v-if="parseUserContent(group.userMessage.content).images.length > 0"
              class="user-images"
            >
              <img
                v-for="(img, imgIdx) in parseUserContent(group.userMessage.content).images"
                :key="imgIdx"
                :src="img"
                class="user-image"
                alt="上传的图片"
                loading="lazy"
                @dblclick="openImageWithSystemViewer(img)"
                @error="handleImageError($event)"
              />
            </div>
            <div class="turn-meta">
              <span>{{ formatTime(group.userMessage.createdAt) }}</span>
            </div>
          </div>
          <!-- 当轮执行计划：位于用户消息之下、模型回复之上 -->
          <ExecutionPlanPanel
            v-if="currentPlan && group.userMessage.id === planRoundMessageId"
            class="turn-plan"
            :plan="currentPlan"
          />
        </template>

        <!-- 助手消息：思考文本 + 工具调用 -->
        <div v-else-if="group.assistantMessage" class="turn-ai">
          <!-- 内容来自自有 LLM，可信来源；v-html 用于渲染 markdown 图片 -->
          <!-- eslint-disable vue/no-v-html -->
          <div
            v-if="group.assistantMessage.content"
            class="ai-text"
            v-html="renderContent(group.assistantMessage.content)"
          />
          <!-- eslint-enable vue/no-v-html -->

          <!-- 工具调用列表 -->
          <div
            v-if="group.assistantMessage.toolCalls.length > 0 || group.invocations.length > 0"
            class="ai-invocations"
          >
            <ToolInvocationPanel
              v-for="inv in group.invocations"
              :key="inv.id"
              :invocation="inv"
              :is-expanded="isInvocationExpanded(inv.id)"
              :tool-label="toolLabel"
              :pretty-json="prettyJson"
              @toggle="onToggleInvocation"
            />
          </div>

          <!-- 生成结果图片预览 -->
          <div
            v-for="inv in group.invocations.filter((i) => i.status === 'succeeded' && i.resultJson)"
            :key="'img-' + inv.id"
            class="ai-images"
          >
            <template v-if="extractImageUrls(inv.resultJson).length > 0">
              <img
                v-for="(url, idx) in extractImageUrls(inv.resultJson)"
                :key="idx"
                :src="url"
                class="ai-image"
                alt="生成的图片"
                loading="lazy"
                @dblclick="openImageWithSystemViewer(url)"
              />
            </template>
          </div>

          <!-- 助手消息底部：token 用量 -->
          <div
            v-if="
              group.assistantMessage.promptTokens !== null ||
              group.assistantMessage.completionTokens !== null
            "
            class="ai-tokens"
          >
            <Hash :size="10" />
            <span v-if="group.assistantMessage.promptTokens !== null">
              in {{ group.assistantMessage.promptTokens }}
            </span>
            <span v-if="group.assistantMessage.completionTokens !== null">
              · out {{ group.assistantMessage.completionTokens }}
            </span>
          </div>
        </div>
      </div>
    </TransitionGroup>

    <!-- 流式输出气泡：LLM 增量返回期间实时展示，落库后由正式消息接管 -->
    <div v-if="isStreaming && streamingContent" class="turn">
      <div class="turn-ai">
        <p class="ai-text ai-text--streaming">
          {{ streamingContent }}<span class="stream-cursor" />
        </p>
      </div>
    </div>

    <!-- Agent 工作中占位 -->
    <div v-if="isWorking" class="turn">
      <div class="turn-ai turn-ai--running">
        <div class="ai-status" :class="`ai-status--${loopPhase}`">
          <LoaderCircle v-if="loopPhase === 'thinking'" :size="13" class="is-spinning" />
          <Sparkles v-else :size="13" class="is-pulsing" />
          <span>{{ workingLabel }}</span>
        </div>
      </div>
    </div>

    <!-- 执行计划兜底位：锚点用户消息缺失时渲染在时间线末尾，保证计划可见。
         正常情况下面板已内联到锚点轮次的用户消息之下（模型回复之上）。 -->
    <ExecutionPlanPanel
      v-if="showPlanAtBottom && currentPlan"
      class="turn-plan"
      :plan="currentPlan"
    />

    <!-- 会话累计 token -->
    <div v-if="tokenUsage.total > 0 && !isWorking" class="conv-tokens">
      <Hash :size="10" />
      <span>会话累计 {{ tokenUsage.total }} tokens</span>
      <span class="conv-tokens-break">
        (in {{ tokenUsage.prompt }} / out {{ tokenUsage.completion }})
      </span>
    </div>
  </div>
</template>

<style scoped>
.grok-messages {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 860px;
  margin: 0 auto;
  padding: 8px var(--page-padding) 24px;
}

.turn {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.turn-user {
  align-self: flex-end;
  max-width: 80%;
  padding: 8px 12px;
  background: var(--color-accent-soft);
  border: 1px solid var(--color-border-subtle);
  border-radius: 12px 12px 4px 12px;
  color: var(--color-text);
  font-size: 13px;
  line-height: 1.5;
}

.user-text {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.user-images {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 6px;
}

.user-image {
  max-width: 180px;
  max-height: 140px;
  border-radius: 6px;
  object-fit: cover;
  cursor: pointer;
}

.turn-meta {
  margin-top: 4px;
  font-size: 10px;
  color: var(--color-text-tertiary);
  display: flex;
  gap: 6px;
}

.dot {
  opacity: 0.6;
}

.turn-ai {
  align-self: flex-start;
  max-width: min(90%, 820px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 0;
  border-left: 2px solid var(--color-border-subtle);
  padding-left: 12px;
}

.ai-text {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--color-text);
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: break-word;
}

.ai-text--streaming {
  display: inline;
}

.stream-cursor {
  display: inline-block;
  width: 6px;
  height: 13px;
  margin-left: 2px;
  vertical-align: text-bottom;
  background: currentColor;
  border-radius: 1px;
  animation: stream-cursor-blink 1s steps(2, start) infinite;
}

@keyframes stream-cursor-blink {
  to {
    visibility: hidden;
  }
}

.ai-invocations {
  display: flex;
  flex-direction: column;
}

.ai-tokens {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.turn-ai--running {
  padding: 6px 0;
}

.ai-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-text-secondary);
}

.ai-status--failed {
  color: var(--color-danger);
}

/* 内联执行计划面板：占满消息列宽度，跟随当轮对话滚动。 */
.turn-plan {
  align-self: stretch;
}

/* ─── 生成结果图片预览 ─── */
.ai-images {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}

.ai-image {
  max-width: min(320px, 100%);
  max-height: 320px;
  width: auto;
  height: auto;
  border-radius: 10px;
  border: 1px solid var(--color-border-subtle);
  object-fit: contain;
  cursor: pointer;
  transition: transform 160ms ease;
}

.ai-image:hover {
  transform: scale(1.02);
}

.conv-tokens {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--color-text-tertiary);
  margin-top: 8px;
}

.conv-tokens-break {
  opacity: 0.7;
}

/* 旋转动画：scoped 样式无法从父页面穿透到本组件内部元素，
   必须在本组件内自定义，否则 LoaderCircle 静止不转。 */
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

/* Transitions */
.turn-enter-active,
.turn-leave-active {
  transition:
    opacity 200ms ease,
    transform 200ms ease;
}

.turn-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.turn-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
