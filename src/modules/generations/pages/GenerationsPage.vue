<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Film,
  Image as ImageIcon,
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
import { useRoute, useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

// Phase 1：组件拆分
import ConversationRail from "../components/ConversationRail.vue";
import CreativeEmptyState from "../components/CreativeEmptyState.vue";
import PromptComposer from "../components/PromptComposer.vue";
import ShotWorkspace from "../components/ShotWorkspace.vue";
import CreativeMemoryPanel from "../components/CreativeMemoryPanel.vue";
import ModelRouterSelector from "../components/ModelRouterSelector.vue";
import MemoryCanvasPanel from "../../../modules/memory/components/MemoryCanvasPanel.vue";
import { useMangaProject } from "../composables/useMangaProject";
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
const manga = useMangaProject();
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
const selectedProviderId = ref<string>("");
const selectedModelName = ref<string>("");
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

// 切换项目/会话时，更新侧边栏显示的上下文名称
watch(
  () => manga.currentProject.value,
  (proj) => {
    if (proj) {
      projectDir.setContextName(proj.title);
    }
  },
);

// 切换对话时更新上下文名称（不在漫画项目中时）
watch(
  () => agentConversation.value?.id,
  () => {
    if (!manga.isInProject.value) {
      projectDir.setContextName(agentConversation.value?.title ?? null);
    }
  },
);

// 回到欢迎页（无对话）时清除上下文名称
watch(
  () => route.query.conversation,
  (convId) => {
    if (!convId && !manga.isInProject.value) {
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

      // 当任务开始执行时，添加 pending 节点
      if (inv.status === "running" && prev !== "running") {
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
      if (inv.status === "succeeded" && prev !== "succeeded" && inv.resultJson) {
        // 更新节点状态
        const nodeId = invocationNodeIds.value.get(inv.id);
        if (nodeId) {
          memoryCanvasRef.value?.setNodeStatus(nodeId, "succeeded");
        }

        try {
          const data = JSON.parse(inv.resultJson);
          let imageUrl = "";
          const remoteId = data.attempt?.remoteJobId || "";
          const remoteUrl = remoteId.replace(/^proxy:image:/, "").replace(/^image:/, "");
          if (remoteUrl && remoteUrl.startsWith("http")) {
            imageUrl = remoteUrl;
          } else if (data.localAsset?.filePath) {
            imageUrl = data.localAsset.filePath;
          }

          if (imageUrl && nodeId) {
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
      if (inv.status === "failed" && prev !== "failed") {
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
    await sendAgentMessage(prompt, credential);
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
async function sendAgentMessage(prompt: string, credential: CredentialRecord): Promise<void> {
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
    await agent.sendMessage(prompt, buildAttachments());
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

/**
 * 添加场景到当前项目。
 */
async function handleAddScene(): Promise<void> {
  const title = prompt("场景名称：");
  if (!title?.trim()) return;
  const index = manga.scenes.value.length;
  await manga.addScene(title.trim(), index);
}

/**
 * 添加镜头到指定场景。
 */
async function handleAddShot(sceneId: string): Promise<void> {
  const index = manga.shots.value.filter((s) => s.sceneId === sceneId).length;
  await manga.addShot(sceneId, index);
}

/**
 * 更新镜头 Prompt。
 */
function handleUpdatePrompt(shotId: string, prompt: string): void {
  // TODO: 调用 manga.updateShotPrompt(shotId, prompt) 保存到后端
  console.log("更新镜头 Prompt:", shotId, prompt);
}

/**
 * 为镜头创建生成任务。
 */
async function handleGenerate(shotId: string, prompt: string): Promise<void> {
  const credential = selectedCredential.value;
  if (!credential) {
    sendError.value = "请先选择一个模型。";
    return;
  }
  await manga.generateForShot(shotId, prompt, {
    credentialId: credential.id,
    providerId: selectedProviderId.value || credential.providerName,
    providerName: credential.providerName,
    modelName: selectedModelName.value || credential.modelName,
    capability: creationMode.value === "video" ? "text-to-video" : "text-to-image",
  });
}

function buildEnrichedPrompt(base: string): string {
  const parts: string[] = [`[${currentMode.value.label}]`, base];
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
    <!-- ═══ 项目工作台：选择项目后显示 ═══ -->
    <ShotWorkspace
      v-if="manga.isInProject.value"
      :project="manga.currentProject.value!"
      :scenes="manga.scenes.value"
      :shots="manga.shots.value"
      :characters="manga.characters.value"
      :selected-shot-id="manga.selectedShotId.value"
      :shot-attempts="manga.shotAttempts.value"
      :generating="manga.generating.value"
      :project-id="manga.currentProject.value!.id"
      @back="manga.leaveProject()"
      @add-scene="handleAddScene"
      @add-shot="handleAddShot"
      @select-shot="(s) => manga.selectShot(s)"
      @update-prompt="handleUpdatePrompt"
      @generate="handleGenerate"
      @cancel="(id) => manga.cancelAttempt(id)"
      @retry="(id) => manga.retryAttempt(id)"
    />

    <!-- ═══ 空状态：居中输入框欢迎页 ═══ -->
    <div v-else-if="isEmpty" class="grok-welcome">
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
        @reset-directory="projectDir.resetToDefault()"
      />
    </div>

    <!-- ═══ 会话状态：对话流 + Composer（顶部栏已移除，对话区直接贴合创意工坊） ═══ -->
    <div v-else ref="conversationEl" class="grok-conversation">
      <Transition name="drawer">
        <aside v-if="showPreferences" class="grok-prefs">
          <div class="prefs-head">
            <span>偏好</span>
            <button type="button" class="prefs-close" @click="showPreferences = false">
              <X :size="14" />
            </button>
          </div>
          <div class="prefs-section">
            <span class="prefs-label">创作类型</span>
            <div class="prefs-mode-grid">
              <button
                v-for="mode in CREATION_MODES"
                :key="mode.id"
                type="button"
                class="prefs-mode"
                :class="{ 'is-active': creationMode === mode.id }"
                @click="creationMode = mode.id"
              >
                <component :is="mode.icon" :size="14" />
                <span>{{ mode.label }}</span>
              </button>
            </div>
          </div>
          <!-- AI 模型路由选择器 -->
          <div class="prefs-section">
            <span class="prefs-label">AI 模型</span>
            <ModelRouterSelector
              :task-type="
                creationMode === 'video'
                  ? 'video_generation'
                  : creationMode === 'image'
                    ? 'image_generation'
                    : 'text_generation'
              "
              :auto-route="true"
              @select-model="
                (p, m) => {
                  selectedProviderId = p;
                  selectedModelName = m;
                }
              "
            />
          </div>
          <!-- 创意记忆面板 -->
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
        @reset-directory="projectDir.resetToDefault()"
      />
    </div>
  </div>
</template>

<style scoped>
/* ════════════════════════════════════════════════
   创意工坊 · 简约浅色 · 居中欢迎页 + 对话模式
   ════════════════════════════════════════════════ */

.grok-shell {
  position: relative;
  display: flex;
  width: 100%;
  height: 100%;
  background: var(--color-canvas);
  color: var(--color-text);
  overflow: hidden;
}

/* —— 欢迎页：居中内容 + 项目列表 —— */
.grok-welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  padding: var(--page-padding);
}

/* —— 会话模式：纵向布局 —— */
.grok-conversation {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  overflow: hidden;
}

/* —— 顶部极简浮层 —— */
.grok-topbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 20px;
  z-index: 20;
  pointer-events: none;
  background: linear-gradient(to bottom, var(--color-canvas) 0%, transparent 100%);
}

.topbar-side {
  display: flex;
  align-items: center;
  gap: 8px;
  pointer-events: auto;
}

.topbar-side--right {
  justify-content: flex-end;
}

.topbar-mode {
  font-size: 13px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.55);
  letter-spacing: 0.02em;
}

:root:not([data-theme="dark"]) .topbar-mode {
  color: rgba(60, 60, 67, 0.65);
}

.topbar-conv {
  font-size: 13px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.4);
  margin-left: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 240px;
}

:root:not([data-theme="dark"]) .topbar-conv {
  color: rgba(60, 60, 67, 0.5);
}

.topbar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.55);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

:root:not([data-theme="dark"]) .topbar-btn {
  color: rgba(60, 60, 67, 0.6);
}

.topbar-btn:hover {
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.9);
}

:root:not([data-theme="dark"]) .topbar-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: rgba(0, 0, 0, 0.85);
}

.topbar-btn.is-active {
  background: rgba(255, 255, 255, 0.1);
  color: var(--color-accent);
}

:root:not([data-theme="dark"]) .topbar-btn.is-active {
  background: rgba(0, 0, 0, 0.06);
}

/* —— 错误浮层 —— */
.grok-error {
  position: absolute;
  top: 56px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  border-radius: 999px;
  background: rgba(220, 38, 38, 0.12);
  border: 1px solid rgba(220, 38, 38, 0.3);
  color: #ff6b6b;
  font-size: 12px;
  z-index: 25;
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

/* —— 偏好抽屉 —— */
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

.prefs-mode-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 6px;
}

.prefs-mode {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.8);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.prefs-mode:hover {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.15);
}

.prefs-mode.is-active {
  background: var(--color-accent-soft);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

:root:not([data-theme="dark"]) .prefs-mode {
  color: rgba(0, 0, 0, 0.75);
  border-color: rgba(0, 0, 0, 0.08);
}

:root:not([data-theme="dark"]) .prefs-mode:hover {
  background: rgba(0, 0, 0, 0.04);
  border-color: rgba(0, 0, 0, 0.15);
}

/* —— 对话流：流式纯文本 —— */
.grok-stream {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  /* 底部为悬浮 Composer 预留空间：--composer-reserve 由 JS ResizeObserver 实测写入。 */
  padding: 80px 20px calc(var(--composer-reserve, 220px) + 8px);
  scroll-behavior: smooth;
}

.grok-messages {
  max-width: 768px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.turn {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* —— 用户消息：右对齐浅胶囊 —— */
.turn-user {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 6px;
  align-self: flex-end;
  max-width: 80%;
}

.user-text {
  margin: 0;
  padding: 10px 14px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.95);
  font-size: 14px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

:root:not([data-theme="dark"]) .user-text {
  background: rgba(0, 0, 0, 0.05);
  color: rgba(0, 0, 0, 0.9);
}

.turn-meta {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.35);
}

:root:not([data-theme="dark"]) .turn-meta {
  color: rgba(60, 60, 67, 0.5);
}

.turn-meta .dot {
  opacity: 0.6;
}

/* —— AI 回复：左对齐纯文本流，无卡片 —— */
.turn-ai {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-self: flex-start;
  max-width: 80%;
}

.ai-status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: rgba(255, 255, 255, 0.75);
  flex-wrap: wrap;
}

:root:not([data-theme="dark"]) .ai-status {
  color: rgba(0, 0, 0, 0.7);
}

.turn-ai--pending .ai-status,
.turn-ai--running .ai-status {
  color: rgba(255, 255, 255, 0.6);
}

.turn-ai--succeeded .ai-status {
  color: rgba(52, 199, 89, 0.95);
}

.turn-ai--failed .ai-status {
  color: rgba(255, 159, 130, 0.95);
}

.ai-status-text {
  font-weight: 500;
}

/* —— Agent 模式：助手文本 + 工具调用进度 —— */
.ai-text {
  margin: 0;
  font-size: 14px;
  line-height: 1.6;
  color: rgba(255, 255, 255, 0.92);
  white-space: pre-wrap;
  word-break: break-word;
}

:root:not([data-theme="dark"]) .ai-text {
  color: rgba(0, 0, 0, 0.88);
}

.ai-invocations {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
  padding: 6px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
}

:root:not([data-theme="dark"]) .ai-invocations {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.06);
}

.invocation {
  display: flex;
  flex-direction: column;
  border-radius: 8px;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.05);
  font-size: 11px;
  color: rgba(255, 255, 255, 0.7);
  transition: border-color var(--duration-fast) var(--ease-out);
}

:root:not([data-theme="dark"]) .invocation {
  background: rgba(0, 0, 0, 0.02);
  border-color: rgba(0, 0, 0, 0.05);
  color: rgba(0, 0, 0, 0.7);
}

.invocation.is-expanded {
  border-color: rgba(255, 255, 255, 0.12);
}

:root:not([data-theme="dark"]) .invocation.is-expanded {
  border-color: rgba(0, 0, 0, 0.12);
}

.invocation-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  width: 100%;
}

.invocation-head:hover {
  background: rgba(255, 255, 255, 0.04);
}

:root:not([data-theme="dark"]) .invocation-head:hover {
  background: rgba(0, 0, 0, 0.04);
}

.invocation-name {
  font-weight: 500;
  flex: 1;
}

.invocation-state {
  color: rgba(255, 255, 255, 0.45);
  font-size: 10px;
}

:root:not([data-theme="dark"]) .invocation-state {
  color: rgba(60, 60, 67, 0.55);
}

.invocation-caret {
  color: rgba(255, 255, 255, 0.4);
  transition: transform var(--duration-fast) var(--ease-out);
}

:root:not([data-theme="dark"]) .invocation-caret {
  color: rgba(60, 60, 67, 0.5);
}

.invocation-caret.is-open {
  transform: rotate(180deg);
}

.invocation--succeeded .invocation-head {
  color: rgba(52, 199, 89, 0.95);
}

.invocation--failed .invocation-head {
  color: rgba(255, 159, 130, 0.95);
}

.invocation--running .invocation-head,
.invocation--pending .invocation-head {
  color: var(--color-accent);
}

/* —— 工具调用详情：参数 + 结果 JSON —— */
.invocation-detail {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 10px 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  background: rgba(0, 0, 0, 0.15);
}

:root:not([data-theme="dark"]) .invocation-detail {
  border-top-color: rgba(0, 0, 0, 0.06);
  background: rgba(0, 0, 0, 0.02);
}

.detail-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.detail-label {
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: rgba(255, 255, 255, 0.4);
}

:root:not([data-theme="dark"]) .detail-label {
  color: rgba(60, 60, 67, 0.5);
}

.detail-block--error .detail-label {
  color: rgba(255, 159, 130, 0.85);
}

.detail-code {
  margin: 0;
  padding: 8px;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.78);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 10.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 240px;
  overflow-y: auto;
}

