<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Film,
  Image as ImageIcon,
  LoaderCircle,
  Mic,
  Music,
  type Sparkles,
  UserCircle,
  Wand2,
  Zap,
  X,
} from "@lucide/vue";

import { useWorkspaceStore } from "../../../app/stores/workspace";
import { type GenerationTaskRecord, pickGenerationOutputFile } from "../../../bridge/generations";
import { listCredentials, type CredentialRecord } from "../../../bridge/credentials";
import { listResourceAccounts, type ResourceAccountRecord } from "../../../bridge/resourceAccounts";
import { memoryV1Status } from "../../../bridge/memory";
import { useToast } from "../../../shared/ui/useToast";
import { useGenerationHistory } from "../composables/useGenerationHistory";
import { useAgentConversation } from "../composables/useAgentConversation";
import { isGenerationNodeTool } from "../generationNodeTools";
import { useRoute, useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

// Phase 1：组件拆分
import ConversationRail from "../components/ConversationRail.vue";
import CreativeEmptyState from "../components/CreativeEmptyState.vue";
import PromptComposer from "../components/PromptComposer.vue";
import ExtensionPanel from "../components/ExtensionPanel.vue";
import {
  buildGenerationHint,
  buildSystemPromptOverride,
  loadSelectedSkillIds,
  saveSelectedSkillIds,
} from "../skills";
import CreativeMemoryPanel from "../components/CreativeMemoryPanel.vue";
import MemoryCanvasPanel from "../../../modules/memory/components/MemoryCanvasPanel.vue";
import { queueV1SubmitAttempt } from "../../../bridge/queue";
import { useProjectDirectory } from "../../../app/stores/projectDirectory";
import { useMemoryCanvasStore } from "../../../app/stores/memoryCanvas";
import type { AttachmentInput } from "../../../bridge/agent";
import { analyzeImage } from "../../../bridge/imageAnalyzer";
import { useSemanticAnalysis } from "../composables/useSemanticAnalysis";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>;
  }
}

/* ──────────────────────────────────────────────
 *  类型 & 常量
 * ────────────────────────────────────────────── */

type CreationMode =
  "agent" | "image" | "video" | "music" | "voiceover" | "digital-human" | "motion";

interface CreationModeItem {
  id: CreationMode;
  label: string;
  icon: typeof Sparkles;
  description: string;
  badge?: string;
}

const CREATION_MODES: CreationModeItem[] = [
  { id: "agent", label: "Agent 模式", icon: Zap, description: "AI 智能体协同创作", badge: "推荐" },
  { id: "image", label: "图片生成", icon: ImageIcon, description: "文生图 / 图生图" },
  { id: "video", label: "视频生成", icon: Film, description: "文生视频 / 图生视频" },
  { id: "music", label: "音乐生成", icon: Music, description: "文本生成音乐" },
  { id: "voiceover", label: "配音生成", icon: Mic, description: "文本转语音" },
  { id: "digital-human", label: "数字人", icon: UserCircle, description: "AI 数字人视频" },
  { id: "motion", label: "动作模仿", icon: Wand2, description: "视频动作迁移" },
];

// 顶部常驻显示的创作类型
const PRIMARY_MODES: CreationModeItem[] = [
  { id: "agent", label: "Agent", icon: Zap, description: "AI 智能体协同创作" },
  { id: "image", label: "图片", icon: ImageIcon, description: "文生图 / 图生图" },
  { id: "video", label: "视频", icon: Film, description: "文生视频 / 图生视频" },
];

// 收纳到「更多」下拉中的创作类型
const EXTRA_MODES: CreationModeItem[] = [
  { id: "music", label: "音乐生成", icon: Music, description: "文本生成音乐" },
  { id: "voiceover", label: "配音生成", icon: Mic, description: "文本转语音" },
  { id: "digital-human", label: "数字人", icon: UserCircle, description: "AI 数字人视频" },
  { id: "motion", label: "动作模仿", icon: Wand2, description: "视频动作迁移" },
];

const PRIMARY_MODE_IDS = new Set<CreationMode>(PRIMARY_MODES.map((m) => m.id));

function isExtraMode(mode: CreationMode): boolean {
  return !PRIMARY_MODE_IDS.has(mode);
}

/* ──────────────────────────────────────────────
 *  示例提示（空状态时显示，点击填入输入框）
 * ────────────────────────────────────────────── */

interface ExamplePrompt {
  id: string;
  title: string;
  prompt: string;
  mode: CreationMode;
  icon: typeof Sparkles;
}

const EXAMPLE_PROMPTS: ExamplePrompt[] = [
  {
    id: "image-portrait",
    title: "人物肖像",
    prompt: "一位 25 岁的亚洲女性肖像，柔和的窗光，胶片质感，自然妆容，写实摄影风格，浅景深",
    mode: "image",
    icon: ImageIcon,
  },
  {
    id: "image-product",
    title: "产品图",
    prompt: "一款极简风格的无线耳机悬浮在纯白背景上，柔和的阴影，专业产品摄影，4K 细节",
    mode: "image",
    icon: ImageIcon,
  },
  {
    id: "video-scene",
    title: "风景短片",
    prompt: "延时摄影：日出时分云海翻涌的山脉，温暖的金色光线，电影级调色，8K 高清",
    mode: "video",
    icon: Film,
  },
  {
    id: "video-story",
    title: "故事短片",
    prompt: "一位少年骑着自行车穿过夏日樱花街道，胶片颗粒感，宫崎骏动画风格，4K 高清",
    mode: "video",
    icon: Film,
  },
];

const EXAMPLES_BY_MODE = computed<Record<CreationMode, ExamplePrompt[]>>(() => ({
  agent: EXAMPLE_PROMPTS,
  image: EXAMPLE_PROMPTS.filter((p) => p.mode === "image"),
  video: EXAMPLE_PROMPTS.filter((p) => p.mode === "video"),
  music: [],
  voiceover: [],
  "digital-human": [],
  motion: [],
}));

/* ──────────────────────────────────────────────
 *  Composable
 * ────────────────────────────────────────────── */

const workspace = useWorkspaceStore();
const history = useGenerationHistory();
const agent = useAgentConversation();
const projectDir = useProjectDirectory();
const route = useRoute();
const router = useRouter();
const toast = useToast();
const { tasks } = history;
const {
  currentConversation: agentConversation,
  groupedMessages: agentGroups,
  isSending: agentSending,
  errorMessage: agentError,
  loopPhase: agentLoopPhase,
  tokenUsage: agentTokenUsage,
  isInvocationExpanded,
  toggleInvocation,
  currentPlan,
  planRoundMessageId,
  pendingQuestion,
  recalledMemories,
  streamingContent: agentStreamingContent,
  isStreaming: agentIsStreaming,
} = agent;

