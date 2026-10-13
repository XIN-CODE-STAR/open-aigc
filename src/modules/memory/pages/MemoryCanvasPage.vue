<script setup lang="ts">
/**
 * MemoryCanvasPage：「工作记忆画布」独立工作区。
 *
 * 画布本身是「每会话一份」（后端按 `conv-<conversationId>` 命名查找），
 * 因此本页按以下顺序解析目标画布：
 *   1) URL 上的 `?conversation=<id>`（从任务/侧栏带过来时）；
 *   2) 否则取最近更新的一次会话；
 *   3) 都没有时传 null —— 画布为空，仍可手动添加节点。
 */
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";

import { listConversations, type ConversationRecord } from "../../../bridge/agent";
import MemoryCanvasPanel from "../components/MemoryCanvasPanel.vue";

const router = useRouter();

const conversations = ref<ConversationRecord[]>([]);
const ready = ref(false);

const conversationId = computed<string | null>(() => {
  const fromQuery = router.currentRoute.value.query.conversation;
  if (typeof fromQuery === "string" && fromQuery.length > 0) return fromQuery;
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
