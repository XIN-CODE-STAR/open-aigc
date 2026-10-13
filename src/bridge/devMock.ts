/**
 * 开发期 mock 桥接 —— **仅用于浏览器调试**。
 *
 * 背景：浏览器里没有 Tauri 运行时，`invokeNative()` 会直接抛
 * `desktop_runtime_required`，所有数据页只能看到报错，无法审阅真实观感。
 * 本模块在「dev 且非 Tauri」时按命令名返回一份示例数据。
 *
 * 硬约束（改动前务必确认）：
 *  1. 只在 `import.meta.env.DEV && !isTauri()` 时被 `native.ts` **动态 import**，
 *     生产构建里这段代码不会进包，也不会被执行。
 *  2. 返回值必须能通过调用方传入的 zod schema 校验；校验失败会照旧抛错，
 *     因此这里的字段要和 `src/bridge/*.ts` 里的 schema 保持一致。
 *  3. 返回 `undefined` 表示「该命令没有 mock」——调用方回退到桌面运行时错误。
 *
 * 新增 mock：在 `MOCKS` 里加一条，命令名从 `src/bridge/*.ts` 的
 * `invokeNative("...")` 复制，不要手写。
 */
import { isTauri } from "@tauri-apps/api/core";

export function isDevMockEnabled(): boolean {
  // vitest 下 MODE === "test"：测试必须走真实的「无运行时」分支，
  // 否则会打破断言 `desktop_runtime_required` 的用例（assets / workspace / backup）。
  if (import.meta.env.MODE === "test") return false;
  return import.meta.env.DEV && !isTauri();
}

/* ──────────────────────────────────────────────
 *  固定 ID（必须是合法 UUID：schema 用 z.uuid() 校验）
 * ────────────────────────────────────────────── */

const WORKSPACE_ID = "11111111-1111-4111-8111-111111111111";
const OWNER_ID = "22222222-2222-4222-8222-222222222222";
const CRED_CHAT = "33333333-3333-4333-8333-333333333301";
const CRED_VIDEO = "33333333-3333-4333-8333-333333333302";
const CRED_LOCAL = "33333333-3333-4333-8333-333333333303";
const CONV_1 = "44444444-4444-4444-8444-444444444401";
const CONV_2 = "44444444-4444-4444-8444-444444444402";
const CONV_3 = "44444444-4444-4444-8444-444444444403";
const MSG_1 = "55555555-5555-4555-8555-555555555501";
const MSG_2 = "55555555-5555-4555-8555-555555555502";
const MSG_3 = "55555555-5555-4555-8555-555555555503";
const INV_1 = "66666666-6666-4666-8666-666666666601";

/** dev 预览用的公开示例视频（CC0）：让画布/资产的视频链路可见。 */
const SAMPLE_VIDEO_URL = "https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4";
const ASSET_A = "77777777-7777-4777-8777-777777777701";
const ASSET_B = "77777777-7777-4777-8777-777777777702";
const ASSET_C = "77777777-7777-4777-8777-777777777703";
const ASSET_D = "77777777-7777-4777-8777-777777777704";
const ASSET_E = "77777777-7777-4777-8777-777777777705";

/* ──────────────────────────────────────────────
 *  画布（memory canvas）——内存态 mock
 *  浏览器预览没有真实 IPC，这里把画布/节点/连线/视口保存在模块内存，
 *  让「生成 → 画布节点带媒体」这条链路在 dev 下也能完整看到。
 * ────────────────────────────────────────────── */

const CANVAS_ID = "88888888-8888-4888-8888-888888888801";
const NODE_UPLOAD_ID = "99999999-9999-4999-8999-999999999901";
const NODE_VIDEO_ID = "99999999-9999-4999-8999-999999999902";
const EDGE_REF_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaa01";

interface MockCanvasRecord {
  id: string;
  workspaceId: string;
  name: string;
  description: string | null;
  nodeCount: number;
  edgeCount: number;
  createdAt: string;
  updatedAt: string;
}

interface MockCanvasNode {
  id: string;
  canvasId: string;
  nodeType: string;
  positionX: number;
  positionY: number;
  width: number | null;
  height: number | null;
  payloadJson: string;
  summary: string | null;
  assetId: string | null;
  createdAt: string;
  updatedAt: string;
}