const credentials = ref<CredentialRecord[]>([]);
const resourceAccounts = ref<ResourceAccountRecord[]>([]);
const loaded = ref(false);
const promptText = ref("");
const selectedCredentialId = ref<string>("");
const selectedAccountId = ref<string>("");
const modelMenuOpen = ref(false);
const modelMenuRef = ref<HTMLElement | null>(null);
const sending = ref(false);
const sendError = ref<string | null>(null);
const recordBusyTaskId = ref<string | null>(null);
const recordError = ref<string | null>(null);
const conversationRef = ref<HTMLElement | null>(null);
/** 输入框当前内容快照，用于失败后重试时恢复 */
const lastDraft = ref("");

// 记忆服务状态
const memoryStatus = ref<"active" | "starting" | "inactive" | "unconfigured">("unconfigured");

async function checkMemoryStatus(): Promise<void> {
  try {
    const result = await memoryV1Status();
    memoryStatus.value = result.available ? "active" : "inactive";
  } catch {
    memoryStatus.value = "inactive";
  }
}

// 创作参数
const creationMode = ref<CreationMode>("agent");
const aspectRatio = ref<string>("16:9");
const videoDuration = ref<string>("4s");
const resolution = ref<string>("1080P");
const frameRate = ref<string>("30fps");
/** 参考图/附件：name 为文件名，dataUrl 为 base64 data URL，mimeType 为 MIME 类型，filePath 为原始文件路径。 */
const referenceImages = ref<
  { name: string; dataUrl: string; mimeType: string; filePath?: string }[]
>([]);
const showPreferences = ref(false);
const memoryCanvas = useMemoryCanvasStore();
const memoryCanvasRef = ref<InstanceType<typeof MemoryCanvasPanel> | null>(null);
const showModeMenu = ref(false);
const modeMenuRef = ref<HTMLElement | null>(null);

// 语义分析
const semanticAnalysis = useSemanticAnalysis();
const showAnalysisPanel = ref(false);
const analysisResults = ref<Map<string, { caption: string; tags: string[] }>>(new Map());
const aspectMenuOpen = ref(false);
const durationMenuOpen = ref(false);
const resolutionMenuOpen = ref(false);
const frameRateMenuOpen = ref(false);
const aspectMenuRef = ref<HTMLElement | null>(null);
const durationMenuRef = ref<HTMLElement | null>(null);
const resolutionMenuRef = ref<HTMLElement | null>(null);
const frameRateMenuRef = ref<HTMLElement | null>(null);
const isDraggingFile = ref(false);
let dragCounter = 0;
let tauriDragDropUnlisten: (() => void) | null = null;

function onDocDragEnter(): void {
  dragCounter++;
  isDraggingFile.value = true;
}
function onDocDragOver(event: DragEvent): void {
  event.preventDefault();
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = "copy";
  }
}
function onDocDragLeave(): void {
  dragCounter--;
  if (dragCounter <= 0) {
    dragCounter = 0;
    isDraggingFile.value = false;
  }
}
function onDocDrop(event: DragEvent): void {
  event.preventDefault();
  dragCounter = 0;
  isDraggingFile.value = false;
  const files = event.dataTransfer?.files;
  if (files && files.length > 0) {
    addReferenceFiles(files);
  }
}

/**
 * 将 Tauri 原生拖拽返回的文件路径列表加入参考图/附件列表。
 * 使用 Tauri asset protocol（已配置 scope: **）读取文件内容。
 */
async function addDroppedFilePaths(paths: string[]): Promise<void> {
  const seen = new Set(referenceImages.value.map((r) => r.name));
  for (const path of paths) {
    const name = path.split(/[/\\]/).pop() ?? path;
    if (seen.has(name)) continue;
    seen.add(name);
    try {
      const assetUrl = convertFileSrc(path);
      const resp = await fetch(assetUrl);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const blob = await resp.blob();
      const mimeType = blob.type || guessMime(name);
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
      referenceImages.value.push({ name, dataUrl, mimeType, filePath: path });
    } catch {
      // Fallback: 尝试用 invoke 调用自定义 Rust 命令
      try {
        const dataUrl = await invoke<string>("file_read_as_data_url", { path });
        const mimeMatch = dataUrl.match(/^data:([^;]+);/);
        const mimeType = mimeMatch?.[1] ?? "application/octet-stream";
        referenceImages.value.push({ name, dataUrl, mimeType, filePath: path });
      } catch (err2) {
        const msg = err2 instanceof Error ? err2.message : String(err2);
        console.error("[drag-drop] Failed to read dropped file:", path, msg);
        toast.error(`文件读取失败: ${name}`);
      }
    }
  }
}

function guessMime(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    bmp: "image/bmp",
    svg: "image/svg+xml",
    mp4: "video/mp4",
    webm: "video/webm",
    mov: "video/quicktime",
    mp3: "audio/mpeg",
    wav: "audio/wav",
    pdf: "application/pdf",
    json: "application/json",
    txt: "text/plain",
    md: "text/markdown",
    csv: "text/csv",
  };
  return map[ext] ?? "application/octet-stream";
}

onMounted(async () => {
  document.addEventListener("dragenter", onDocDragEnter);
  document.addEventListener("dragover", onDocDragOver);
  document.addEventListener("dragleave", onDocDragLeave);
  document.addEventListener("drop", onDocDrop);

  // Tauri native drag-drop: more reliable than browser drag events in WebView.
  // dragDropEnabled: true in tauri.conf.json enables Tauri to intercept native file drops
  // and fire onDragDropEvent instead of letting the browser handle them.
  if (typeof window.__TAURI_INTERNALS__ !== "undefined") {
    try {
      const appWindow = getCurrentWindow();
      tauriDragDropUnlisten = await appWindow.onDragDropEvent(async (event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          isDraggingFile.value = true;
        } else if (event.payload.type === "leave") {
          isDraggingFile.value = false;
        } else if (event.payload.type === "drop") {
          isDraggingFile.value = false;
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            await addDroppedFilePaths(paths);
          }
        }
      });
    } catch (err) {
      console.warn("[drag-drop] Failed to register Tauri native drag-drop handler:", err);
    }
  }
});

onUnmounted(() => {
  document.removeEventListener("dragenter", onDocDragEnter);
  document.removeEventListener("dragover", onDocDragOver);
  document.removeEventListener("dragleave", onDocDragLeave);
  document.removeEventListener("drop", onDocDrop);
  tauriDragDropUnlisten?.();
  tauriDragDropUnlisten = null;
});

let pollTimer: ReturnType<typeof setInterval> | null = null;

/* ──────────────────────────────────────────────
 *  Computed
 * ────────────────────────────────────────────── */

