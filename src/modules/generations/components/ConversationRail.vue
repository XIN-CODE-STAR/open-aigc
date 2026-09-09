<script setup lang="ts">
/**
 * ConversationRail：创意工坊中央消息流容器。
 *
 * 根据当前模式分发：
 * - Agent 模式：渲染 ExecutionPlanPanel + AgentStepTimeline + UserQuestionBubble
 * - 非 Agent 模式：渲染 CreationResultGrid
 */
import type { AgentMessageGroup, AgentLoopPhase } from "../composables/useAgentConversation";
import type { GenerationTaskRecord } from "../../../bridge/generations";
import type { MemoryRecallEntry, PlanRecord } from "../../../bridge/agent";
import { nextTick, onMounted, ref, watch } from "vue";
import AgentStepTimeline from "./AgentStepTimeline.vue";
import CreationResultGrid from "./CreationResultGrid.vue";
import MemoryRecallPanel from "./MemoryRecallPanel.vue";
import UserQuestionBubble from "./UserQuestionBubble.vue";

const props = defineProps<{
  agentGroups: AgentMessageGroup[];
  tasks: GenerationTaskRecord[];
  isAgentMode: boolean;
  agentWorking: boolean;
  agentLoopPhase: AgentLoopPhase;
  agentWorkingLabel: string;
  agentTokenUsage: { total: number; prompt: number; completion: number };
  /** 流式输出的实时累积文本。 */
  streamingContent: string;
  /** 流式输出是否进行中。 */
  isStreaming: boolean;
  isInvocationExpanded: (id: string) => boolean;
  recordBusyTaskId: string | null;
  recordError: string | null;
  toolLabel: (name: string) => string;
  prettyJson: (raw: string) => string;
  formatTime: (iso: string) => string;
  /** 当前执行计划（Plan-and-Execute 模式）。 */
  currentPlan: PlanRecord | null;
  /** 当前计划所属轮次的用户消息 id（用于把计划内联到该轮回复上方）。 */
  planRoundMessageId: string | null;
  /** Agent 提问（等待学生回答）。 */
  pendingQuestion: { toolCallId: string; question: string } | null;
  /** 规划阶段检索到的相关记忆。 */
  recalledMemories: MemoryRecallEntry[];
}>();

const emit = defineEmits<{
  "toggle-invocation": [id: string];
  "attach-result": [task: GenerationTaskRecord];
  "answer-question": [answer: string];
}>();

const scrollContainer = ref<HTMLElement | null>(null);

function scrollToBottom(): void {
  const el = scrollContainer.value;
  if (el) el.scrollTop = el.scrollHeight;
}

// 组件挂载时（进入对话）滚动到底部
onMounted(() => {
  nextTick(() => {
    scrollToBottom();
    setTimeout(scrollToBottom, 100);
  });
});

// groups 变化时（加载历史消息/新消息）滚动到底部
watch(
  () => props.agentGroups.length,
  () => {
    nextTick(() => {
      requestAnimationFrame(scrollToBottom);
      setTimeout(scrollToBottom, 80);
    });
  },
);

// 流式输出期间，内容增长时自动滚动到底部，保证最新片段可见。
watch(
  () => props.streamingContent,
  () => {
    if (!props.isStreaming) return;
    const el = scrollContainer.value;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  },
);

// 新计划生成时滚动到底部，保证计划面板（内联在当轮回复上方）可见。
// 仅按 plan id 触发（计划创建），步骤进度更新不强制滚动，避免打断用户回看历史。
let lastScrolledPlanId: string | null = null;
watch(
  () => props.currentPlan?.id ?? null,
  (planId) => {
    if (!planId || planId === lastScrolledPlanId) return;
    lastScrolledPlanId = planId;
    const el = scrollContainer.value;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  },
);

function onToggleInvocation(id: string): void {
  emit("toggle-invocation", id);
}

function onAttachResult(task: GenerationTaskRecord): void {
  emit("attach-result", task);
}

function onAnswerQuestion(answer: string): void {
  emit("answer-question", answer);
}
</script>

<template>
  <main ref="scrollContainer" class="grok-stream">
    <div class="grok-messages">
      <!-- 记忆检索结果 -->
      <MemoryRecallPanel v-if="recalledMemories.length > 0" :memories="recalledMemories" />

      <!-- Agent 消息流：Agent 模式或存在历史消息时都显示，切换模式不会丢失对话 -->
      <AgentStepTimeline
        v-if="isAgentMode || agentGroups.length > 0"
        :groups="agentGroups"
        :is-working="isAgentMode && agentWorking"
        :loop-phase="isAgentMode ? agentLoopPhase : 'idle'"
        :working-label="isAgentMode ? agentWorkingLabel : ''"
        :token-usage="agentTokenUsage"
        :streaming-content="isAgentMode ? streamingContent : ''"
        :is-streaming="isAgentMode && isStreaming"
        :is-invocation-expanded="isInvocationExpanded"
        :tool-label="toolLabel"
        :pretty-json="prettyJson"
        :format-time="formatTime"
        :current-plan="isAgentMode ? currentPlan : null"
        :plan-round-message-id="isAgentMode ? planRoundMessageId : null"
        @toggle-invocation="onToggleInvocation"
      />
      <CreationResultGrid
        v-else-if="tasks.length > 0"
        :tasks="tasks"
        :record-busy-task-id="recordBusyTaskId"
        :record-error="recordError"
        :format-time="formatTime"
        @attach-result="onAttachResult"
      />

      <!-- Agent 提问气泡 -->
      <UserQuestionBubble
        v-if="pendingQuestion"
        :question="pendingQuestion.question"
        @answer="onAnswerQuestion"
      />
    </div>
  </main>
</template>

<style scoped>
.grok-stream {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* 底部为悬浮 Composer 预留空间：--composer-reserve 由父级 GenerationsPage
     通过 ResizeObserver 实测 composer 高度后写入，保证滚动到底不遮文字。 */
  padding: 16px 24px calc(var(--composer-reserve, 220px) + 8px);
  scroll-behavior: smooth;
}

.grok-stream::-webkit-scrollbar {
  width: 6px;
}

.grok-stream::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--color-text) 15%, transparent);
  border-radius: 3px;
}
</style>