interface MockCanvasEdge {
  id: string;
  canvasId: string;
  sourceNodeId: string;
  targetNodeId: string;
  edgeType: string;
  label: string | null;
  createdAt: string;
}

const demoNodes: MockCanvasNode[] = [
  {
    id: NODE_UPLOAD_ID,
    canvasId: CANVAS_ID,
    nodeType: "upload",
    positionX: 80,
    positionY: 60,
    width: 220,
    height: null,
    payloadJson: JSON.stringify({
      imageUrl: placeholderDataUrl("reference-beach.png"),
      status: "succeeded",
    }),
    summary: "参考图：傍晚海面",
    assetId: ASSET_A,
    createdAt: ago(2),
    updatedAt: ago(2),
  },
  {
    id: NODE_VIDEO_ID,
    canvasId: CANVAS_ID,
    nodeType: "video",
    positionX: 360,
    positionY: 140,
    width: 220,
    height: null,
    payloadJson: JSON.stringify({
      imageUrl: SAMPLE_VIDEO_URL,
      status: "succeeded",
      prompt: "傍晚暖光的海浪",
    }),
    summary: "傍晚暖光的海浪（5 秒）",
    assetId: null,
    createdAt: ago(1),
    updatedAt: ago(1),
  },
];

const demoEdges: MockCanvasEdge[] = [
  {
    id: EDGE_REF_ID,
    canvasId: CANVAS_ID,
    sourceNodeId: NODE_UPLOAD_ID,
    targetNodeId: NODE_VIDEO_ID,
    edgeType: "reference",
    label: null,
    createdAt: ago(1),
  },
];

const demoViewport = { canvasId: CANVAS_ID, zoom: 0.9, panX: 0, panY: 0, updatedAt: ago(0) };

function demoCanvasRecord(): MockCanvasRecord {
  return {
    id: CANVAS_ID,
    workspaceId: WORKSPACE_ID,
    // 画布按「conv-<会话ID>」命名查找（useCanvasStore.loadCanvas），
    // 这里挂到 demo 会话上，展开画布即可看到节点与媒体。
    name: `conv-${CONV_1}`,
    description: null,
    nodeCount: demoNodes.length,
    edgeCount: demoEdges.length,
    createdAt: ago(3),
    updatedAt: new Date().toISOString(),
  };
}

/** 把 `{request}` 里的可选字符串安全转成 string | null。 */
function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

/** 相对「现在」偏移若干天，返回 RFC3339 字符串。 */
function ago(days: number, hours = 0): string {
  return new Date(Date.now() - days * 86_400_000 - hours * 3_600_000).toISOString();
}

/** 取 `{ request: {...} }` 里的 request；不是对象时返回空对象。 */
function requestOf(args?: Record<string, unknown>): Record<string, unknown> {
  const request = args?.request;
  if (typeof request === "object" && request !== null) {
    return request as Record<string, unknown>;
  }
  return {};
}

/* ──────────────────────────────────────────────
 *  示例数据
 * ────────────────────────────────────────────── */

const WORKSPACE_STATUS = {
  initialized: true,
  workspace: {
    workspaceId: WORKSPACE_ID,
    workspaceName: "本地工作区",
    teacherId: OWNER_ID,
    teacherName: "本地用户",
    createdAt: ago(42),
  },
  database: {
    schemaVersion: 35,
    sqliteVersion: "3.46.0",
    journalMode: "wal",
    foreignKeysEnabled: true,
  },
  storage: {
    directoriesReady: true,
    writable: true,
    manifestReady: true,
    assetCount: 5,
    totalBytes: 18_432_000,
    missingAssetCount: 0,
    checkedAt: ago(0),
  },
};

const CREDENTIALS = [
  {
    id: CRED_CHAT,
    providerName: "deepseek",
    displayName: "DeepSeek 对话",
    baseUrl: "https://api.deepseek.com/v1",
    modelName: "deepseek-chat",
    credentialKey: `credential:${CRED_CHAT}`,
    enabled: true,
    createdAt: ago(20),
    updatedAt: ago(1, 2),
    credentialType: "api_key",
    scope: "user",
    authMode: "api_key",
    status: "ok",
    lastHealthCheckAt: ago(0, 3),
  },
  {
    id: CRED_VIDEO,
    providerName: "volcengine",
    displayName: "火山 Seedance（视频）",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    modelName: "seedance-2-0-250601",
    credentialKey: `credential:${CRED_VIDEO}`,
    enabled: true,
    createdAt: ago(12),
    updatedAt: ago(2),
    credentialType: "api_key",
    scope: "user",
    authMode: "api_key",
    status: "degraded",
    lastHealthCheckAt: ago(0, 6),
  },
  {
    id: CRED_LOCAL,
    providerName: "openai-compatible",
    displayName: "本地 Ollama",
    baseUrl: "http://127.0.0.1:11434/v1",
    modelName: "qwen2.5:14b",
    credentialKey: `credential:${CRED_LOCAL}`,
    enabled: false,
    createdAt: ago(30),
    updatedAt: ago(9),
    credentialType: "local_endpoint",
    scope: "workspace",
    authMode: "manual",
    status: "unknown",
  },
];