const orderedTasks = computed(() =>
  [...tasks.value].sort((a, b) => a.createdAt.localeCompare(b.createdAt)),
);

const hasPendingOrRunning = computed(() =>
  tasks.value.some((t) => t.status === "pending" || t.status === "running"),
);

const selectedCredential = computed(
  () => credentials.value.find((c) => c.id === selectedCredentialId.value) ?? null,
);

const selectedAccount = computed(
  () => resourceAccounts.value.find((a) => a.id === selectedAccountId.value) ?? null,
);

/** 是否有任何可用的模型来源（API 凭据或 AI 账号）。 */
const hasAnyModelSource = computed(
  () => credentials.value.length > 0 || resourceAccounts.value.some((a) => a.enabled),
);

const currentMode = computed(
  () => CREATION_MODES.find((m) => m.id === creationMode.value) ?? CREATION_MODES[0],
);

const promptPlaceholder = computed(() =>
  !hasAnyModelSource.value
    ? "请先在「模型管理」配置 AI 凭据或连接 AI 账号…"
    : '输入想法、剧本或上传参考，支持 "/" 使用技能，"@" 添加主体，和 Agent 一起创作',
);

const canSend = computed(
  () =>
    !sending.value &&
    !agentSending.value &&
    promptText.value.trim().length > 0 &&
    (selectedCredential.value !== null || (selectedAccount.value !== null && !isAgentMode.value)),
);

const isAgentMode = computed(() => creationMode.value === "agent");

const agentWorking = computed(() => agentSending.value);
const creativeMemoryUserId = computed(() => "default");

/** 当前是否处于空状态（无活跃会话/无路由会话 ID）。 */
const isEmpty = computed(
  () =>
    !agentWorking.value &&
    agentGroups.value.length === 0 &&
    !agentConversation.value &&
    !routeConversationId(),
);

/**
 * 对话流分组：在 agentGroups 末尾追加一条合成的错误消息（如有），
 * 使报错信息直接显示为 Agent 的一条回复，而非浮窗提示。
 */
const conversationGroups = computed(() => {
  const groups = agentGroups.value;
  const error = agentError.value;
  if (!error) return groups;
  const lastId = groups.length > 0 ? groups[groups.length - 1].id : "";
  const errorId = `err-${lastId}-${error.length}`;
  return [
    ...groups,
    {
      id: errorId,
      assistantMessage: {
        id: errorId,
        conversationId: agentConversation.value?.id ?? "",
        role: "assistant" as const,
        content: `⚠️ ${error}`,
        toolCalls: [],
        toolCallId: null,
        remoteModel: null,
        finishReason: "error",
        parentMessageId: null,
        revision: 1,
        createdAt: new Date().toISOString(),
        promptTokens: null,
        completionTokens: null,
      },
      invocations: [],
      toolResults: [],
    },
  ];
});

/**
 * Agent 模式下统一显示的错误信息：优先展示 agent 错误，回退到 generation 历史。
 */
/**
 * Agent 工作状态标签：思考中 / 工具执行中 / 空闲。
 */
const agentWorkingLabel = computed(() => {
  switch (agentLoopPhase.value) {
    case "thinking":
      return "Agent 思考中…";
    case "executing":
      return "工具执行中…";
    case "failed":
      return "执行失败";
    default:
      return "";
  }
});

/**
 * 工具调用名称中文映射，便于在 UI 中显示友好名称。
 */
const TOOL_LABELS: Record<string, string> = {
  image_generation: "图片生成",
  video_generation: "视频生成",
  list_credentials: "查看凭据",
  current_time: "当前时间",
};

function toolLabel(name: string): string {
  return TOOL_LABELS[name] ?? name;
}

/**
 * 美化 JSON 字符串：尝试解析并缩进，失败时原样返回。
 */
function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/* ──────────────────────────────────────────────
 *  数据加载
 * ────────────────────────────────────────────── */

async function loadCredentials(): Promise<void> {
  try {
    const list = await listCredentials();
    credentials.value = list.filter((c) => c.enabled);
    if (!selectedCredentialId.value && credentials.value.length > 0) {
      selectedCredentialId.value = credentials.value[0].id;
    }
  } catch {
    credentials.value = [];
  }
  // 同时加载 AI 账号（即梦等）
  try {
    const accounts = await listResourceAccounts();
    // 包含 active 和 need_login 状态的账号，need_login 表示 Session 可能需要更新但仍可用
    resourceAccounts.value = accounts.filter((a) => a.enabled && a.status !== "blocked");
    if (
      !selectedCredentialId.value &&
      !selectedAccountId.value &&
      resourceAccounts.value.length > 0
    ) {
      selectedAccountId.value = resourceAccounts.value[0].id;
    }
  } catch {
    resourceAccounts.value = [];
  }
}

async function loadAll(): Promise<void> {
  projectDir.restore();
  await Promise.all([
    history.load(),
    loadCredentials(),
    agent.loadConversations(),
    checkMemoryStatus(),
  ]);
  await syncConversationFromRoute();
}

function routeConversationId(): string | null {
  return typeof route.query.conversation === "string" ? route.query.conversation : null;
}

async function syncConversationFromRoute(): Promise<void> {
  const conversationId = routeConversationId();
  if (!conversationId) {
    // 无 conversation ID（新建对话）→ 清除当前会话，回到欢迎页
    if (agentConversation.value) {
      agent.clearCurrentConversation();
      agent.clearPlan();
      agent.clearPendingQuestion();
      agent.clearRecalledMemories();
    }
    return;
  }
  if (agentConversation.value?.id === conversationId) {
    return;
  }

  let conversation = agent.conversations.value.find((item) => item.id === conversationId);
  if (!conversation) {
    await agent.loadConversations();
    conversation = agent.conversations.value.find((item) => item.id === conversationId);
  }
  if (conversation) {
    await agent.selectConversation(conversation);
  }
}

watch(
  () => [route.query.conversation, agent.conversations.value.map((item) => item.id).join("|")],
  () => {
    void syncConversationFromRoute();
  },
  { immediate: true },
);

watch(
  () => workspace.isReady,
  (ready) => {
    if (ready && !loaded.value) {
      loaded.value = true;
      void loadAll();
    } else if (!ready) {
      loaded.value = false;
    }
  },
  { immediate: true },
);

// 切换对话时更新侧边栏上下文名称
watch(
  () => agentConversation.value?.id,
  () => {
    projectDir.setContextName(agentConversation.value?.title ?? null);
  },
);

// 回到欢迎页（无对话）时清除上下文名称
watch(
  () => route.query.conversation,
  (convId) => {
    if (!convId) {
      projectDir.setContextName(null);
    }
  },
);