:root:not([data-theme="dark"]) .detail-code {
  background: rgba(255, 255, 255, 0.6);
  border-color: rgba(0, 0, 0, 0.08);
  color: rgba(0, 0, 0, 0.78);
}

.detail-block--error .detail-code {
  border-color: rgba(220, 38, 38, 0.3);
  color: rgba(255, 159, 130, 0.9);
}

.detail-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--color-accent);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}

/* —— Token 用量 —— */
.ai-tokens {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  font-size: 10px;
  color: rgba(255, 255, 255, 0.35);
  font-variant-numeric: tabular-nums;
}

:root:not([data-theme="dark"]) .ai-tokens {
  color: rgba(60, 60, 67, 0.45);
}

.conv-tokens {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 12px;
  padding: 6px 10px;
  align-self: center;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.05);
  font-size: 10px;
  color: rgba(255, 255, 255, 0.4);
  font-variant-numeric: tabular-nums;
}

:root:not([data-theme="dark"]) .conv-tokens {
  background: rgba(0, 0, 0, 0.02);
  border-color: rgba(0, 0, 0, 0.05);
  color: rgba(60, 60, 67, 0.5);
}

.conv-tokens-break {
  color: rgba(255, 255, 255, 0.3);
}

:root:not([data-theme="dark"]) .conv-tokens-break {
  color: rgba(60, 60, 67, 0.4);
}