const RESOURCE_ACCOUNTS = [
  {
    id: "88888888-8888-4888-8888-888888888801",
    providerId: "jimeng",
    accountType: "web",
    displayName: "即梦账号 A",
    status: "active",
    credentialKey: "resource-account:88888888-8888-4888-8888-888888888801",
    baseUrl: "http://127.0.0.1:5100",
    extraJson: "{}",
    enabled: true,
    createdAt: ago(15),
    updatedAt: ago(0, 4),
    lastHealthCheckAt: ago(0, 4),
  },
  {
    id: "88888888-8888-4888-8888-888888888802",
    providerId: "jimeng",
    accountType: "web",
    displayName: "即梦账号 B（需重新登录）",
    status: "need_login",
    credentialKey: "resource-account:88888888-8888-4888-8888-888888888802",
    baseUrl: "http://127.0.0.1:5100",
    extraJson: "{}",
    enabled: true,
    createdAt: ago(15),
    updatedAt: ago(1, 6),
    lastHealthCheckAt: ago(1, 6),
  },
];

const ASSETS = [
  {
    id: ASSET_A,
    storageNamespace: "generation",
    assetKind: "image",
    displayName: "海浪 01.png",
    relativePath: "generation/2026/10/wave-01.png",
    sizeBytes: 2_411_520,
    sha256: "a".repeat(64),
    mimeType: "image/png",
    integrityStatus: "valid",
    metadataJson: "{}",
    originDeviceId: null,
    revision: 1,
    createdAt: ago(3),
    updatedAt: ago(3),
  },
  {
    id: ASSET_B,
    storageNamespace: "generation",
    assetKind: "image",
    displayName: "产品主图 v2.png",
    relativePath: "generation/2026/10/product-hero-v2.png",
    sizeBytes: 3_882_112,
    sha256: "b".repeat(64),
    mimeType: "image/png",
    integrityStatus: "valid",
    metadataJson: "{}",
    originDeviceId: null,
    revision: 2,
    createdAt: ago(5),
    updatedAt: ago(4),
  },
  {
    id: ASSET_C,
    storageNamespace: "generation",
    assetKind: "video",
    displayName: "分镜成片 30s.mp4",
    relativePath: "generation/2026/10/storyboard-final.mp4",
    sizeBytes: 11_534_336,
    sha256: "c".repeat(64),
    mimeType: "video/mp4",
    integrityStatus: "valid",
    metadataJson: "{}",
    originDeviceId: null,
    revision: 1,
    createdAt: ago(2),
    updatedAt: ago(2),
  },
  {
    id: ASSET_D,
    storageNamespace: "workspace",
    assetKind: "document",
    displayName: "剧本大纲.md",
    relativePath: "workspace/script-outline.md",
    sizeBytes: 12_288,
    sha256: "d".repeat(64),
    mimeType: "text/markdown",
    integrityStatus: "valid",
    metadataJson: "{}",
    originDeviceId: null,
    revision: 1,
    createdAt: ago(8),
    updatedAt: ago(8),
  },
  {
    id: ASSET_E,
    storageNamespace: "generation",
    assetKind: "image",
    displayName: "参考图（已丢失）.png",
    relativePath: "generation/2026/09/missing-ref.png",
    sizeBytes: 1_048_576,
    sha256: null,
    mimeType: "image/png",
    integrityStatus: "missing",
    metadataJson: "{}",
    originDeviceId: null,
    revision: 1,
    createdAt: ago(22),
    updatedAt: ago(21),
  },
];

