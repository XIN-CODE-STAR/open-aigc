<script setup lang="ts">
/**
 * MemoryCanvasPage：「记忆画布」独立工作区。
 *
 * 画布本身是「每会话一份」（后端按 `conv-<conversationId>` 命名查找），
 * 因此目标画布按以下顺序解析：
 *   1) URL 上的 `?conversation=<id>`（从任务/侧栏带过来时）；
 *   2) 当前活跃会话 —— 用户在创意工坊里正在查看或运行的那个
 *      （跨页共享，见 stores/activeConversation）；
 *   3) 最近更新的一次会话；
 *   4) 都没有时传 null —— 画布为空，仍可手动添加节点。
 *
 * 第 2 级不能省、也不能换成「最近更新」：用户在多个会话之间来回切换时，
 * 按更新时间取会跳到别的会话的画布上。
 */
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";

import { listConversations, type ConversationRecord } from "../../../bridge/agent";
import MemoryCanvasPanel from "../components/MemoryCanvasPanel.vue";
import { useActiveConversationStore } from "../../../app/stores/activeConversation";

const router = useRouter();
const activeConversation = useActiveConversationStore();

const conversations = ref<ConversationRecord[]>([]);
const ready = ref(false);

const conversationId = computed<string | null>(() => {
  const fromQuery = router.currentRoute.value.query.conversation;
  if (typeof fromQuery === "string" && fromQuery.length > 0) return fromQuery;
  const active = activeConversation.id;
  if (active && conversations.value.some((item) => item.id === active)) return active;
  return conversations.value[0]?.id ?? null;
});

onMounted(async () => {
  try {
    conversations.value = await listConversations();
  } catch {
    // 读取失败按「无会话」处理：画布仍可打开并手动编辑
  }
  ready.value = true;
});

/** 独立页面里关闭 = 回到创意工坊（面板自身不显示关闭按钮，此处兜底）。 */
function onClose(): void {
  void router.push("/generations");
}
</script>

<template>
  <div class="canvas-page">
    <MemoryCanvasPanel
      :conversation-id="conversationId"
      :visible="ready"
      standalone
      @close="onClose"
    />
  </div>
</template>

<style scoped>
.canvas-page {
  position: relative;
  width: 100%;
  height: calc(100vh - var(--topbar-height) - (var(--page-padding) * 2));
  min-height: 520px;
}
</style>