watch(
  hasPendingOrRunning,
  (active) => {
    if (active && !pollTimer) {
      pollTimer = setInterval(() => {
        void history.load();
      }, 2500);
    } else if (!active && pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  },
  { immediate: true },
);

watch(
  () => orderedTasks.value.length,
  async () => {
    await nextTick();
    scrollToBottom();
  },
);

// Agent 分组数量变化（新消息追加/加载历史对话）触发滚动
watch(
  () => agentGroups.value.length,
  async () => {
    await nextTick();
    requestAnimationFrame(() => {
      scrollToBottom();
      // 兜底：部分浏览器在 rAF 后仍未完成布局
      setTimeout(scrollToBottom, 80);
    });
  },
);

// Agent 工具调用状态变化（pending → running → succeeded）也触发滚动，
// 确保用户能看到最新的工具执行进度。
watch(
  () => agent.invocations.value.map((i) => `${i.id}:${i.status}`).join("|"),
  async () => {
    await nextTick();
    scrollToBottom();
  },
);

// 生成结果图片自动添加到工作记忆画布
const prevInvocationStatuses = ref<Map<string, string>>(new Map());
const invocationNodeIds = ref<Map<string, string>>(new Map());
watch(
  () => agent.invocations.value,
  (invocations) => {
    for (const inv of invocations) {
      const prev = prevInvocationStatuses.value.get(inv.id);

      // Agent 画布工具（直写后端）成功后实时刷新画布内容
      if (
        inv.toolName?.startsWith("canvas_") &&
        inv.status === "succeeded" &&
        prev !== "succeeded"
      ) {
        memoryCanvasRef.value?.refreshNodes();
      }

      // 只有生成类工具才需要在画布上建 pending 节点；画布工具的节点由后端直写、
      // 成功后走上面的 refreshNodes()。此前此处无筛选：画布工具一进 running 就会
      // 凭空造出「摘要=工具名」的幻影 image 节点（2026-10-01 0338e63 引入）。
      // 判定与回归测试见 ../generationNodeTools.ts。
      const isGenerationTool = isGenerationNodeTool(inv.toolName);

      // 当任务开始执行时，添加 pending 节点
      if (isGenerationTool && inv.status === "running" && prev !== "running") {
        const prompt: string = inv.argumentsJson
          ? (() => {
              try {
                return JSON.parse(inv.argumentsJson).prompt || inv.toolName;
              } catch {
                return inv.toolName;
              }
            })()
          : inv.toolName;
        memoryCanvasRef.value?.addGenerationNode("image", prompt, inv.id).then((nodeId) => {
          if (nodeId) invocationNodeIds.value.set(inv.id, nodeId);
        });
      }

      // 当任务成功时，更新状态并连线到上传节点
      if (
        isGenerationTool &&
        inv.status === "succeeded" &&
        prev !== "succeeded" &&
        inv.resultJson
      ) {
        // 更新节点状态
        const nodeId = invocationNodeIds.value.get(inv.id);
        if (nodeId) {
          memoryCanvasRef.value?.setNodeStatus(nodeId, "succeeded");
        }

        try {
          const data = JSON.parse(inv.resultJson);
          let imageUrl = "";
          // 工具返回 payload 里直接有 imageUrl（后端已注入）
          if (data.imageUrl && data.imageUrl.startsWith("http")) {
            imageUrl = data.imageUrl;
          } else {
            const remoteId = data.attempt?.remoteJobId || "";
            const remoteUrl = remoteId.replace(/^proxy:(image|i2i):/, "");
            if (remoteUrl && remoteUrl.startsWith("http")) {
              imageUrl = remoteUrl;
            } else if (data.localAsset?.filePath) {
              imageUrl = data.localAsset.filePath;
            }
          }

          if (imageUrl && nodeId) {
            // 把生成结果图片 URL 写入节点 payload（CanvasNodeCard 渲染用）
            memoryCanvasRef.value
              ?.setNodeImageUrl(nodeId, imageUrl)
              .catch((e) => console.warn(String(e)));
            // 连线到最近的上传节点（用户上传的参考图）
            const uploadNodes = memoryCanvasRef.value?.findRecentNodesByType("upload") || [];
            for (const uploadNode of uploadNodes.slice(-3)) {
              memoryCanvasRef.value?.addEdgeBetweenNodes(uploadNode.id, nodeId, "reference");
            }
          } else if (imageUrl && !nodeId) {
            // 没有 pending 节点时创建新节点
            memoryCanvasRef.value?.addGenerationNode("image", imageUrl, inv.id);
          }
        } catch {
          // ignore parse errors
        }
      }

      // 当任务失败时，更新节点状态
      if (isGenerationTool && inv.status === "failed" && prev !== "failed") {
        const nodeId = invocationNodeIds.value.get(inv.id);
        if (nodeId) {
          memoryCanvasRef.value?.setNodeStatus(nodeId, "failed");
        }
      }

      prevInvocationStatuses.value.set(inv.id, inv.status);
    }
  },
  { deep: true },
);

// Agent 循环阶段切换（thinking → executing → idle）触发滚动
watch(agentLoopPhase, async () => {
  await nextTick();
  scrollToBottom();
});

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as Node | null;
  if (modelMenuRef.value && target && !modelMenuRef.value.contains(target)) {
    modelMenuOpen.value = false;
  }
  if (modeMenuRef.value && target && !modeMenuRef.value.contains(target)) {
    showModeMenu.value = false;
  }
  if (aspectMenuRef.value && target && !aspectMenuRef.value.contains(target)) {
    aspectMenuOpen.value = false;
  }
  if (durationMenuRef.value && target && !durationMenuRef.value.contains(target)) {
    durationMenuOpen.value = false;
  }
  if (resolutionMenuRef.value && target && !resolutionMenuRef.value.contains(target)) {
    resolutionMenuOpen.value = false;
  }
  if (frameRateMenuRef.value && target && !frameRateMenuRef.value.contains(target)) {
    frameRateMenuOpen.value = false;
  }
}

/* ── 悬浮 Composer：实测高度，为对话流预留底部空间 ── */
const composerEl = ref<InstanceType<typeof PromptComposer> | null>(null);
const showExtensionPanel = ref(false);
const selectedSkillIds = ref<string[]>(loadSelectedSkillIds());
watch(selectedSkillIds, (ids) => saveSelectedSkillIds(ids), { deep: true });
const conversationEl = ref<HTMLElement | null>(null);
let composerResizeObserver: ResizeObserver | null = null;