const CONVERSATIONS = [
  {
    id: CONV_1,
    workspaceId: WORKSPACE_ID,
    title: "生成一段 5 秒的海浪视频",
    credentialId: CRED_VIDEO,
    systemPrompt: null,
    status: "active",
    executionMode: "plan-and-execute",
    loopStateJson: null,
    revision: 1,
    createdAt: ago(1),
    updatedAt: ago(1),
  },
  {
    id: CONV_2,
    workspaceId: WORKSPACE_ID,
    title: "暗色调科技感的主视觉方案",
    credentialId: CRED_CHAT,
    systemPrompt: null,
    status: "active",
    executionMode: "plan-and-execute",
    loopStateJson: null,
    revision: 1,
    createdAt: ago(4),
    updatedAt: ago(4),
  },
  {
    id: CONV_3,
    workspaceId: WORKSPACE_ID,
    title: "在画布上创建一条便签，记录项目主线",
    credentialId: CRED_CHAT,
    systemPrompt: null,
    status: "archived",
    executionMode: "react",
    loopStateJson: null,
    revision: 2,
    createdAt: ago(9),
    updatedAt: ago(7),
  },
];

const MESSAGES = [
  {
    id: MSG_1,
    conversationId: CONV_1,
    role: "user",
    content: "生成一段 5 秒的海浪视频，要傍晚的暖光",
    toolCalls: [],
    toolCallId: null,
    remoteModel: null,
    finishReason: null,
    parentMessageId: null,
    revision: 1,
    createdAt: ago(1),
    promptTokens: null,
    completionTokens: null,
  },
  {
    id: MSG_2,
    conversationId: CONV_1,
    role: "assistant",
    content: "已按「傍晚暖光 + 5 秒」提交视频生成任务，稍等片刻。",
    toolCalls: [
      {
        id: "call_demo_1",
        type: "function",
        function: { name: "video_generation", arguments: '{"prompt":"傍晚暖光的海浪"}' },
      },
    ],
    toolCallId: null,
    remoteModel: "deepseek-chat",
    finishReason: "tool_calls",
    parentMessageId: null,
    revision: 1,
    createdAt: ago(1),
    promptTokens: 812,
    completionTokens: 96,
  },
  {
    id: MSG_3,
    conversationId: CONV_1,
    role: "assistant",
    content: "视频已生成完成，可在右侧预览或到资产库查看。",
    toolCalls: [],
    toolCallId: null,
    remoteModel: "deepseek-chat",
    finishReason: "stop",
    parentMessageId: null,
    revision: 1,
    createdAt: ago(1),
    promptTokens: 1024,
    completionTokens: 48,
  },
];

const INVOCATIONS = [
  {
    id: INV_1,
    messageId: MSG_2,
    conversationId: CONV_1,
    toolName: "video_generation",
    argumentsJson: '{"prompt":"傍晚暖光的海浪","durationSeconds":5}',
    // 浏览器预览没有真实生成能力：给一个可播放的公开示例视频，
    // 让画布节点 / 资产预览的媒体链路在 dev 下也能被完整看到。
    resultJson: JSON.stringify({
      status: "succeeded",
      imageUrl: SAMPLE_VIDEO_URL,
      attempt: { id: "attempt_demo_1", status: "succeeded", resultUrl: SAMPLE_VIDEO_URL },
      localAsset: null,
    }),
    status: "succeeded",
    errorMessage: null,
    generationTaskId: null,
    startedAt: ago(1),
    completedAt: ago(1),
    createdAt: ago(1),
  },
];

/* ──────────────────────────────────────────────
 *  命令 → 处理函数
 * ────────────────────────────────────────────── */

type MockHandler = (args?: Record<string, unknown>) => unknown;

