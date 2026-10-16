import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  AGENT_EVENT_CHANNEL,
  AgentCommandError,
  clearConversation as clearConversationIpc,
  createConversation as createConversationIpc,
  deleteConversation as deleteConversationIpc,
  getConversation as getConversationIpc,
  listConversations as listConversationsIpc,
  listInvocations as listInvocationsIpc,
  listMessages as listMessagesIpc,
  parseAgentEvent,
  sendMessage as sendMessageIpc,
  type AgentEvent,
  type AttachmentInput,
  type ConversationRecord,
  type CreateConversationInput,
  type MemoryRecallEntry,
  type MessageRecord,
  type PlanRecord,
  type ToolInvocationRecord,
} from "../../../bridge/agent";
import { playErrorSound, playSuccessSound } from "../../../shared/composables/useNotificationSound";
import { useConversationWorkspaces } from "../../../app/composables/useConversationWorkspaces";
import { useProjectDirectory } from "../../../app/stores/projectDirectory";

export type AgentConversationPhase = "idle" | "loading" | "ready" | "error";

/**
 * Agent 循环当前阶段的细化状态，用于驱动 UI 中的差异化占位动画。
 *  - `idle`：未在发送
 *  - `thinking`：已调用 LLM，等待首个 assistant 消息或工具调用事件
 *  - `executing`：至少有一个工具调用处于 pending/running
 *  - `done`：循环正常结束（短暂保留用于过渡动画）
 *  - `failed`：循环异常终止
 */
export type AgentLoopPhase = "idle" | "thinking" | "executing" | "done" | "failed";

const DEFAULT_ERROR_MESSAGE = "Agent 会话操作失败，请稍后重试。";

/**
 * Agent 消息分组：
 *  - 用户消息单独成一组
 *  - 助手消息 + 紧随其后的 tool 消息合并为一组（tool 消息归属到 assistant 的 tool_call 下）
 *  这样视觉上呈现"用户 → 助手思考/工具调用/工具结果 → 用户"的清晰流程。
 */
export interface AgentMessageGroup {
  id: string;
  userMessage?: MessageRecord;
  assistantMessage?: MessageRecord;
  /** 该 assistant 消息关联的工具调用记录（按 messageId 匹配） */
  invocations: ToolInvocationRecord[];
  /** 该 assistant 消息的 tool_calls 对应的 tool 结果消息（按 tool_call_id 匹配） */
  toolResults: MessageRecord[];
}

/** 会话列表发生变化时广播的事件名（侧栏 / 任务页监听它重新拉取）。 */
const CONVERSATIONS_CHANGED_EVENT = "aigc-agent-conversations-updated";

/** 广播「会话列表已变化」。 */
function notifyConversationsChanged(): void {
  window.dispatchEvent(new CustomEvent(CONVERSATIONS_CHANGED_EVENT));
}

/**
 * 管理 Agent 会话列表、当前会话的消息与工具调用记录，
 * 并通过 Tauri 事件实时同步后端 ReAct 循环的进度。
 *
 * 必须在 Vue 组件的 setup 阶段调用以正确绑定 listen/unlisten 生命周期。
 */