/* —— 加载状态分层动画 —— */
.ai-status--thinking {
  color: var(--color-accent);
}

.ai-status--executing {
  color: rgba(52, 199, 89, 0.9);
}

.ai-status--failed {
  color: rgba(255, 159, 130, 0.95);
}

.is-pulsing {
  animation: agent-pulse 1.6s ease-in-out infinite;
}

@keyframes agent-pulse {
  0%,
  100% {
    opacity: 0.5;
    transform: scale(0.92);
  }
  50% {
    opacity: 1;
    transform: scale(1.05);
  }
}

@media (prefers-reduced-motion: reduce) {
  .is-pulsing {
    animation: none;
  }
}

/* —— 错误浮层增强：文本 + 重试 + 关闭 —— */
.grok-error-text {
  flex: 1;
  min-width: 0;
}

.grok-error-retry {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 10px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.grok-error-retry:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.25);
}

:root:not([data-theme="dark"]) .grok-error-retry {
  border-color: rgba(0, 0, 0, 0.15);
  background: rgba(0, 0, 0, 0.04);
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .grok-error-retry:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.08);
  border-color: rgba(0, 0, 0, 0.25);
}

.grok-error-retry:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.grok-error-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: rgba(255, 255, 255, 0.5);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.grok-error-close:hover {
  background: rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.85);
}

