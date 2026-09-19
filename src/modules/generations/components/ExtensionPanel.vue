<script setup lang="ts">
/**
 * ExtensionPanel：统一扩展面板（工具 + 技能 + MCP）。
 *
 * - 工具 tab：Agent 内置工具列表（只读展示）
 * - 技能 tab：可开关的提示词技能，选中的技能注入 Agent 系统提示词
 * - MCP tab：预留标准 MCP 协议接入（Phase 2）
 */
import { ref } from "vue";
import { X, Blocks, Wrench, Sparkles, Globe, CircleCheck } from "@lucide/vue";
import { EXTENSION_SKILLS as SKILLS } from "../skills";

defineProps<{ visible: boolean }>();
const emit = defineEmits<{ close: [] }>();

const activeTab = ref<"tools" | "skills" | "mcp">("tools");

interface ToolItem {
  name: string;
  description: string;
}

const builtinTools: ToolItem[] = [
  { name: "image_generation", description: "生成图片：文生图 / 图生图（自动使用对话参考图）" },
  { name: "video_generation", description: "生成视频：文生视频 / 图生视频" },
  { name: "canvas_search", description: "检索工作记忆画布上的节点内容" },
  { name: "canvas_add_note", description: "在画布上创建便签节点（自动关联相关节点）" },
  { name: "canvas_connect", description: "在画布上两个节点间建立连线" },
  { name: "analyze_image", description: "使用视觉模型深度分析图片内容" },
  { name: "parse_script", description: "解析剧本文本为分镜结构" },
  { name: "search_similar_assets", description: "在资产库中搜索与描述相似的图片" },
  { name: "list_credentials", description: "列出可用的 AI 平台凭据" },
  { name: "current_time", description: "获取当前时间" },
  { name: "apply_script_plan", description: "将分镜计划应用到画布" },
  { name: "update_canvas_node_status", description: "更新画布节点状态" },
];

const selectedSkillIds = defineModel<string[]>({ default: () => [] });

function toggleSkill(id: string): void {
  const current = selectedSkillIds.value;
  if (current.includes(id)) {
    selectedSkillIds.value = current.filter((s) => s !== id);
  } else {
    selectedSkillIds.value = [...current, id];
  }
}

const mcpServers = ref([
  { name: "内置工具", status: "connected", count: builtinTools.length },
]);
</script>

<template>
  <Transition name="ext-slide">
    <aside v-if="visible" class="ext-panel">
      <div class="ext-header">
        <span class="ext-header__icon"><Blocks :size="16" /></span>
        <span class="ext-header__title">扩展</span>
        <button class="ext-close" @click="emit('close')"><X :size="14" /></button>
      </div>

      <!-- Tabs -->
      <div class="ext-tabs">
        <button
          :class="['ext-tab', activeTab === 'tools' && 'ext-tab--active']"
          @click="activeTab = 'tools'"
        >
          <Wrench :size="13" /> 工具
        </button>
        <button
          :class="['ext-tab', activeTab === 'skills' && 'ext-tab--active']"
          @click="activeTab = 'skills'"
        >
          <Sparkles :size="13" /> 技能
        </button>
        <button
          :class="['ext-tab', activeTab === 'mcp' && 'ext-tab--active']"
          @click="activeTab = 'mcp'"
        >
          <Globe :size="13" /> MCP
        </button>
      </div>

      <!-- Tools Tab -->
      <div v-if="activeTab === 'tools'" class="ext-body">
        <p class="ext-hint">Agent 可调用的内置工具。</p>
        <div v-for="tool in builtinTools" :key="tool.name" class="ext-item">
          <div class="ext-item__icon"><Wrench :size="13" /></div>
          <div class="ext-item__info">
            <span class="ext-item__name">{{ tool.name }}</span>
            <span class="ext-item__desc">{{ tool.description }}</span>
          </div>
          <CircleCheck :size="14" class="ext-item__check" />
        </div>
      </div>

      <!-- Skills Tab -->
      <div v-if="activeTab === 'skills'" class="ext-body">
        <p class="ext-hint">选中的技能会注入 Agent 的行为提示词，影响分析和生成风格。</p>
        <div v-for="skill in SKILLS" :key="skill.id" class="ext-item ext-item--clickable" @click="toggleSkill(skill.id)">
          <div class="ext-item__icon"><Sparkles :size="13" /></div>
          <div class="ext-item__info">
            <span class="ext-item__name">{{ skill.name }}</span>
            <span class="ext-item__desc">{{ skill.description }}</span>
          </div>
          <button
            class="ext-toggle"
            :class="{ 'ext-toggle--on': selectedSkillIds.includes(skill.id) }"
            type="button"
            @click.stop="toggleSkill(skill.id)"
          >
            <span class="ext-toggle__knob" />
          </button>
        </div>
      </div>

      <!-- MCP Tab -->
      <div v-if="activeTab === 'mcp'" class="ext-body">
        <p class="ext-hint">MCP（Model Context Protocol）服务器连接。未来可接入外部工具服务。</p>
        <div v-for="server in mcpServers" :key="server.name" class="ext-item">
          <div class="ext-item__icon"><Globe :size="13" /></div>
          <div class="ext-item__info">
            <span class="ext-item__name">{{ server.name }}</span>
            <span class="ext-item__desc">{{ server.count }} 个工具已注册</span>
          </div>
          <span class="ext-tag">{{ server.status }}</span>
        </div>
        <div class="ext-empty">
          <span>更多 MCP Server 集成即将推出</span>
        </div>
      </div>
    </aside>
  </Transition>