export function useAgentConversation() {
  const phase = ref<AgentConversationPhase>("idle");
  const conversations = ref<ConversationRecord[]>([]);
  const currentConversation = ref<ConversationRecord | null>(null);
  const messages = ref<MessageRecord[]>([]);
  const invocations = ref<ToolInvocationRecord[]>([]);
  /**
   * 正在发送的会话 id（null 表示空闲）。
   *
   * 刻意用「会话 id」而不是全局布尔：否则在 A 会话发送中切到 B，
   * B 会被误判为「正在处理」，A 的旧请求返回时还会把状态写进 B。
   */
  const sendingConversationId = ref<string | null>(null);
  /** 当前会话是否正在发送（按会话归属判定，切换会话后不会串味）。 */
  const isSending = computed(
    () =>
      sendingConversationId.value !== null &&
      sendingConversationId.value === currentConversation.value?.id,
  );
  const errorMessage = ref<string | undefined>(undefined);
  const conversationWorkspaces = useConversationWorkspaces();
  const projectDir = useProjectDirectory();

  /** 当前执行计划（Plan-and-Execute 模式）。 */
  const currentPlan = ref<PlanRecord | null>(null);
  /**
   * 当前执行计划所属轮次的用户消息 id。
   * 用于把计划面板渲染到该轮用户消息之下、助手回复之上；
   * 为空时前端退化为"渲染在时间线末尾"的兜底布局。
   */
  const planRoundMessageId = ref<string | null>(null);
  /** Agent 向学生提出的问题（等待回答）。 */
  const pendingQuestion = ref<{ toolCallId: string; question: string } | null>(null);
  /** 规划阶段检索到的相关记忆。 */
  const recalledMemories = ref<MemoryRecallEntry[]>([]);
  /** 流式输出内容（实时累积）。 */
  const streamingContent = ref<string>("");
  /** 流式输出是否进行中。 */
  const isStreaming = ref(false);

  /** 工具调用详情展开状态：记录 invocation id */
  const expandedInvocations = ref<Set<string>>(new Set());

  /** 最近一次发送的内容，用于失败后重试。 */
  let lastSentContent: string | null = null;
  /** 最近一次发送所在的会话 id，重试时校验会话未被切换。 */
  let lastSentConversationId: string | null = null;

  /**
   * 生成任务 id → 发起它的会话 id。
   *
   * 异步生成（图片/视频）完成事件 `generation://completed` 的载荷**不含会话信息**，
   * 若直接写入「当前正在看的会话」，就会出现「新建/切换对话后，晚到的生成结果
   * 落进另一段对话」。这里在收到工具调用更新（其 `generationTaskId` 在提交时即写入）
   * 时登记归属，完成事件据此精确回填到原会话。
   */
  const generationTaskOwners = new Map<string, string>();

  let unlisten: UnlistenFn | null = null;
  let listRequest = 0;
  /** 是否已收到过第一条事件（仅用于一次性诊断日志）。 */
  let firstEventLogged = false;

  /** 诊断日志：WebView2 console 不会进 dev 日志，改走后端 stderr 通道。 */
  function debugLog(message: string): void {
    void invoke("agent_v1_debug_log", { request: { message } }).catch(() => undefined);
  }

  const isBusy = computed(() => phase.value === "loading");

  /**
   * 细化的循环阶段：根据 isSending + 是否有 running 工具调用推断。
   *  - thinking：发送中但还没有任何工具进入 pending/running
   *  - executing：发送中且至少一个工具调用处于 pending/running
   */
  const loopPhase = computed<AgentLoopPhase>(() => {
    if (!isSending.value) {
      return errorMessage.value ? "failed" : "idle";
    }
    const hasActiveTool = invocations.value.some(
      (i) => i.status === "pending" || i.status === "running",
    );
    return hasActiveTool ? "executing" : "thinking";
  });

  /**
   * 把扁平的消息列表分组：user 单独成组，assistant + 紧随其后的 tool 合并成一组。
   * tool 消息按 tool_call_id 关联到前一个 assistant 的 tool_call。
   */
  const groupedMessages = computed<AgentMessageGroup[]>(() => {
    const groups: AgentMessageGroup[] = [];
    const invocationsByMessage = new Map<string, ToolInvocationRecord[]>();
    for (const inv of invocations.value) {
      const list = invocationsByMessage.get(inv.messageId);
      if (list) list.push(inv);
      else invocationsByMessage.set(inv.messageId, [inv]);
    }

    for (const msg of messages.value) {
      if (msg.role === "user") {
        groups.push({
          id: msg.id,
          userMessage: msg,
          invocations: [],
          toolResults: [],
        });
      } else if (msg.role === "assistant") {
        groups.push({
          id: msg.id,
          assistantMessage: msg,
          invocations: invocationsByMessage.get(msg.id) ?? [],
          toolResults: [],
        });
      } else if (msg.role === "tool") {
        // tool 消息合并到最近的 assistant 组（从后向前查找）
        let target: AgentMessageGroup | undefined;
        for (let i = groups.length - 1; i >= 0; i--) {
          if (groups[i].assistantMessage !== undefined) {
            target = groups[i];
            break;
          }
        }
        if (target) {
          target.toolResults.push(msg);
        } else {
          // 异常情况：tool 消息没有前导 assistant，独立成组以保留信息
          groups.push({
            id: msg.id,
            assistantMessage: msg,
            invocations: [],
            toolResults: [],
          });
        }
      }
      // system 消息不展示
    }
    return groups;
  });

  /**
   * 当前会话累计 token 用量（assistant 消息的 promptTokens/completionTokens 求和）。
   */
  const tokenUsage = computed(() => {
    let prompt = 0;
    let completion = 0;
    for (const msg of messages.value) {
      if (msg.role === "assistant") {
        prompt += msg.promptTokens ?? 0;
        completion += msg.completionTokens ?? 0;
      }
    }
    return { prompt, completion, total: prompt + completion };
  });

  /** 在当前消息列表中倒序查找最后一条用户消息的 id（即触发本轮计划的用户消息）。 */
  function findLastUserMessageId(): string | null {
    for (let i = messages.value.length - 1; i >= 0; i--) {
      const msg = messages.value[i];
      if (msg && msg.role === "user") {
        return msg.id;
      }
    }
    return null;
  }

  /**
   * 登记生成任务的归属会话。
   *
   * 注意：这里**先于** `handleEvent` 的会话过滤执行——即使当前正在看别的会话，
   * 也要把归属记下来，否则切回来时无法判断某个完成事件是否属于本会话。
   */
  function rememberGenerationOwner(event: AgentEvent): void {
    if (event.kind === "toolInvocationUpdated" && event.invocation.generationTaskId) {
      generationTaskOwners.set(event.invocation.generationTaskId, event.conversationId);
    }
  }

  function handleEvent(event: AgentEvent): void {
    if (!currentConversation.value) return;
    if (event.conversationId !== currentConversation.value.id) return;

    switch (event.kind) {
      case "messageAppended": {
        const incoming = event.message;
        const exists = messages.value.some((m) => m.id === incoming.id);
        if (!exists) {
          messages.value = [...messages.value, incoming];
        }
        // assistant 消息落库意味着对应流式片段已结束，清空流式状态，
        // 避免气泡残留或与已落库文本重复展示。
        if (incoming.role === "assistant") {
          streamingContent.value = "";
          isStreaming.value = false;
        }
        break;
      }
      case "toolInvocationUpdated": {
        const incoming = event.invocation;
        const index = invocations.value.findIndex((i) => i.id === incoming.id);
        if (index === -1) {
          invocations.value = [...invocations.value, incoming];
        } else {
          const next = invocations.value.slice();
          next[index] = incoming;
          invocations.value = next;
        }
        break;
      }
      case "done": {
        pendingQuestion.value = null;
        // 兜底清理：若 streamDone/messageAppended 事件丢失，循环结束后气泡不得残留。
        streamingContent.value = "";
        isStreaming.value = false;
        playSuccessSound();
        break;
      }
      case "failed": {
        errorMessage.value = event.error;
        pendingQuestion.value = null;
        streamingContent.value = "";
        isStreaming.value = false;
        playErrorSound();
        break;
      }
      case "planCreated": {
        currentPlan.value = event.plan;
        planRoundMessageId.value = findLastUserMessageId();
        break;
      }
      case "planUpdated": {
        currentPlan.value = event.plan;
        if (!planRoundMessageId.value) {
          planRoundMessageId.value = findLastUserMessageId();
        }
        break;
      }
      case "planStepChanged": {
        if (currentPlan.value && currentPlan.value.id === event.planId) {
          const steps = currentPlan.value.steps.map((s) =>
            s.index === event.stepIndex ? { ...s, status: event.status } : s,
          );
          currentPlan.value = { ...currentPlan.value, steps };
        }
        break;
      }
      case "userQuestionAsked": {
        pendingQuestion.value = {
          toolCallId: event.toolCallId,
          question: event.question,
        };
        break;
      }
      case "memoryRecalled": {
        recalledMemories.value = event.memories;
        break;
      }
      case "streamChunk": {
        // 后端每个执行步骤的片段计数器从 0 重新开始：
        // chunkIndex 归零即新流式片段开始，清空上一步骤的残留内容，
        // 避免多步骤计划把各步文本串联到同一个气泡里。
        if (event.chunkIndex === 0) {
          streamingContent.value = "";
        }
        streamingContent.value += event.chunk;
        isStreaming.value = true;
        break;
      }
      case "streamDone": {
        streamingContent.value = event.fullContent;
        isStreaming.value = false;
        break;
      }
    }
  }

  const genUnlistenFns: UnlistenFn[] = [];

  async function startListening(): Promise<void> {
    if (unlisten) return;
    try {
      unlisten = await listen<unknown>(AGENT_EVENT_CHANNEL, (event) => {
        if (!firstEventLogged) {
          firstEventLogged = true;
          debugLog("[AgentEvent] first event received");
        }
        const parsed = parseAgentEvent(event.payload);
        if (parsed) {
          rememberGenerationOwner(parsed);
          handleEvent(parsed);
        } else {
          // 诊断日志：事件契约不匹配导致静默丢弃，输出 payload 预览便于排查。
          let preview = "";
          try {
            preview = JSON.stringify(event.payload)?.slice(0, 300) ?? "";
          } catch {
            preview = "(unserializable)";
          }
          debugLog(`[AgentEvent] unrecognized payload dropped: ${preview}`);
          console.warn("[AgentEvent] unrecognized payload dropped", event.payload);
        }
      });
      debugLog("[AgentEvent] subscribed OK");
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      debugLog(`[AgentEvent] subscribe FAILED: ${detail}`);
      console.error("[AgentEvent] subscribe failed:", error);
      errorMessage.value = "实时事件订阅失败，请重启应用以恢复 Agent 实时进度。";
    }

    // 监听生成完成事件：把生成结果图片注入聊天流
    try {
      const genUnlisten = await listen<{
        attemptId: string;
        taskId: string;
        resultUrl?: string;
      }>("generation://completed", (event) => {
        const { resultUrl, taskId } = event.payload;
        const active = currentConversation.value;
        if (!resultUrl || !active) return;
        // 只回填到发起该生成的会话：晚到的结果绝不写进另一段对话。
        const owner = generationTaskOwners.get(taskId);
        if (owner !== active.id) {
          debugLog(
            `[GenerationEvent] drop result task=${taskId} owner=${owner ?? "unknown"} current=${active.id}`,
          );
          return;
        }
        // 把生成结果作为 assistant 消息注入聊天
        const resultMessage: MessageRecord = {
          id: `gen-result-${event.payload.attemptId}`,
          conversationId: active.id,
          role: "assistant",
          content: `✅ 图片已生成！

![生成结果](${resultUrl})`,
          toolCalls: [],
          toolCallId: null,
          remoteModel: null,
          finishReason: "stop",
          parentMessageId: null,
          revision: 0,
          createdAt: new Date().toISOString(),
          promptTokens: null,
          completionTokens: null,
        };
        const exists = messages.value.some((m) => m.id === resultMessage.id);
        if (!exists) {
          messages.value = [...messages.value, resultMessage];
        }
      });
      genUnlistenFns.push(genUnlisten);
      debugLog("[GenerationEvent] generation://completed subscribed");
    } catch (error) {
      debugLog(`[GenerationEvent] subscribe failed: ${error}`);
    }
  }

  function stopListening(): void {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    for (const fn of genUnlistenFns) {
      fn();
    }
    genUnlistenFns.length = 0;
  }

  function clearError(): void {
    errorMessage.value = undefined;
  }

  function clearPlan(): void {
    currentPlan.value = null;
    planRoundMessageId.value = null;
  }

  function clearPendingQuestion(): void {
    pendingQuestion.value = null;
  }

  function clearRecalledMemories(): void {
    recalledMemories.value = [];
  }

  function setError(error: unknown): void {
    if (error instanceof AgentCommandError) {
      errorMessage.value = error.message;
    } else if (error instanceof Error && error.message) {
      errorMessage.value = error.message;
    } else {
      errorMessage.value = DEFAULT_ERROR_MESSAGE;
    }
  }

  function toggleInvocation(id: string): void {
    const next = new Set(expandedInvocations.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedInvocations.value = next;
  }

  function isInvocationExpanded(id: string): boolean {
    return expandedInvocations.value.has(id);
  }

  async function loadConversations(): Promise<void> {
    const request = ++listRequest;
    clearError();
    phase.value = "loading";
    try {
      const records = await listConversationsIpc();
      if (request !== listRequest) return;
      conversations.value = records;
      phase.value = "ready";
    } catch (error) {
      if (request !== listRequest) return;
      conversations.value = [];
      setError(error);
    }
  }

  async function selectConversation(conversation: ConversationRecord): Promise<void> {
    currentConversation.value = conversation;
    // 执行计划、流式文本等都是实时态，不跨会话保留，避免上一会话的残留串进新会话。
    resetConversationView();
    clearError();
    try {
      const [msgRecords, invRecords] = await Promise.all([
        listMessagesIpc(conversation.id),
        listInvocationsIpc(conversation.id),
      ]);
      if (currentConversation.value?.id !== conversation.id) return;
      messages.value = msgRecords;
      invocations.value = invRecords;
    } catch (error) {
      setError(error);
    }
  }

  async function refreshCurrent(): Promise<void> {
    if (!currentConversation.value) return;
    const id = currentConversation.value.id;
    try {
      const fresh = await getConversationIpc(id);
      if (fresh && currentConversation.value?.id === id) {
        currentConversation.value = fresh;
      }
    } catch (error) {
      setError(error);
    }
  }

  async function createConversation(
    input: CreateConversationInput,
  ): Promise<ConversationRecord | null> {
    clearError();
    try {
      const record = await createConversationIpc(input);
      // 明确区分「打开了自定义目录」与「仍用默认区」：projectDir.workspaceName
      // 在默认区下会返回 "OPEN AIGC"，而那也是合法的文件夹名，不能靠它判断。
      conversationWorkspaces.record(
        record.id,
        projectDir.isCustom ? projectDir.workspaceName : null,
      );
      conversations.value = [record, ...conversations.value];
      await selectConversation(record);
      return record;
    } catch (error) {
      setError(error);
      return null;
    }
  }

  async function deleteConversation(id: string): Promise<boolean> {
    clearError();
    try {
      await deleteConversationIpc(id);
      conversationWorkspaces.remove(id);
      conversations.value = conversations.value.filter((c) => c.id !== id);
      if (currentConversation.value?.id === id) {
        currentConversation.value = null;
        messages.value = [];
        invocations.value = [];
        expandedInvocations.value = new Set();
        currentPlan.value = null;
        planRoundMessageId.value = null;
        lastSentContent = null;
        lastSentConversationId = null;
      }
      // 通知侧栏/任务页重新拉取：它们共享同一份列表，但只有收到事件才会
      // 去服务端刷新（否则删掉的会话要等下一次创建才消失）。
      notifyConversationsChanged();
      return true;
    } catch (error) {
      setError(error);
      return false;
    }
  }

  /**
   * 重置「当前会话视图」的全部派生状态。
   *
   * 必须包含流式文本与循环阶段：否则 A 会话正在流式输出时切到 B，
   * 那段气泡会残留在 B 里（另一条「串到新对话」的路径）。
   */
  function resetConversationView(): void {
    messages.value = [];
    invocations.value = [];
    expandedInvocations.value = new Set();
    currentPlan.value = null;
    planRoundMessageId.value = null;
    pendingQuestion.value = null;
    recalledMemories.value = [];
    streamingContent.value = "";
    isStreaming.value = false;
    lastSentContent = null;
    lastSentConversationId = null;
  }

  /**
   * 清空当前会话，回到欢迎页。
   *
   * 只清除本地视图状态，**不创建**任何会话记录——这正是「清空对话」与
   * 「新建对话」的区别：清空绝不能走新建动作，否则历史里会凭空多出一条空对话。
   */
  function clearCurrentConversation(): void {
    clearError();
    currentConversation.value = null;
    resetConversationView();
  }

  /**
   * 清空当前会话的消息记录（**保留会话本身**）。
   *
   * 成功后本地视图一并重置，但 `currentConversation` 保持不变——用户留在同一个
   * 会话里继续聊，而不是被丢进一条新会话。这正是「清空对话」与「新建对话」的分野：
   * 清空不得走新建动作，否则历史里会凭空多出一条空会话。
   */
  async function clearConversation(): Promise<boolean> {
    const conversation = currentConversation.value;
    if (!conversation) return false;
    clearError();
    try {
      await clearConversationIpc(conversation.id);
      resetConversationView();
      // 消息没了但会话还在：刷新一次让任务页回放区/侧栏时间同步
      notifyConversationsChanged();
      return true;
    } catch (error) {
      setError(error);
      return false;
    }
  }

  async function sendMessage(
    content: string,
    attachments?: AttachmentInput[],
    systemPromptOverride?: string,
  ): Promise<void> {
    if (!currentConversation.value) {
      errorMessage.value = "请先选择或创建一个 Agent 会话。";
      return;
    }
    if (isSending.value) {
      errorMessage.value = "上一条消息仍在处理中，请等待 Agent 回复完成后再发送。";
      return;
    }
    clearError();
    const conversationId = currentConversation.value.id;
    sendingConversationId.value = conversationId;
    streamingContent.value = "";
    isStreaming.value = false;
    // 新一轮对话开始时清空上一轮的执行计划，避免旧计划面板残留在新消息下方；
    // 本轮若触发 Plan-and-Execute，planCreated 事件会重新填充。
    currentPlan.value = null;
    planRoundMessageId.value = null;
    lastSentContent = content;
    lastSentConversationId = conversationId;
    try {
      await sendMessageIpc(
        conversationId,
        content,
        attachments && attachments.length > 0 ? attachments : undefined,
        systemPromptOverride,
      );
      // invoke 完成后主动重新加载消息与工具调用，不完全依赖实时事件。
      const [msgRecords, invRecords] = await Promise.all([
        listMessagesIpc(conversationId),
        listInvocationsIpc(conversationId),
      ]);
      if (currentConversation.value?.id === conversationId) {
        messages.value = msgRecords;
        invocations.value = invRecords;
      }
      await refreshCurrent();
      // 后端 append_message 会 touch updated_at，但侧栏/任务页手里是缓存副本，
      // 不通知的话「刚聊过的会话」在列表里仍显示旧时间，直到发生创建/删除。
      notifyConversationsChanged();
    } catch (error) {
      setError(error);
    } finally {
      // 只有仍是本会话在发送时才清除，避免把其它会话的在途状态一并清掉。
      if (sendingConversationId.value === conversationId) {
        sendingConversationId.value = null;
      }
    }
  }

  /**
   * 重试上一次发送。仅当会话未切换且存在历史内容时有效。
   */
  async function retryLastSend(): Promise<void> {
    if (!lastSentContent) return;
    if (!lastSentConversationId) return;
    if (currentConversation.value?.id !== lastSentConversationId) {
      errorMessage.value = "会话已切换，无法重试上一次发送。";
      return;
    }
    const content = lastSentContent;
    lastSentContent = null;
    lastSentConversationId = null;
    await sendMessage(content);
  }

  onMounted(() => {
    void startListening();
  });

  onUnmounted(() => {
    stopListening();
  });

  return {
    phase,
    conversations,
    currentConversation,
    messages,
    invocations,
    isSending,
    errorMessage,
    currentPlan,
    planRoundMessageId,
    pendingQuestion,
    recalledMemories,
    streamingContent,
    isStreaming,
    isBusy,
    loopPhase,
    groupedMessages,
    tokenUsage,
    loadConversations,
    selectConversation,
    refreshCurrent,
    createConversation,
    deleteConversation,
    clearCurrentConversation,
    clearConversation,
    sendMessage,
    retryLastSend,
    toggleInvocation,
    isInvocationExpanded,
    clearError,
    clearPlan,
    clearPendingQuestion,
    clearRecalledMemories,
  };
}