:root:not([data-theme="dark"]) .grok-error-close {
  color: rgba(60, 60, 67, 0.5);
}

:root:not([data-theme="dark"]) .grok-error-close:hover {
  background: rgba(0, 0, 0, 0.08);
  color: rgba(0, 0, 0, 0.85);
}

.ai-fail-msg {
  margin: 4px 0 0;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
  line-height: 1.5;
}

:root:not([data-theme="dark"]) .ai-fail-msg {
  color: rgba(60, 60, 67, 0.55);
}

.ai-inline-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 999px;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.ai-inline-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.2);
}

.ai-inline-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

:root:not([data-theme="dark"]) .ai-inline-btn {
  border-color: rgba(0, 0, 0, 0.12);
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .ai-inline-btn:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
  border-color: rgba(0, 0, 0, 0.2);
}

.record-error {
  margin: 12px 0 0;
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(220, 38, 38, 0.1);
  color: #ff6b6b;
  font-size: 12px;
}

/* —— 空状态：Grok 风格大标题居中 —— */
.grok-empty {
  max-width: 640px;
  margin: 12vh auto 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  text-align: center;
}

.empty-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: 14px;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  margin-bottom: 8px;
}

.empty-title {
  margin: 0;
  font-size: 28px;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: rgba(255, 255, 255, 0.95);
  line-height: 1.2;
}

:root:not([data-theme="dark"]) .empty-title {
  color: rgba(0, 0, 0, 0.9);
}

.empty-accent {
  color: var(--color-accent);
}

.empty-sub {
  margin: 0 0 16px;
  font-size: 14px;
  color: rgba(255, 255, 255, 0.5);
  line-height: 1.5;
}

:root:not([data-theme="dark"]) .empty-sub {
  color: rgba(60, 60, 67, 0.55);
}

.empty-cta {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
  font-size: 12px;
  margin-bottom: 8px;
}

:root:not([data-theme="dark"]) .empty-cta {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.75);
}

.cta-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border: none;
  border-radius: 999px;
  background: var(--color-accent);
  color: #fff;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.cta-btn:hover {
  opacity: 0.9;
}

.empty-examples {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-top: 8px;
}

.example-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.03);
  color: rgba(255, 255, 255, 0.8);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.example-chip:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
  transform: translateY(-1px);
}

