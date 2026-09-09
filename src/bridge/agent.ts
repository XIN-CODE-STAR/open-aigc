import { z } from "zod";

import { firstValidationError, invokeNative, NativeCommandError } from "./native";

// ──────────────────────────────────────────────────────────────────
// Domain schemas (must mirror src-tauri/src/domain/agent.rs exactly)
// ──────────────────────────────────────────────────────────────────

const conversationStatusSchema = z.enum(["active", "archived", "error"]);
const executionModeSchema = z.enum(["plan-and-execute", "react"]);
const planStepStatusSchema = z.enum(["pending", "in-progress", "completed", "failed", "skipped"]);
const planStepKindSchema = z.enum([
  "task",
  "image_generation",
  "video_generation",
  "audio_generation",
  "composite",
  "critic",
  "style_setup",
]);
const messageRoleSchema = z.enum(["system", "user", "assistant", "tool"]);
const toolInvocationStatusSchema = z.enum(["pending", "running", "succeeded", "failed", "skipped"]);

const functionCallSchema = z
  .object({
    name: z.string().min(1),
    arguments: z.string(),
  })
  .passthrough();

const toolCallSchema = z
  .object({
    id: z.string(),
    type: z.literal("function"),
    function: functionCallSchema,
  })
  .passthrough();