// Composer 高度会随模式芯片换行、参数栏、参考图、多行输入动态变化，
// 用 ResizeObserver 实测高度写入 --composer-reserve，对话流据此预留底部空间，
// 保证滚动到底时最后一条消息不会被悬浮输入框遮挡。
watch(
  composerEl,
  (instance) => {
    composerResizeObserver?.disconnect();
    composerResizeObserver = null;
    const el = instance?.$el;
    if (!(el instanceof HTMLElement)) return;
    composerResizeObserver = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const height = Math.ceil(entry.target.getBoundingClientRect().height);
      conversationEl.value?.style.setProperty("--composer-reserve", `${height}px`);
    });
    composerResizeObserver.observe(el);
  },
  { flush: "post" },
);

onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  composerResizeObserver?.disconnect();
  composerResizeObserver = null;
  document.removeEventListener("click", onDocumentClick);
  window.removeEventListener(
    "aigc-agent-conversations-updated",
    handleConversationsUpdatedExternally,
  );
});

document.addEventListener("click", onDocumentClick);

/**
 * 侧栏右键删除/重命名会话后会广播该事件：重新拉取会话列表，
 * 若当前选中的会话已被删除，则清空选中状态并移除路由参数。
 */
function handleConversationsUpdatedExternally(): void {
  void agent.loadConversations().then(() => {
    const current = agentConversation.value;
    if (current && !agent.conversations.value.some((item) => item.id === current.id)) {
      agent.clearCurrentConversation();
      if (routeConversationId()) {
        void router.replace({ path: "/generations" });
      }
    }
  });
}

window.addEventListener("aigc-agent-conversations-updated", handleConversationsUpdatedExternally);

function scrollToBottom(): void {
  // 优先滚动 ConversationRail 内部的 .grok-stream（实际可滚动容器）
  const stream = conversationRef.value?.querySelector(".grok-stream");
  if (stream) {
    stream.scrollTop = stream.scrollHeight;
    return;
  }
  const el = conversationRef.value;
  if (el) el.scrollTop = el.scrollHeight;
}

function pickCredential(id: string): void {
  selectedCredentialId.value = id;
  selectedAccountId.value = ""; // 凭据与账号互斥，直接生成模式凭据优先
  modelMenuOpen.value = false;
}

function pickAccount(id: string): void {
  selectedAccountId.value = id;
  selectedCredentialId.value = ""; // 清除凭据选择，使用账号
  modelMenuOpen.value = false;
}

function removeReferenceImage(index: number): void {
  referenceImages.value.splice(index, 1);
}

// 参考图变化时自动触发语义分析，无需手动点击。
watch(
  () => referenceImages.value.length,
  (newLen, oldLen) => {
    if (newLen > 0 && newLen !== oldLen) {
      void analyzeAllReferences();
    }
  },
);

/**
 * 分析所有参考图片。
 */
async function analyzeAllReferences(): Promise<void> {
  if (referenceImages.value.length === 0) return;

  const items: [string, string][] = referenceImages.value.map((img, idx) => [
    `ref-${Date.now()}-${idx}`,
    img.dataUrl,
  ]);

  try {
    const results = await semanticAnalysis.analyzeBatch(items);
    for (let i = 0; i < results.length; i++) {
      const result = results[i];
      if (result) {
        analysisResults.value.set(referenceImages.value[i].name, {
          caption: result.profile.caption ?? "",
          tags: result.profile.tags.map((t: { name: string }) => t.name),
        });
      }
    }
    if (results.length > 0) {
      showAnalysisPanel.value = true;
    }
  } catch (err) {
    console.error("[SemanticAnalysis] Batch failed:", err);
  }
}

/**
 * 将拖入的文件加入参考图/附件列表。
 * 使用 FileReader 读取为 base64 data URL，供 LLM 多模态消息使用。
 * 使用 Set 去重，避免同名文件重复加入。
 */
function addReferenceFiles(files: FileList | null | undefined): void {
  if (!files || files.length === 0) return;
  const seen = new Set(referenceImages.value.map((r) => r.name));
  for (let i = 0; i < files.length; i += 1) {
    const file = files.item(i);
    if (!file) continue;
    if (seen.has(file.name)) continue;
    if (file.size > 20 * 1024 * 1024) continue; // 20MB per file
    seen.add(file.name);
    const reader = new FileReader();
    reader.onload = () => {
      referenceImages.value.push({
        name: file.name,
        dataUrl: reader.result as string,
        mimeType: file.type || "application/octet-stream",
      });
    };
    reader.readAsDataURL(file);
  }
}

/** 文件拖拽逻辑（dragenter/over/leave/drop）已抽离到 PromptComposer。 */

function applyExamplePrompt(example: ExamplePrompt): void {
  creationMode.value = example.mode;
  promptText.value = example.prompt;
  // 滚动到底部让用户立即看到已填入的 prompt
  nextTick(() => scrollToBottom());
  const textarea = document.querySelector<HTMLTextAreaElement>(".prompt-input");
  textarea?.focus();
}

function navigateToCredentials(): void {
  void router.push("/models");
}

async function send(): Promise<void> {
  const prompt = promptText.value.trim();
  if (!prompt) return;
  if (sending.value || agentSending.value) {
    toast.info("Agent 正在处理中，请等待回复完成后再发送。");
    return;
  }
  const credential = selectedCredential.value;
  const account = selectedAccount.value;

  if (!credential && !account) {
    sendError.value = "请先选择一个模型或连接 AI 账号（在「模型管理」中配置）。";
    toast.error(sendError.value);
    return;
  }

  if (isAgentMode.value && !credential) {
    sendError.value =
      "Agent 对话需要 LLM 凭据（如 Grok / OpenAI）。即梦账号仅用于图片/视频生成，请切换到「图片生成」或「视频生成」模式。";
    toast.error(sendError.value);
    return;
  }

  sendError.value = null;

  if (isAgentMode.value && credential) {
    const skillOverride = buildSystemPromptOverride(
      selectedSkillIds.value,
      projectDir.projectMemory,
    );
    await sendAgentMessage(prompt, credential, skillOverride);
    return;
  }

  // 直接生成模式：优先用 credential，其次用 account
  sending.value = true;
  try {
    const enrichedPrompt = buildEnrichedPrompt(prompt);
    const providerName = credential?.providerName ?? account?.providerId ?? "";
    const modelName = credential?.modelName ?? `${account?.providerId ?? "jimeng"}-account`;
    const task = await history.createTask({
      providerName,
      modelName,
      promptText: enrichedPrompt,
    });

    // 触发实际生成流程（通过 queue 系统提交 attempt，PollWorker 接管轮询）
    const isVideo = creationMode.value === "video";
    const requestSnapshot: Record<string, unknown> = {
      model: modelName,
      provider: providerName,
      prompt: enrichedPrompt,
      task_type: isVideo ? "video_generation" : "image_generation",
    };
    if (isVideo && videoDuration.value) {
      // 提取数字部分："4s" → 4, "10s" → 10
      const durationNum = parseInt(videoDuration.value, 10);
      requestSnapshot.duration = isNaN(durationNum) ? 5 : durationNum;
      requestSnapshot.resolution = resolution.value || "720p";
      requestSnapshot.ratio = aspectRatio.value || "16:9";
    } else {
      requestSnapshot.ratio = aspectRatio.value || "1:1";
    }
    if (referenceImages.value.length > 0) {
      requestSnapshot.referenceImages = referenceImages.value.map((r) => ({
        name: r.name,
        mimeType: r.mimeType,
        dataUrl: r.dataUrl,
      }));
      // 视频模式：传递首帧图片 URL（图生视频）
      if (isVideo) {
        requestSnapshot.reference_image_url = referenceImages.value[0].dataUrl;
      }
    }

    const credentialId = credential?.id ?? account?.id ?? "";
    const providerId = credential?.providerName ?? account?.providerId ?? "jimeng";
    await queueV1SubmitAttempt({
      taskId: task.id,
      credentialId,
      capability: isVideo ? "text-to-video" : "text-to-image",
      providerId,
      requestSnapshotJson: JSON.stringify(requestSnapshot),
    });

    promptText.value = "";
    referenceImages.value = [];
    await nextTick();
    scrollToBottom();
  } catch (error) {
    sendError.value = error instanceof Error ? error.message : "提交失败，请稍后重试。";
  } finally {
    sending.value = false;
  }
}