:root:not([data-theme="dark"]) .example-chip {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.75);
}

:root:not([data-theme="dark"]) .example-chip:hover {
  background: rgba(0, 0, 0, 0.06);
  border-color: rgba(0, 0, 0, 0.2);
}

/* ════════════════════════════════════════════════
   底部 Composer：大输入框居中 + 底部工具栏
   ════════════════════════════════════════════════ */
.grok-composer {
  flex-shrink: 0;
  padding: 12px 20px 20px;
  background: linear-gradient(to top, var(--color-canvas) 60%, transparent 100%);
}

.composer-box {
  max-width: 768px;
  margin: 0 auto;
  padding: 10px 12px 8px;
  border-radius: 18px;
  background: rgba(28, 28, 30, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.24);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

:root:not([data-theme="dark"]) .composer-box {
  background: rgba(255, 255, 255, 0.92);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.06);
}

/* —— 模式芯片平铺 —— */
.mode-row {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.mode-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 10px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: rgba(255, 255, 255, 0.65);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.mode-chip:hover {
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.9);
}

.mode-chip.is-active {
  background: rgba(255, 255, 255, 0.1);
  color: #ffffff;
}

:root:not([data-theme="dark"]) .mode-chip {
  color: rgba(60, 60, 67, 0.6);
}

:root:not([data-theme="dark"]) .mode-chip:hover {
  background: rgba(0, 0, 0, 0.05);
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .mode-chip.is-active {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.95);
}

.mode-more {
  position: relative;
}

.mode-caret {
  opacity: 0.6;
  transition: transform var(--duration-fast) var(--ease-out);
}

.mode-chip.is-open .mode-caret {
  transform: rotate(180deg);
}

.mode-menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  min-width: 180px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .mode-menu {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.mode-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.8);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.mode-menu-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.mode-menu-item.is-selected {
  color: var(--color-accent);
}

.mode-menu-check {
  margin-left: auto;
}

:root:not([data-theme="dark"]) .mode-menu-item {
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .mode-menu-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

/* —— 参考图预览 —— */
.ref-strip {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.ref-thumb {
  position: relative;
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  display: flex;
  align-items: center;
  justify-content: center;
  color: rgba(255, 255, 255, 0.55);
}

:root:not([data-theme="dark"]) .ref-thumb {
  background: rgba(0, 0, 0, 0.04);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(60, 60, 67, 0.55);
}

.ref-remove {
  position: absolute;
  top: -5px;
  right: -5px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: none;
  background: rgba(220, 38, 38, 0.85);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  padding: 0;
}

/* —— 大输入框 —— */
.prompt-input {
  width: 100%;
  min-height: 56px;
  max-height: 200px;
  padding: 10px 4px;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.95);
  font-size: 15px;
  line-height: 1.5;
  font-family: inherit;
  resize: none;
  outline: none;
}

.prompt-input::placeholder {
  color: rgba(255, 255, 255, 0.4);
}

:root:not([data-theme="dark"]) .prompt-input {
  color: rgba(0, 0, 0, 0.95);
}

:root:not([data-theme="dark"]) .prompt-input::placeholder {
  color: rgba(60, 60, 67, 0.45);
}

.prompt-input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* —— 拖入文件高亮 —— */
.prompt-input.is-dragover {
  border-radius: 10px;
  background: var(--color-accent-soft);
  box-shadow: inset 0 0 0 1.5px dashed var(--color-accent);
  outline: none;
}

.prompt-input.is-dragover::placeholder {
  color: var(--color-accent);
  opacity: 0.85;
}

/* —— 参数芯片行 —— */
.param-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.param-wrap {
  position: relative;
}

.param-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.param-chip:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

:root:not([data-theme="dark"]) .param-chip {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .param-chip:hover {
  background: rgba(0, 0, 0, 0.06);
  border-color: rgba(0, 0, 0, 0.2);
}

.param-ratio {
  display: inline-block;
  background: currentColor;
  opacity: 0.7;
  border-radius: 1px;
}

.param-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 14px;
  padding: 0 4px;
  border-radius: 4px;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.param-caret {
  opacity: 0.6;
  transition: transform var(--duration-fast) var(--ease-out);
}

.param-caret.is-open {
  transform: rotate(180deg);
}

.param-pop {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 140px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .param-pop {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.param-pop-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.param-pop-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.param-pop-item.is-selected {
  color: var(--color-accent);
}

.param-pop-ratio {
  display: inline-block;
  background: currentColor;
  opacity: 0.7;
  border-radius: 1px;
}

.param-pop-check {
  margin-left: auto;
}

:root:not([data-theme="dark"]) .param-pop-item {
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .param-pop-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

/* —— 底部工具栏 —— */
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding-top: 4px;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.tool-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.55);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.tool-btn:hover {
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.9);
}

:root:not([data-theme="dark"]) .tool-btn {
  color: rgba(60, 60, 67, 0.6);
}

:root:not([data-theme="dark"]) .tool-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: rgba(0, 0, 0, 0.85);
}

/* —— 模型选择 —— */
.model-picker {
  position: relative;
  margin-left: 4px;
  min-width: 0;
}

.model-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  max-width: 200px;
}

.model-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

.model-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.model-btn-label {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-btn-name {
  font-weight: 500;
}

.model-btn-provider {
  font-size: 11px;
  color: var(--color-text-tertiary);
  font-weight: 400;
}

.model-btn-caret {
  opacity: 0.6;
  transition: transform var(--duration-fast) var(--ease-out);
  flex-shrink: 0;
}

.model-btn-caret.is-open {
  transform: rotate(180deg);
}

:root:not([data-theme="dark"]) .model-btn {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .model-btn:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.06);
  border-color: rgba(0, 0, 0, 0.2);
}

.model-pop {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 220px;
  max-width: 280px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .model-pop {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.model-pop-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.model-pop-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.model-pop-item.is-selected {
  background: var(--color-accent-soft);
}

.model-pop-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1;
}

.model-pop-name {
  font-size: 12px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.95);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-pop-provider {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.45);
}

.model-pop-check {
  color: var(--color-accent);
  flex-shrink: 0;
}

:root:not([data-theme="dark"]) .model-pop-item {
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .model-pop-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

:root:not([data-theme="dark"]) .model-pop-name {
  color: rgba(0, 0, 0, 0.95);
}

:root:not([data-theme="dark"]) .model-pop-provider {
  color: rgba(60, 60, 67, 0.55);
}

/* —— 字数提示 / 发送按钮 —— */
.char-count {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.4);
  font-variant-numeric: tabular-nums;
}

:root:not([data-theme="dark"]) .char-count {
  color: rgba(60, 60, 67, 0.5);
}

.send-hint {
  font-size: 11px;
  color: rgba(255, 159, 130, 0.8);
}

.send-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border: none;
  border-radius: 50%;
  background: var(--color-accent);
  color: #ffffff;
  cursor: pointer;
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.send-btn:hover:not(:disabled) {
  opacity: 0.92;
}

.send-btn:active:not(:disabled) {
  transform: scale(0.95);
}

.send-btn:disabled {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.4);
  cursor: not-allowed;
  box-shadow: none;
}