const MOCKS: Record<string, MockHandler> = {
  workspace_v1_get_status: () => WORKSPACE_STATUS,
  workspace_v1_get_root_path: () => ({
    workspaceDir: "C:\\OpenAIGC-Demo\\workspace",
    managedFilesDir: "C:\\OpenAIGC-Demo\\workspace\\files",
  }),

  credential_v1_list: () => CREDENTIALS,
  resource_account_v1_list: () => RESOURCE_ACCOUNTS,
  agent_v1_get_llm_credential: () => ({ credentialId: CRED_CHAT }),

  asset_v1_list: (args) => {
    const request = requestOf(args);
    const search = typeof request.search === "string" ? request.search.trim() : "";
    const namespace = request.storageNamespace;
    const kind = request.assetKind;
    const integrity = request.integrityStatus;
    return ASSETS.filter((asset) => {
      if (search && !asset.displayName.includes(search)) return false;
      if (typeof namespace === "string" && asset.storageNamespace !== namespace) return false;
      if (typeof kind === "string" && asset.assetKind !== kind) return false;
      if (typeof integrity === "string" && asset.integrityStatus !== integrity) return false;
      return true;
    });
  },

  agent_v1_list_conversations: () => CONVERSATIONS,
  agent_v1_get_conversation: (args) => {
    const id = requestOf(args).id;
    return CONVERSATIONS.find((item) => item.id === id) ?? null;
  },
  // 所有会话都返回同一段示例对话，保证任意会话点开都有内容可看。
  agent_v1_list_messages: () => MESSAGES,
  agent_v1_list_invocations: () => INVOCATIONS,

  /* ── 画布（内存态：增删改即时生效） ── */
  memory_canvas_v1_list: () => [demoCanvasRecord()],
  memory_canvas_v1_get: () => demoCanvasRecord(),
  memory_canvas_v1_create: (args) => {
    const request = requestOf(args);
    return { ...demoCanvasRecord(), name: optionalString(request.name) ?? "工作记忆画布" };
  },
  memory_canvas_v1_delete: () => undefined,

  memory_node_v1_list: (args) => {
    const request = requestOf(args);
    return demoNodes.filter((node) => node.canvasId === request.canvas_id);
  },
  memory_node_v1_add: (args) => {
    const request = requestOf(args);
    const now = new Date().toISOString();
    const node: MockCanvasNode = {
      id: crypto.randomUUID(),
      canvasId: String(request.canvas_id ?? CANVAS_ID),
      nodeType: String(request.node_type ?? "image"),
      positionX: Number(request.position_x ?? 0),
      positionY: Number(request.position_y ?? 0),
      width: null,
      height: null,
      payloadJson: String(request.payload_json ?? "{}"),
      summary: optionalString(request.summary),
      assetId: optionalString(request.asset_id),
      createdAt: now,
      updatedAt: now,
    };
    demoNodes.push(node);
    return node;
  },
  memory_node_v1_update: (args) => {
    const request = requestOf(args);
    const index = demoNodes.findIndex((node) => node.id === request.id);
    const base = index >= 0 ? demoNodes[index] : undefined;
    const now = new Date().toISOString();
    const updated: MockCanvasNode = {
      id: String(request.id ?? base?.id ?? crypto.randomUUID()),
      canvasId: String(request.canvas_id ?? base?.canvasId ?? CANVAS_ID),
      nodeType: String(request.node_type ?? base?.nodeType ?? "image"),
      positionX: Number(request.position_x ?? base?.positionX ?? 0),
      positionY: Number(request.position_y ?? base?.positionY ?? 0),
      width: base?.width ?? null,
      height: base?.height ?? null,
      payloadJson: String(request.payload_json ?? base?.payloadJson ?? "{}"),
      summary:
        request.summary === undefined ? (base?.summary ?? null) : optionalString(request.summary),
      assetId:
        request.asset_id === undefined ? (base?.assetId ?? null) : optionalString(request.asset_id),
      createdAt: base?.createdAt ?? now,
      updatedAt: now,
    };
    if (index >= 0) demoNodes[index] = updated;
    return updated;
  },
  memory_node_v1_delete: (args) => {
    const request = requestOf(args);
    const index = demoNodes.findIndex((node) => node.id === request.id);
    if (index >= 0) demoNodes.splice(index, 1);
    return undefined;
  },

  memory_edge_v1_list: (args) => {
    const request = requestOf(args);
    return demoEdges.filter((edge) => edge.canvasId === request.canvas_id);
  },
  memory_edge_v1_add: (args) => {
    const request = requestOf(args);
    const edge: MockCanvasEdge = {
      id: crypto.randomUUID(),
      canvasId: String(request.canvas_id ?? CANVAS_ID),
      sourceNodeId: String(request.source_node_id ?? ""),
      targetNodeId: String(request.target_node_id ?? ""),
      edgeType: String(request.edge_type ?? "reference"),
      label: optionalString(request.label),
      createdAt: new Date().toISOString(),
    };
    demoEdges.push(edge);
    return edge;
  },
  memory_edge_v1_delete: (args) => {
    const request = requestOf(args);
    const index = demoEdges.findIndex((edge) => edge.id === request.id);
    if (index >= 0) demoEdges.splice(index, 1);
    return undefined;
  },
  memory_edge_v1_update_label: (args) => {
    const request = requestOf(args);
    const edge = demoEdges.find((item) => item.id === request.id);
    if (edge) edge.label = optionalString(request.label);
    return undefined;
  },

  memory_viewport_v1_get: () => demoViewport,
  memory_viewport_v1_save: (args) => {
    const request = requestOf(args);
    demoViewport.zoom = Number(request.zoom ?? demoViewport.zoom);
    demoViewport.panX = Number(request.pan_x ?? demoViewport.panX);
    demoViewport.panY = Number(request.pan_y ?? demoViewport.panY);
    demoViewport.updatedAt = new Date().toISOString();
    return undefined;
  },
};