</template>

<style scoped>
.ext-panel {
  position: fixed;
  top: 0;
  right: 0;
  bottom: 0;
  width: 380px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--color-border-subtle);
  background: var(--color-surface, #0f1420);
  box-shadow: -8px 0 32px rgb(0 0 0 / 40%);
}

.ext-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 12px 16px;
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
}
.ext-header__icon { color: var(--color-accent); display: flex; }
.ext-header__title {
  flex: 1;
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}
.ext-close {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  color: var(--color-text-tertiary);
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
}
.ext-close:hover { color: var(--color-text); background: var(--color-surface-hover); }

.ext-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--color-border-subtle);
  flex-shrink: 0;
}
.ext-tab {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 8px 0;
  color: var(--color-text-tertiary);
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease-out), border-color var(--duration-fast) var(--ease-out);
}
.ext-tab--active {
  color: var(--color-accent);
  border-bottom-color: var(--color-accent);
}

.ext-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-3);
}
.ext-hint {
  margin-bottom: var(--space-3);
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.5;
}

.ext-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: 8px;
  background: var(--color-surface-subtle, rgb(255 255 255 / 2%));
  transition: border-color var(--duration-fast) var(--ease-out);
}
.ext-item + .ext-item { margin-top: var(--space-2); }
.ext-item--clickable { cursor: pointer; }
.ext-item--clickable:hover { border-color: rgb(99 102 241 / 30%); }
.ext-item__icon { color: var(--color-accent); display: flex; padding-top: 2px; flex-shrink: 0; }
.ext-item__info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.ext-item__name {
  color: var(--color-text);
  font-size: 12px;
  font-weight: 600;
  font-family: var(--font-mono, monospace);
}
.ext-item__desc {
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.4;
}
.ext-item__check { color: var(--color-accent); flex-shrink: 0; margin-top: 2px; }

.ext-toggle {
  position: relative;
  width: 34px;
  height: 18px;
  flex-shrink: 0;
  border: none;
  border-radius: 9px;
  background: rgb(255 255 255 / 12%);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}
.ext-toggle--on { background: var(--color-accent); }
.ext-toggle__knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #fff;
  transition: transform var(--duration-fast) var(--ease-out);
}
.ext-toggle--on .ext-toggle__knob { transform: translateX(16px); }

.ext-tag {
  padding: 1px 6px;
  color: var(--color-accent);
  background: var(--color-accent-soft, rgb(99 102 241 / 10%));
  border-radius: 4px;
  font-size: 10px;
  font-weight: 500;
  white-space: nowrap;
  flex-shrink: 0;
}

.ext-empty {
  display: grid;
  place-items: center;
  padding: var(--space-5);
  color: var(--color-text-tertiary);
  font-size: 11px;
}

/* Transition */
.ext-slide-enter-active,
.ext-slide-leave-active {
  transition: transform 200ms var(--ease-out, ease), opacity 200ms ease;
}
.ext-slide-enter-from,
.ext-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