:root:not([data-theme="dark"]) .send-btn:disabled {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.3);
}

.send-error {
  margin: 6px 0 0;
  font-size: 11px;
  color: #ff6b6b;
}

/* —— 旋转动画 —— */
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

/* —— 过渡动画增强 —— */
.turn-enter-active,
.turn-leave-active {
  transition:
    opacity var(--duration-base) var(--ease-out),
    transform var(--duration-base) var(--ease-out);
}

.turn-enter-from {
  opacity: 0;
  transform: translateY(12px) scale(0.98);
}

.turn-leave-to {
  opacity: 0;
  transform: translateY(-12px) scale(0.98);
}

/* —— 菜单动画 —— */
.menu-enter-active,
.menu-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.95);
}

/* —— 参数行动画 —— */
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
  transform: translateY(-8px);
}

.params-enter-to,
.params-leave-from {
  opacity: 1;
  max-height: 80px;
  transform: translateY(0);
}

/* —— 过渡：抽屉 —— */
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

/* —— 过渡：淡入 —— */
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-fast) var(--ease-out);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* —— reduced-motion —— */
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

  .example-chip:hover {
    transform: none;
  }
}

/* —— 响应式 —— */
@media (max-width: 640px) {
  .grok-topbar {
    padding: 10px 14px;
  }

  .grok-stream {
    padding: 70px 14px calc(var(--composer-reserve, 220px) + 8px);
  }

  .grok-composer {
    padding: 8px 12px 12px;
  }

  .grok-empty {
    margin-top: 8vh;
  }

  .empty-title {
    font-size: 22px;
  }

  .turn-user,
  .turn-ai {
    max-width: 92%;
  }
}

/* —— 创意工坊工作台优化 —— */
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

.grok-main {
  height: 100%;
  min-height: 0;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.035) 0%, transparent 28%),
    linear-gradient(180deg, var(--color-surface) 0%, var(--color-canvas) 100%);
}

/* —— 顶部工具栏 —— */
.grok-topbar {
  position: relative;
  top: auto;
  left: auto;
  right: auto;
  min-height: 62px;
  padding: 10px 20px;
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  pointer-events: auto;
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.topbar-side {
  min-width: 0;
  gap: 10px;
  flex: 1;
}

.topbar-side--right {
  flex: 0 0 auto;
}

.topbar-copy {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.topbar-title-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}

.topbar-mode {
  color: var(--color-text);
  font-size: 14px;
  font-weight: 700;
  letter-spacing: 0;
}

.topbar-conv {
  max-width: 320px;
  margin-left: 0;
  color: var(--color-text-secondary);
}

.topbar-conv::before {
  content: "/";
  margin-right: 8px;
  color: var(--color-text-tertiary);
}

.topbar-status {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  min-width: 0;
}

.topbar-status-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 260px;
  height: 22px;
  padding: 0 8px;
  overflow: hidden;
  border: 1px solid var(--color-border-subtle);
  border-radius: 7px;
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
  font-size: 11px;
  line-height: 1;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: all var(--duration-fast) var(--ease-out);
}