/** 命中则返回 mock 数据；未命中返回 undefined（调用方回退到桌面运行时错误）。 */
export function resolveDevMock(command: string, args?: Record<string, unknown>): unknown {
  if (!isDevMockEnabled()) return undefined;
  if (!placeholderInstalled) {
    placeholderInstalled = true;
    installAssetPlaceholder();
  }
  const handler = MOCKS[command];
  if (!handler) return undefined;
  return handler(args);
}

/* ──────────────────────────────────────────────
 *  资产缩略图占位
 * ────────────────────────────────────────────── */

let placeholderInstalled = false;

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** 生成一张内联 SVG 占位图：按文件名派生色相的渐变「作品图」+ 文件名。
 *  浏览器预览没有真实资产文件，用渐变图形代替纯灰占位，让资产库/画布看起来有内容。 */
function placeholderDataUrl(filePath: string): string {
  const name = escapeXml(filePath.split(/[\\/]/).pop() ?? "asset");
  let hue = 0;
  for (const ch of name) hue = (hue * 31 + ch.charCodeAt(0)) % 360;
  const c1 = `hsl(${hue} 46% 74%)`;
  const c2 = `hsl(${(hue + 42) % 360} 42% 52%)`;
  const c3 = `hsl(${(hue + 320) % 360} 40% 62%)`;
  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="320" viewBox="0 0 480 320">',
    "<defs>",
    '<linearGradient id="g" x1="0" y1="0" x2="1" y2="1">',
    `<stop offset="0%" stop-color="${c1}"/>`,
    `<stop offset="100%" stop-color="${c2}"/>`,
    "</linearGradient>",
    "</defs>",
    '<rect width="480" height="320" fill="url(#g)"/>',
    `<circle cx="118" cy="104" r="62" fill="${c3}" opacity="0.55"/>`,
    `<circle cx="352" cy="196" r="96" fill="#ffffff" opacity="0.16"/>`,
    '<path d="M0 268 L132 176 L236 244 L330 168 L480 262 L480 320 L0 320 Z" fill="#000000" opacity="0.18"/>',
    '<path d="M0 292 L148 226 L268 286 L372 224 L480 288 L480 320 L0 320 Z" fill="#000000" opacity="0.12"/>',
    `<text x="24" y="40" font-family="monospace" font-size="14" fill="#ffffff" opacity="0.9">${name}</text>`,
    "</svg>",
  ].join("");
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/**
 * 浏览器里 `convertFileSrc()` 会抛错（它读 `window.__TAURI_INTERNALS__`），
 * 资产缩略图因此全是破图。这里只补一个 `convertFileSrc`，把 mock 路径映射成占位图。
 *
 * ⚠️ **绝不能设置 `globalThis.isTauri`** —— `isTauri()` 读的正是那个字段，
 * 一旦设上，本模块的 mock 会整体失效并转去调用真实 IPC。
 */
function installAssetPlaceholder(): void {
  const scope = window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> };
  const internals = scope.__TAURI_INTERNALS__ ?? {};
  if (typeof internals.convertFileSrc === "function") return;
  internals.convertFileSrc = (filePath: unknown): string => {
    if (typeof filePath !== "string") return "";
    // 视频资产：返回可播放的示例视频，让悬停预览/播放器在 dev 下真的能播
    if (/\.(mp4|mov|webm|m4v)$/i.test(filePath)) return SAMPLE_VIDEO_URL;
    return placeholderDataUrl(filePath);
  };
  scope.__TAURI_INTERNALS__ = internals;
}