function buildAttachments(): AttachmentInput[] | undefined {
  if (referenceImages.value.length === 0) return undefined;
  return referenceImages.value.map((ref) => ({
    name: ref.name,
    mimeType: ref.mimeType,
    dataUrl: ref.dataUrl,
  }));
}

/**
 * Agent 模式发送：首次发送时自动创建会话（标题用 prompt 截断）。
 * 后续发送复用当前会话。事件流通过 useAgentConversation 实时刷新 UI。
 * 失败时保留 promptText 以便重试。
 */
async function sendAgentMessage(
  prompt: string,
  credential: CredentialRecord,
  skillOverride?: string,
): Promise<void> {
  try {
    // 当切换了凭据（与当前对话绑定的不同）时，自动新建对话
    if (agentConversation.value && agentConversation.value.credentialId !== credential.id) {
      agent.clearCurrentConversation();
      agent.clearPlan();
      agent.clearPendingQuestion();
      agent.clearRecalledMemories();
      lastDraft.value = "";
      if (route.query.conversation) {
        await router.replace({ path: "/generations" });
      }
    }

    if (!agentConversation.value) {
      const title = prompt.length > 40 ? prompt.slice(0, 40) + "…" : prompt;
      const conversation = await agent.createConversation({
        title,
        credentialId: credential.id,
      });
      if (!conversation) {
        toast.error(agent.errorMessage.value ?? "创建会话失败。");
        return;
      }
      await router.replace({ path: "/generations", query: { conversation: conversation.id } });
      window.dispatchEvent(new CustomEvent("aigc-agent-conversations-updated"));
    }
    await agent.sendMessage(prompt, buildAttachments(), skillOverride);
    console.log(
      "[Canvas] sendMessage completed, error:",
      agent.errorMessage.value,
      "conversationId:",
      agentConversation.value?.id,
    );
    if (!agent.errorMessage.value) {
      // 将用户上传的图片添加到记忆画布
      const uploadedImages = [...referenceImages.value];
      console.log(
        "[Canvas] uploadedImages count:",
        uploadedImages.length,
        "memoryCanvasRef:",
        !!memoryCanvasRef.value,
      );
      const uploadNodeIds: string[] = [];
      for (const img of uploadedImages) {
        // 如果有文件路径，先进行本地分析获取描述
        let description = "";
        if (img.filePath && img.mimeType.startsWith("image/")) {
          try {
            const analysis = await analyzeImage(img.filePath);
            description = analysis.description;
          } catch (e) {
            console.warn("[Canvas] image analysis failed:", e);
          }
        }
        const nodeId = await memoryCanvasRef.value?.addUploadNode(
          "image",
          img.name,
          img.dataUrl,
          description,
        );
        console.log("[Canvas] addUploadNode result:", nodeId, "for:", img.name);
        if (nodeId) uploadNodeIds.push(nodeId);
      }
      // 将上传节点之间互相连线（reference 关系）
      if (uploadNodeIds.length > 1) {
        for (let i = 1; i < uploadNodeIds.length; i++) {
          await memoryCanvasRef.value?.addEdgeBetweenNodes(
            uploadNodeIds[0],
            uploadNodeIds[i],
            "reference",
          );
        }
      }

      promptText.value = "";
      referenceImages.value = [];
      lastDraft.value = "";
    } else {
      toast.error(agent.errorMessage.value);
    }
    await nextTick();
    scrollToBottom();
  } catch (error) {
    sendError.value = error instanceof Error ? error.message : "Agent 调用失败，请稍后重试。";
    toast.error(sendError.value);
  }
}

/**
 * Agent 模式下用户主动开新会话：清空当前选中，下一次 send 即触发新建。
 */
function startNewAgentConversation(): void {
  agent.clearCurrentConversation();
  agent.clearPlan();
  agent.clearPendingQuestion();
  agent.clearRecalledMemories();
  promptText.value = "";
  referenceImages.value = [];
  lastDraft.value = "";
  if (route.query.conversation) {
    void router.replace({ path: "/generations" });
  }
  nextTick(() => scrollToBottom());
}

/**
 * 回答 Agent 的提问：将答案作为新消息发送，恢复 Agent 循环。
 */
async function answerQuestion(answer: string): Promise<void> {
  agent.clearPendingQuestion();
  const credential = selectedCredential.value;
  if (credential) {
    await sendAgentMessage(answer, credential);
  }
}

function buildEnrichedPrompt(base: string): string {
  const parts: string[] = [`[${currentMode.value.label}]`, base];
  const generationHint = buildGenerationHint(selectedSkillIds.value);
  if (generationHint) {
    parts.push(generationHint);
  }
  if (creationMode.value === "image" || creationMode.value === "video") {
    parts.push(`比例:${aspectRatio.value}`);
  }
  if (creationMode.value === "video") {
    parts.push(
      `时长:${videoDuration.value}`,
      `分辨率:${resolution.value}`,
      `帧率:${frameRate.value}`,
    );
  }
  if (referenceImages.value.length > 0) {
    parts.push(`参考图:${referenceImages.value.length}张`);
  }
  return parts.join(" ");
}

function onPromptKeydown(event: KeyboardEvent): void {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    void send();
  }
}