.topbar-status-item:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border);
}

.topbar-status-item strong {
  color: var(--color-text-tertiary);
  font-weight: 600;
}

.topbar-status-item.is-muted {
  border-color: var(--color-warning-soft);
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.topbar-btn,
.tool-btn {
  border: 1px solid transparent;
  color: var(--color-text-secondary);
  transition: all var(--duration-fast) var(--ease-out);
}

.topbar-btn:hover,
.tool-btn:hover,
:root:not([data-theme="dark"]) .topbar-btn:hover,
:root:not([data-theme="dark"]) .tool-btn:hover {
  border-color: var(--color-border-subtle);
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.topbar-btn.is-active {
  border-color: var(--color-border);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.grok-error {
  top: 76px;
  max-width: min(680px, calc(100% - 32px));
  border-radius: var(--radius-control);
  background: var(--color-danger-soft);
  color: var(--color-danger);
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

.prefs-mode {
  border-color: var(--color-border-subtle);
}

.prefs-mode:hover,
:root:not([data-theme="dark"]) .prefs-mode:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border);
}

.grok-stream {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  /* 底部为悬浮 Composer 预留空间：--composer-reserve 由 JS ResizeObserver 实测写入，
     保证滚动到底时最后一条消息不会被悬浮输入框遮挡。 */
  padding: 24px 28px calc(var(--composer-reserve, 220px) + 8px);
  scroll-behavior: smooth;
}

.grok-messages {
  max-width: 880px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.turn {
  gap: 14px;
}

.turn-user {
  max-width: min(78%, 720px);
}

.turn-ai {
  max-width: min(84%, 820px);
  padding-left: 12px;
  border-left: 2px solid var(--color-border-subtle);
}

.user-text {
  border: 1px solid var(--color-border-subtle);
  border-radius: 12px;
  background: var(--color-accent-soft);
  color: var(--color-text);
}

.turn-meta,
.ai-tokens,
.conv-tokens,
:root:not([data-theme="dark"]) .turn-meta,
:root:not([data-theme="dark"]) .ai-tokens,
:root:not([data-theme="dark"]) .conv-tokens {
  color: var(--color-text-tertiary);
}

.ai-status,
:root:not([data-theme="dark"]) .ai-status {
  color: var(--color-text-secondary);
}

.ai-text,
:root:not([data-theme="dark"]) .ai-text {
  color: var(--color-text);
  font-size: 14.5px;
  line-height: 1.65;
}

.ai-invocations,
.invocation {
  border-color: var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.invocation-detail,
:root:not([data-theme="dark"]) .invocation-detail {
  border-top-color: var(--color-border-subtle);
  background: var(--color-surface);
}

.detail-label {
  color: var(--color-text-secondary);
  letter-spacing: 0;
}

.detail-code,
:root:not([data-theme="dark"]) .detail-code {
  border-color: var(--color-border-subtle);
  background: var(--color-canvas);
  color: var(--color-text);
  font-family: var(--font-mono);
}

.grok-empty {
  flex: 1;
  max-width: 720px;
  margin: 0 auto;
  padding: 44px 16px 34px;
  justify-content: center;
}

.empty-mark {
  width: 44px;
  height: 44px;
  border-radius: var(--radius-control);
}

.empty-title {
  color: var(--color-text);
  font-size: 26px;
  letter-spacing: 0;
}

.empty-sub,
:root:not([data-theme="dark"]) .empty-sub {
  max-width: 520px;
  color: var(--color-text-secondary);
}

.empty-cta {
  border-color: var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  color: var(--color-text);
}

.example-chip {
  border-color: var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
}

.example-chip:hover,
:root:not([data-theme="dark"]) .example-chip:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.grok-composer {
  flex-shrink: 0;
  margin-top: auto;
  padding: 16px 28px 24px;
  border-top: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.composer-box {
  max-width: 720px;
  padding: 14px;
  border-radius: var(--radius-surface);
  border-color: var(--color-border);
  background: var(--material-sheet);
  box-shadow: var(--shadow-lg);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.mode-row {
  gap: 6px;
  min-height: 32px;
}

.mode-chip,
.param-chip,
.model-btn {
  min-height: 30px;
  border-color: var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  transition: all var(--duration-fast) var(--ease-out);
}

.mode-chip:hover,
.param-chip:hover,
.model-btn:hover:not(:disabled) {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.mode-chip:hover,
.param-chip:hover,
.model-btn:hover:not(:disabled),
:root:not([data-theme="dark"]) .mode-chip:hover,
:root:not([data-theme="dark"]) .param-chip:hover,
:root:not([data-theme="dark"]) .model-btn:hover:not(:disabled) {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.mode-chip.is-active,
:root:not([data-theme="dark"]) .mode-chip.is-active {
  border-color: var(--color-accent);
  background: linear-gradient(135deg, var(--color-accent) 0%, var(--aurora-cyan) 100%);
  color: #ffffff;
  box-shadow: 0 0 16px var(--aurora-glow-1);
}

.prompt-input {
  min-height: 92px;
  padding: 12px 4px;
  color: var(--color-text);
  font-size: 15px;
}

.prompt-input::placeholder {
  color: var(--color-text-tertiary);
}

.param-row {
  gap: 8px;
}

.param-pop,
.mode-menu,
.model-pop {
  border-color: var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--material-sheet);
  box-shadow: var(--shadow-menu);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  animation: menuSlideUp 0.2s var(--ease-out);
}

@keyframes menuSlideUp {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.param-pop-item,
.mode-menu-item,
.model-pop-item,
:root:not([data-theme="dark"]) .param-pop-item,
:root:not([data-theme="dark"]) .mode-menu-item,
:root:not([data-theme="dark"]) .model-pop-item {
  color: var(--color-text);
}

.param-pop-item:hover,
.mode-menu-item:hover,
.model-pop-item:hover,
:root:not([data-theme="dark"]) .param-pop-item:hover,
:root:not([data-theme="dark"]) .mode-menu-item:hover,
:root:not([data-theme="dark"]) .model-pop-item:hover {
  background: var(--color-surface-hover);
}

.toolbar {
  align-items: center;
  padding-top: 2px;
}

.toolbar-left {
  gap: 6px;
}

.toolbar-right {
  gap: 10px;
}

.model-btn {
  max-width: 240px;
}

.model-pop-name,
:root:not([data-theme="dark"]) .model-pop-name {
  color: var(--color-text);
}

.model-pop-provider,
:root:not([data-theme="dark"]) .model-pop-provider {
  color: var(--color-text-secondary);
}

.char-count,
:root:not([data-theme="dark"]) .char-count {
  color: var(--color-text-tertiary);
}

.send-btn {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-control);
  background: linear-gradient(135deg, var(--color-accent) 0%, var(--aurora-cyan) 100%);
  color: var(--color-on-accent);
  box-shadow: 0 2px 8px var(--aurora-glow-1);
  transition: all var(--duration-fast) var(--ease-out);
}

.send-btn:hover:not(:disabled) {
  opacity: 0.92;
  box-shadow: 0 4px 16px var(--aurora-glow-1);
}

.send-btn:hover:not(:disabled) {
  background: var(--color-accent-hover);
  opacity: 1;
}

.send-btn:disabled,
:root:not([data-theme="dark"]) .send-btn:disabled {
  background: var(--color-surface-hover);
  color: var(--color-text-disabled);
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

@media (max-width: 640px) {
  .grok-topbar {
    min-height: 56px;
    align-items: flex-start;
    padding: 10px 12px;
  }

  .topbar-status {
    display: none;
  }

  .topbar-conv {
    max-width: 42vw;
  }

  .grok-stream {
    padding: 16px 12px calc(var(--composer-reserve, 220px) + 8px);
  }

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
    padding-bottom: 2px;
  }

  .mode-chip {
    flex: 0 0 auto;
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

  .model-btn {
    max-width: 154px;
  }

  .turn-user,
  .turn-ai {
    max-width: 100%;
  }

  .empty-title {
    font-size: 22px;
  }
}

/* ════════════════════════════════════════════════
   悬浮 Composer（仅会话模式）
   将 composer 移出文档流，作为悬浮卡片叠在对话流之上：
   - 去掉原整层的顶边框/实底背景/模糊，让对话流延伸到底部、融为一体；
   - 底部用画布同色淡出渐变做视觉过渡，避免硬边遮挡；
   - footer 自身 pointer-events: none，空白区域不拦截对话流交互，
     composer-box 本体恢复可交互；
   - 对话流通过 --composer-reserve（见 .grok-stream padding）预留底部空间，
     滚动到底不遮文字。
   欢迎页（.grok-welcome）不受该规则影响，保留原有在流整层样式。
   ════════════════════════════════════════════════ */
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

/* 拖拽文件时 footer 需要接收事件，覆盖上方 pointer-events: none */
.grok-conversation .grok-composer.grok-composer.is-drag-active {
  pointer-events: auto;
}

/* 注意：composer-box 是 PromptComposer 内部元素（非组件根节点），
   不带本页面的 scoped 属性，必须用 :deep() 穿透子组件才能命中。 */
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

/* ── 语义分析区域 ── */
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