const conversationRecordSchema = z
  .object({
    id: z.string().min(1, "会话标识无效。"),
    workspaceId: z.string().min(1, "工作空间标识无效。"),
    title: z.string().min(1).max(200),
    credentialId: z.string().min(1, "凭据标识无效。"),
    systemPrompt: z.string().max(8000).nullable(),
    status: conversationStatusSchema,
    executionMode: executionModeSchema,
    loopStateJson: z.string().nullable(),
    revision: z.number().int().positive(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .passthrough();

const messageRecordSchema = z
  .object({
    id: z.string().min(1, "消息标识无效。"),
    conversationId: z.string().min(1, "会话标识无效。"),
    role: messageRoleSchema,
    content: z.string().nullable(),
    toolCalls: z.array(toolCallSchema),
    toolCallId: z.string().nullable(),
    remoteModel: z.string().nullable(),
    finishReason: z.string().nullable(),
    parentMessageId: z.string().nullable(),
    revision: z.number().int().positive(),
    createdAt: z.string().min(1),
    promptTokens: z.number().int().nonnegative().nullable(),
    completionTokens: z.number().int().nonnegative().nullable(),
  })
  .passthrough();

const toolInvocationRecordSchema = z
  .object({
    id: z.string().min(1, "调用标识无效。"),
    messageId: z.string().min(1, "消息标识无效。"),
    conversationId: z.string().min(1, "会话标识无效。"),
    toolName: z.string().min(1).max(80),
    argumentsJson: z.string().min(1),
    resultJson: z.string().nullable(),
    status: toolInvocationStatusSchema,
    errorMessage: z.string().nullable(),
    generationTaskId: z.string().nullable(),
    startedAt: z.string().nullable(),
    completedAt: z.string().nullable(),
    createdAt: z.string().min(1),
  })
  .passthrough();

// ──────────────────────────────────────────────────────────────────
// Plan-and-Execute schemas
// ──────────────────────────────────────────────────────────────────

const planStepSchema = z
  .object({
    index: z.number().int().nonnegative(),
    description: z.string().min(1).max(500),
    status: planStepStatusSchema,
    kind: planStepKindSchema.default("task"),
  })
  .strict();

const planRecordSchema = z
  .object({
    id: z.uuid("计划标识无效。"),
    conversationId: z.uuid("会话标识无效。"),
    goal: z.string().min(1).max(2000),
    steps: z.array(planStepSchema),
    status: planStepStatusSchema,
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict();

// ──────────────────────────────────────────────────────────────────
// Event schemas (Tauri event payloads)
// ──────────────────────────────────────────────────────────────────

const messageAppendedEventSchema = z.object({
  kind: z.literal("messageAppended"),
  conversationId: z.string().min(1),
  message: messageRecordSchema,
});

const toolInvocationUpdatedEventSchema = z.object({
  kind: z.literal("toolInvocationUpdated"),
  conversationId: z.string().min(1),
  invocation: toolInvocationRecordSchema,
});

const doneEventSchema = z.object({
  kind: z.literal("done"),
  conversationId: z.string().min(1),
  finishReason: z.string().min(1),
});

const failedEventSchema = z.object({
  kind: z.literal("failed"),
  conversationId: z.string().min(1),
  error: z.string().min(1),
});

const planCreatedEventSchema = z.object({
  kind: z.literal("planCreated"),
  conversationId: z.string().min(1),
  plan: planRecordSchema,
});

const planUpdatedEventSchema = z.object({
  kind: z.literal("planUpdated"),
  conversationId: z.string().min(1),
  plan: planRecordSchema,
});

const planStepChangedEventSchema = z.object({
  kind: z.literal("planStepChanged"),
  conversationId: z.string().min(1),
  planId: z.string().min(1),
  stepIndex: z.number().int().nonnegative(),
  status: planStepStatusSchema,
});

const userQuestionAskedEventSchema = z.object({
  kind: z.literal("userQuestionAsked"),
  conversationId: z.string().min(1),
  toolCallId: z.string(),
  question: z.string().min(1),
});

const memoryRecallEntrySchema = z
  .object({
    sourceType: z.string(),
    content: z.string(),
    score: z.number(),
  })
  .strict();

const memoryRecalledEventSchema = z.object({
  kind: z.literal("memoryRecalled"),
  conversationId: z.string().min(1),
  memories: z.array(memoryRecallEntrySchema),
});

const streamChunkEventSchema = z.object({
  kind: z.literal("streamChunk"),
  conversationId: z.string().min(1),
  chunk: z.string(),
  chunkIndex: z.number().int(),
});

const streamDoneEventSchema = z.object({
  kind: z.literal("streamDone"),
  conversationId: z.string().min(1),
  fullContent: z.string(),
  totalChunks: z.number().int(),
});

const directorFallbackEventSchema = z.object({
  kind: z.literal("directorFallback"),
  conversationId: z.string().min(1),
  reason: z.string(),
});

const agentEventSchema = z.discriminatedUnion("kind", [
  messageAppendedEventSchema,
  toolInvocationUpdatedEventSchema,
  doneEventSchema,
  failedEventSchema,
  planCreatedEventSchema,
  planUpdatedEventSchema,
  planStepChangedEventSchema,
  userQuestionAskedEventSchema,
  memoryRecalledEventSchema,
  streamChunkEventSchema,
  streamDoneEventSchema,
  directorFallbackEventSchema,
]);

// ──────────────────────────────────────────────────────────────────
// IPC request schemas
// ──────────────────────────────────────────────────────────────────

const createConversationRequestSchema = z
  .object({
    title: z.string().trim().min(1, "会话标题不能为空。").max(200, "会话标题不能超过 200 个字符。"),
    credentialId: z.uuid("凭据标识无效。"),
    systemPrompt: z.string().max(8000, "系统提示不能超过 8000 个字符。").optional(),
  })
  .strict();

const listConversationsRequestSchema = z.object({}).strict();

const getConversationRequestSchema = z
  .object({
    id: z.uuid("会话标识无效。"),
  })
  .strict();

const deleteConversationRequestSchema = z
  .object({
    id: z.uuid("会话标识无效。"),
  })
  .strict();

const renameConversationRequestSchema = z
  .object({
    id: z.uuid("会话标识无效。"),
    title: z.string().trim().min(1, "会话标题不能为空。").max(200, "会话标题不能超过 200 个字符。"),
  })
  .strict();

const listMessagesRequestSchema = z
  .object({
    conversationId: z.uuid("会话标识无效。"),
  })
  .strict();

const attachmentSchema = z
  .object({
    name: z.string().min(1),
    mimeType: z.string().min(1),
    dataUrl: z.string().min(1),
  })
  .passthrough();

const sendMessageRequestSchema = z
  .object({
    conversationId: z.uuid("会话标识无效。"),
    content: z.string().trim().min(1, "消息内容不能为空。"),
    attachments: z.array(attachmentSchema).optional(),
  })
  .passthrough();

const sendMessageResultSchema = z
  .object({
    conversationId: z.string().min(1),
    finalMessage: messageRecordSchema.nullable(),
    invocations: z.array(toolInvocationRecordSchema),
    finishReason: z.string().min(1),
  })
  .passthrough();

const conversationListSchema = z.array(conversationRecordSchema);
const messageListSchema = z.array(messageRecordSchema);
const invocationListSchema = z.array(toolInvocationRecordSchema);
const conversationOrNullSchema = conversationRecordSchema.nullable();

// ──────────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────────

export type ConversationStatus = z.infer<typeof conversationStatusSchema>;
export type ExecutionMode = z.infer<typeof executionModeSchema>;
export type PlanStepStatus = z.infer<typeof planStepStatusSchema>;
export type PlanStep = z.infer<typeof planStepSchema>;
export type PlanRecord = z.infer<typeof planRecordSchema>;
export type MemoryRecallEntry = z.infer<typeof memoryRecallEntrySchema>;
export type MessageRole = z.infer<typeof messageRoleSchema>;
export type ToolInvocationStatus = z.infer<typeof toolInvocationStatusSchema>;
export type ToolCall = z.infer<typeof toolCallSchema>;
export type ConversationRecord = z.infer<typeof conversationRecordSchema>;
export type MessageRecord = z.infer<typeof messageRecordSchema>;
export type ToolInvocationRecord = z.infer<typeof toolInvocationRecordSchema>;
export type AgentEvent = z.infer<typeof agentEventSchema>;
export type SendMessageResult = z.infer<typeof sendMessageResultSchema>;
export type CreateConversationInput = z.input<typeof createConversationRequestSchema>;

export { NativeCommandError as AgentCommandError };
export const AGENT_EVENT_CHANNEL = "agent://event";

// ──────────────────────────────────────────────────────────────────
// IPC functions
// ──────────────────────────────────────────────────────────────────

/**
 * 创建一个新的 Agent 会话。workspaceId 由后端从当前工作空间注入。
 */
export async function createConversation(
  input: CreateConversationInput,
): Promise<ConversationRecord> {
  const request = parseRequest(createConversationRequestSchema, { ...input }, "会话信息无效。");
  return invokeNative("agent_v1_create_conversation", conversationRecordSchema, { request });
}

/**
 * 列出当前工作空间下所有 Agent 会话。
 */
export async function listConversations(): Promise<ConversationRecord[]> {
  const request = parseRequest(listConversationsRequestSchema, {}, "会话查询无效。");
  return invokeNative("agent_v1_list_conversations", conversationListSchema, { request });
}

/**
 * 按 id 获取单个会话。不存在时返回 null。
 */
export async function getConversation(id: string): Promise<ConversationRecord | null> {
  const request = parseRequest(getConversationRequestSchema, { id }, "会话标识无效。");
  return invokeNative("agent_v1_get_conversation", conversationOrNullSchema, { request });
}

/**
 * 删除一个会话（级联删除消息与工具调用记录）。
 */
export async function deleteConversation(id: string): Promise<void> {
  const request = parseRequest(deleteConversationRequestSchema, { id }, "会话标识无效。");
  await invokeNative("agent_v1_delete_conversation", z.union([z.void(), z.null()]), { request });
}

/**
 * 重命名会话标题，返回更新后的会话记录。
 */
export async function renameConversation(id: string, title: string): Promise<ConversationRecord> {
  const request = parseRequest(renameConversationRequestSchema, { id, title }, "会话信息无效。");
  return invokeNative("agent_v1_rename_conversation", conversationRecordSchema, { request });
}

/**
 * 列出会话的所有消息（按创建时间升序）。
 */
export async function listMessages(conversationId: string): Promise<MessageRecord[]> {
  const request = parseRequest(listMessagesRequestSchema, { conversationId }, "会话标识无效。");
  return invokeNative("agent_v1_list_messages", messageListSchema, { request });
}

/**
 * 列出会话的所有工具调用记录。
 */
export async function listInvocations(conversationId: string): Promise<ToolInvocationRecord[]> {
  const request = parseRequest(listMessagesRequestSchema, { conversationId }, "会话标识无效。");
  return invokeNative("agent_v1_list_invocations", invocationListSchema, { request });
}

/**
 * 发送用户消息并触发 ReAct 工具循环。
 * 后端会通过 Tauri 事件（AGENT_EVENT_CHANNEL）实时推送：
 *  - messageAppended：写入新消息（user/assistant/tool）
 *  - toolInvocationUpdated：工具调用状态变更
 *  - done：循环正常结束
 *  - failed：循环异常终止
 * 返回最终 assistant 消息与全部工具调用记录。
 */
export type AttachmentInput = z.infer<typeof attachmentSchema>;

export async function sendMessage(
  conversationId: string,
  content: string,
  attachments?: AttachmentInput[],
): Promise<SendMessageResult> {
  const request = parseRequest(
    sendMessageRequestSchema,
    { conversationId, content, attachments },
    "消息内容无效。",
  );
  return invokeNative("agent_v1_send_message", sendMessageResultSchema, { request });
}

/**
 * 设置生成输出目录。传入路径则保存图片到该目录，传 null 则恢复默认。
 */
export async function setOutputDirectory(path: string | null): Promise<void> {
  await invokeNative("agent_v1_set_output_directory", z.union([z.void(), z.null()]), {
    request: { path },
  });
}

/**
 * 解析 Tauri 事件 payload。失败时返回 null，便于调用方静默丢弃脏数据。
 */
export function parseAgentEvent(payload: unknown): AgentEvent | null {
  const result = agentEventSchema.safeParse(payload);
  return result.success ? result.data : null;
}

// ── Semantic Pipeline — 图片语义分析 ──

const semanticTagSchema = z.object({
  name: z.string(),
  confidence: z.number(),
  source: z.string(),
});

const semanticEntitySchema = z.object({
  entityType: z.string(),
  name: z.string(),
  confidence: z.number(),
  bbox: z
    .object({ x: z.number(), y: z.number(), width: z.number(), height: z.number() })
    .nullable()
    .optional(),
});

const artifactSemanticProfileSchema = z.object({
  artifactId: z.string(),
  caption: z.string().nullable(),
  ocrText: z.string().nullable(),
  tags: z.array(semanticTagSchema),
  entities: z.array(semanticEntitySchema),
  embeddingId: z.string().nullable(),
  analyzer: z.string(),
  analyzerVersion: z.string(),
  analyzedAt: z.string(),
});

const analysisResultSchema = z.object({
  profile: artifactSemanticProfileSchema,
  adapterId: z.string(),
  elapsedMs: z.number(),
});

const retrievalResultSchema = z.object({
  profile: artifactSemanticProfileSchema,
  score: z.number(),
  matchReason: z.string(),
});

export type ArtifactSemanticProfile = z.infer<typeof artifactSemanticProfileSchema>;
export type AnalysisResult = z.infer<typeof analysisResultSchema>;
export type RetrievalResult = z.infer<typeof retrievalResultSchema>;

/**
 * 分析单个资源图片，生成语义画像。
 */
export async function analyzeAsset(
  assetId: string,
  imageDataUrl: string,
  preferredAdapter?: string,
): Promise<AnalysisResult> {
  return invokeNative("agent_v1_analyze_asset", analysisResultSchema, {
    request: { assetId, imageDataUrl, preferredAdapter },
  });
}

/**
 * 批量分析资源图片。
 */
export async function analyzeAssetsBatch(
  items: [string, string][],
  preferredAdapter?: string,
): Promise<AnalysisResult[]> {
  return invokeNative("agent_v1_analyze_assets_batch", z.array(analysisResultSchema), {
    request: { items, preferredAdapter },
  });
}

/**
 * 按语义搜索资源。
 */
export async function searchAssetsSemantic(
  query: string,
  limit?: number,
): Promise<RetrievalResult[]> {
  return invokeNative("agent_v1_search_assets_semantic", z.array(retrievalResultSchema), {
    request: { query, limit },
  });
}

function parseRequest<T>(schema: z.ZodType<T>, value: unknown, fallback: string): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    throw firstValidationError(result, fallback);
  }
  return result.data;
}