async function attachResult(task: GenerationTaskRecord): Promise<void> {
  recordError.value = null;
  recordBusyTaskId.value = task.id;
  try {
    const filePath = await pickGenerationOutputFile();
    if (!filePath) return;
    await history.attachOutput(task.id, filePath);
  } catch (error) {
    recordError.value = error instanceof Error ? error.message : "录入结果失败。";
  } finally {
    recordBusyTaskId.value = null;
  }
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}
</script>

<template>
  <!-- ═══════════════════════════════════════════════
       创意工坊：工作台风格 · 顶部状态条 · 底部 Composer
       ═══════════════════════════════════════════════ -->
  <div class="grok-shell">
    <!-- ═══ 空状态：居中输入框欢迎页 ═══ -->
    <div v-if="isEmpty" class="grok-welcome">
      <CreativeEmptyState
        :current-mode="currentMode"
        :examples="EXAMPLES_BY_MODE[creationMode]"
        :is-agent-mode="isAgentMode"
        :has-credentials="credentials.length > 0 || resourceAccounts.some((a) => a.enabled)"
        :prompt-text="promptText"
        :placeholder="promptPlaceholder"
        :is-sending="sending || agentSending"
        :can-send="canSend"
        :is-dragging-file="isDraggingFile"
        :primary-modes="PRIMARY_MODES"
        :extra-modes="EXTRA_MODES"
        :creation-mode="creationMode"
        :show-mode-menu="showModeMenu"
        :credentials="credentials"
        :selected-credential-id="selectedCredentialId"
        :resource-accounts="resourceAccounts"
        :selected-account-id="selectedAccountId"
        :aspect-ratio="aspectRatio"
        :video-duration="videoDuration"
        :resolution="resolution"
        :frame-rate="frameRate"
        :memory-status="memoryStatus"
        :project-directory="projectDir.selectedPath"
        :project-display-name="projectDir.displayName"
        :reference-images="referenceImages"
        :recent-projects="projectDir.recentDirs"
        @pick="applyExamplePrompt"
        @configure-credentials="navigateToCredentials"
        @update:prompt-text="promptText = $event"
        @send="send"
        @prompt-keydown="onPromptKeydown"
        @update:creation-mode="creationMode = $event as CreationMode"
        @update:show-mode-menu="showModeMenu = $event"
        @select-credential="pickCredential"
        @select-account="pickAccount"
        @update:aspect-ratio="aspectRatio = $event"
        @update:video-duration="videoDuration = $event"
        @update:resolution="resolution = $event"
        @update:frame-rate="frameRate = $event"
        @drop-files="addReferenceFiles($event)"
        @remove-reference="referenceImages.splice($event, 1)"
        @select-directory="projectDir.selectDirectory()"
        @select-project="projectDir.selectRecent($event)"
        @reset-directory="projectDir.resetToDefault()"
        @toggle-mcp="showExtensionPanel = !showExtensionPanel"
      />
    </div>

    <!-- ═══ 会话状态：对话流 + Composer（顶部栏已移除，对话区直接贴合创意工坊） ═══ -->
    <div v-else ref="conversationEl" class="grok-conversation">
      <Transition name="drawer">
        <aside v-if="showPreferences" class="grok-prefs">
          <div class="prefs-head">
            <span>创作记忆</span>
            <button type="button" class="prefs-close" @click="showPreferences = false">
              <X :size="14" />
            </button>
          </div>
          <div class="prefs-section">
            <CreativeMemoryPanel :user-id="creativeMemoryUserId" />
          </div>
        </aside>
      </Transition>

      <!-- 工作记忆画布面板 -->
      <MemoryCanvasPanel
        ref="memoryCanvasRef"
        :conversation-id="agentConversation?.id ?? null"
        :visible="memoryCanvas.visible"
        @close="memoryCanvas.close()"
      />

      <ConversationRail
        :agent-groups="conversationGroups"
        :tasks="orderedTasks"
        :is-agent-mode="isAgentMode"
        :agent-working="agentWorking"
        :agent-loop-phase="agentLoopPhase"
        :agent-working-label="agentWorkingLabel"
        :agent-token-usage="agentTokenUsage"
        :streaming-content="agentStreamingContent"
        :is-streaming="agentIsStreaming"
        :is-invocation-expanded="isInvocationExpanded"
        :record-busy-task-id="recordBusyTaskId"
        :record-error="recordError"
        :tool-label="toolLabel"
        :pretty-json="prettyJson"
        :format-time="formatTime"
        :current-plan="currentPlan"
        :plan-round-message-id="planRoundMessageId"
        :pending-question="pendingQuestion"
        :recalled-memories="recalledMemories"
        @toggle-invocation="toggleInvocation"
        @attach-result="attachResult"
        @answer-question="answerQuestion"
      />

      <!-- 语义分析结果面板（分析由 watch 自动触发，无需手动按钮） -->
      <div v-if="referenceImages.length > 0" class="semantic-analysis-section">
        <!-- 分析中指示器 -->
        <div
          v-if="semanticAnalysis.isAnalyzing.value"
          class="analyze-btn"
          style="pointer-events: none; opacity: 0.7"
        >
          <LoaderCircle :size="14" class="spin" />
          <span>分析中...</span>
        </div>

        <!-- 分析结果面板 -->
        <Transition name="analysis-panel">
          <div v-if="showAnalysisPanel && analysisResults.size > 0" class="analysis-panel">
            <div class="analysis-header">
              <span class="analysis-title">语义分析结果</span>
              <button type="button" class="analysis-close" @click="showAnalysisPanel = false">
                <X :size="12" />
              </button>
            </div>
            <div class="analysis-content">
              <div v-for="[name, result] in analysisResults" :key="name" class="analysis-item">
                <div class="analysis-item-name">{{ name }}</div>
                <div class="analysis-caption">{{ result.caption }}</div>
                <div v-if="result.tags.length > 0" class="analysis-tags">
                  <span v-for="tag in result.tags" :key="tag" class="analysis-tag">{{ tag }}</span>
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </div>

      <PromptComposer
        ref="composerEl"
        v-model="promptText"
        v-model:show-mode-menu="showModeMenu"
        v-model:is-dragging-file="isDraggingFile"
        :creation-mode="creationMode"
        :primary-modes="PRIMARY_MODES"
        :extra-modes="EXTRA_MODES"
        :current-mode="currentMode"
        :placeholder="promptPlaceholder"
        :is-sending="sending || agentSending"
        :can-send="canSend"
        :has-credentials="credentials.length > 0 || resourceAccounts.some((a) => a.enabled)"
        :is-agent-mode="isAgentMode"
        :reference-images="referenceImages"
        :credentials="credentials"
        :selected-credential-id="selectedCredentialId"
        :resource-accounts="resourceAccounts"
        :selected-account-id="selectedAccountId"
        :aspect-ratio="aspectRatio"
        :video-duration="videoDuration"
        :resolution="resolution"
        :frame-rate="frameRate"
        :char-limit="8000"
        :send-error="sendError"
        :is-extra-mode="isExtraMode(creationMode)"
        :show-preferences="showPreferences"
        :project-directory="projectDir.selectedPath"
        :project-display-name="projectDir.displayName"
        :active-skill-count="selectedSkillIds.length"
        :recent-projects="projectDir.recentDirs"
        @update:creation-mode="creationMode = $event as CreationMode"
        @send="send"
        @remove-reference="removeReferenceImage"
        @select-credential="pickCredential"
        @select-account="pickAccount"
        @update:aspect-ratio="aspectRatio = $event"
        @update:video-duration="videoDuration = $event"
        @update:resolution="resolution = $event"
        @update:frame-rate="frameRate = $event"
        @prompt-keydown="onPromptKeydown"
        @drop-files="addReferenceFiles($event)"
        @start-new-agent-conversation="startNewAgentConversation"
        @toggle-preferences="showPreferences = !showPreferences"
        @select-directory="projectDir.selectDirectory()"
        @select-project="projectDir.selectRecent($event)"
        @reset-directory="projectDir.resetToDefault()"
        @toggle-mcp="showExtensionPanel = !showExtensionPanel"
      />
    </div>

    <!-- 扩展面板（工具/技能/MCP）：置于页面根层，两种布局下均可唤起 -->
    <ExtensionPanel
      v-model="selectedSkillIds"
      :visible="showExtensionPanel"
      @close="showExtensionPanel = false"
    />
  </div>
</template>

<style scoped>
.grok-shell {
  position: relative;
  display: flex;
  width: 100%;
  height: 100%;
  background: var(--color-canvas);
  color: var(--color-text);
  overflow: hidden;
}
.grok-welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  padding: var(--page-padding);
}
.grok-conversation {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  overflow: hidden;
}
.grok-prefs {
  position: absolute;
  top: 56px;
  right: 20px;
  width: 280px;
  padding: 14px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .grok-prefs {
  background: rgba(255, 255, 255, 0.95);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.1);
}

.prefs-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.9);
  margin-bottom: 12px;
}

:root:not([data-theme="dark"]) .prefs-head {
  color: rgba(0, 0, 0, 0.85);
}

.prefs-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: rgba(255, 255, 255, 0.55);
  cursor: pointer;
}

.prefs-close:hover {
  background: rgba(255, 255, 255, 0.08);
}

.prefs-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.prefs-label {
  font-size: 11px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
.drawer-enter-active,
.drawer-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
@media (prefers-reduced-motion: reduce) {
  .turn-enter-active,
  .turn-leave-active,
  .menu-enter-active,
  .menu-leave-active,
  .params-enter-active,
  .params-leave-active,
  .drawer-enter-active,
  .drawer-leave-active,
  .fade-enter-active,
  .fade-leave-active {
    transition-duration: 0.01ms !important;
  }
}
.grok-shell {
  height: calc(100vh - var(--topbar-height) - (var(--page-padding) * 2));
  min-height: 560px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  /* 深色底：让 PrismaticBurst 棱镜光带透出层次 */
  background: var(--color-canvas);
  box-shadow: var(--shadow-sm);
  isolation: isolate;
}

.grok-prefs {
  top: 76px;
  border-radius: var(--radius-surface);
  background: var(--material-sheet);
  border-color: var(--color-border-subtle);
  box-shadow: var(--shadow-menu);
}

.prefs-head,
.prefs-mode,
:root:not([data-theme="dark"]) .prefs-head,
:root:not([data-theme="dark"]) .prefs-mode {
  color: var(--color-text);
}

.prefs-label {
  color: var(--color-text-secondary);
  letter-spacing: 0;
  text-transform: none;
}

@media (max-width: 720px) {
  .grok-shell {
    height: auto;
    min-height: calc(100vh - var(--topbar-height));
    border-right: 0;
    border-left: 0;
    border-radius: 0;
  }
}
.grok-conversation .grok-composer.grok-composer {
  position: absolute;
  right: 0;
  bottom: 0;
  left: 0;
  z-index: 25;
  margin: 0;
  padding: 12px 28px 18px;
  border-top: none;
  background: linear-gradient(
    to top,
    var(--color-canvas) 0%,
    color-mix(in srgb, var(--color-canvas) 96%, transparent) 40%,
    color-mix(in srgb, var(--color-canvas) 80%, transparent) 70%,
    transparent 100%
  );
  backdrop-filter: blur(10px) saturate(140%);
  -webkit-backdrop-filter: blur(10px) saturate(140%);
  pointer-events: none;
}
.grok-conversation .grok-composer.grok-composer.is-drag-active {
  pointer-events: auto;
}
.grok-conversation :deep(.composer-box.composer-box) {
  pointer-events: auto;
  background: var(--material-sheet);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  border: 1px solid var(--color-border-subtle);
  border-radius: 16px;
  box-shadow: var(--shadow-lg);
}

:root:not([data-theme="dark"]) .grok-conversation :deep(.composer-box.composer-box) {
  background: var(--material-sheet);
  border-color: var(--color-border-subtle);
  box-shadow: var(--shadow-lg);
}

@media (max-width: 640px) {
  .grok-conversation .grok-composer.grok-composer {
    padding: 8px 12px 12px;
  }
}
.semantic-analysis-section {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 0 28px;
  z-index: 26;
}

.analyze-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: 1px solid var(--color-border);
  border-radius: 8px;
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  pointer-events: auto;
}

.analyze-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  border-color: var(--color-primary);
  color: var(--color-primary);
}

.analyze-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.analyze-btn .spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.analysis-panel {
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  width: min(400px, 90vw);
  max-height: 300px;
  margin-bottom: 8px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 12px;
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  pointer-events: auto;
}

.analysis-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--color-border-subtle);
}

.analysis-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text);
}

.analysis-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
}

.analysis-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.analysis-content {
  padding: 10px 14px;
  overflow-y: auto;
  max-height: 240px;
}

.analysis-item {
  padding: 8px 0;
}

.analysis-item + .analysis-item {
  border-top: 1px solid var(--color-border-subtle);
}

.analysis-item-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text);
  margin-bottom: 4px;
}

.analysis-caption {
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.5;
  margin-bottom: 6px;
}

.analysis-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.analysis-tag {
  display: inline-block;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--color-primary-soft);
  color: var(--color-primary);
  font-size: 10px;
}

.analysis-panel-enter-active,
.analysis-panel-leave-active {
  transition:
    opacity 180ms ease,
    transform 180ms ease;
}

.analysis-panel-enter-from,
.analysis-panel-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}
</style>
