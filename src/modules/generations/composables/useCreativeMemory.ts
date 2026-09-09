/**
 * useCreativeMemory — 用户创意记忆 composable。
 *
 * 管理用户的风格偏好、负面偏好、品牌规范、工作流习惯等记忆。
 * 支持记忆的增删改查、暂停/恢复、置信度确认。
 */
import { ref, computed } from "vue";
import type { CreativeMemoryRecord, MemoryType } from "../../../bridge/creative_memory";
import {
  listCreativeMemories,
  saveCreativeMemory,
  updateMemoryStatus,
  confirmMemory,
  deleteMemory,
  memoryTypeLabel,
  memoryStatusLabel,
  parseMemoryContent,
} from "../../../bridge/creative_memory";

export type { MemoryType };

export interface UseCreativeMemoryOptions {
  userId: string;
  autoLoad?: boolean;
}

export function useCreativeMemory(options: UseCreativeMemoryOptions) {
  const { userId, autoLoad = true } = options;

  const memories = ref<CreativeMemoryRecord[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const saving = ref(false);

  // 按类型分组的记忆
  const memoriesByType = computed(() => {
    const grouped: Partial<Record<MemoryType, CreativeMemoryRecord[]>> = {};
    for (const memory of memories.value) {
      const type = memory.memoryType;
      const group = grouped[type] ?? [];
      group.push(memory);
      grouped[type] = group;
    }
    return grouped;
  });

  const activeMemories = computed(() => memories.value.filter((m) => m.status === "active"));

  const stylePreferences = computed(() => memoriesByType.value.style_preference ?? []);

  const negativePreferences = computed(() => memoriesByType.value.negative_preference ?? []);

  const brandRules = computed(() => memoriesByType.value.brand_rule ?? []);

  // ─── 加载记忆 ────────────────────────────────

  async function loadMemories(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      memories.value = await listCreativeMemories({
        status: "active",
        limit: 100,
      });
    } catch (e) {
      error.value = e instanceof Error ? e.message : "加载创意记忆失败";
    } finally {
      loading.value = false;
    }
  }

  // ─── 保存记忆 ────────────────────────────────

  async function saveMemory(
    memoryType: MemoryType,
    content: Record<string, unknown>,
    summary: string,
    scope: "user" | "project" | "brand" = "user",
    scopeRefId?: string,
  ): Promise<CreativeMemoryRecord | null> {
    saving.value = true;
    error.value = null;
    try {
      const record = await saveCreativeMemory({
        memoryType,
        scope,
        scopeRefId,
        contentJson: JSON.stringify(content),
        summary,
        source: "explicit_save",
        createdBy: userId,
      });
      memories.value = [record, ...memories.value];
      return record;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "保存记忆失败";
      return null;
    } finally {
      saving.value = false;
    }
  }

  // ─── 暂停/恢复/删除 ─────────────────────────

  async function pauseMemory(id: string): Promise<boolean> {
    try {
      const record = await updateMemoryStatus(id, "paused");
      const idx = memories.value.findIndex((m) => m.id === id);
      if (idx >= 0) memories.value[idx] = record;
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "暂停记忆失败";
      return false;
    }
  }

  async function resumeMemory(id: string): Promise<boolean> {
    try {
      const record = await updateMemoryStatus(id, "active");
      const idx = memories.value.findIndex((m) => m.id === id);
      if (idx >= 0) memories.value[idx] = record;
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "恢复记忆失败";
      return false;
    }
  }

  async function removeMemory(id: string): Promise<boolean> {
    try {
      await deleteMemory(id);
      memories.value = memories.value.filter((m) => m.id !== id);
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "删除记忆失败";
      return false;
    }
  }

  async function confirmMemoryById(id: string): Promise<boolean> {
    try {
      const record = await confirmMemory(id);
      const idx = memories.value.findIndex((m) => m.id === id);
      if (idx >= 0) memories.value[idx] = record;
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "确认记忆失败";
      return false;
    }
  }

  // ─── 便捷方法 ────────────────────────────────

  /**
   * 保存风格偏好。
   * @param preferences 例如 { colorPalette: ["warm", "muted"], composition: "rule_of_thirds" }
   * @param summary 人类可读摘要
   */
  async function saveStylePreference(
    preferences: Record<string, unknown>,
    summary: string,
  ): Promise<CreativeMemoryRecord | null> {
    return saveMemory("style_preference", preferences, summary);
  }

  /**
   * 保存负面偏好（不喜欢的风格）。
   * @param avoid 例如 { avoid: ["网红感", "赛博霓虹", "过度磨皮"] }
   * @param summary 人类可读摘要
   */
  async function saveNegativePreference(
    avoid: Record<string, unknown>,
    summary: string,
  ): Promise<CreativeMemoryRecord | null> {
    return saveMemory("negative_preference", avoid, summary);
  }

  /**
   * 解析记忆内容。
   */
  function getContent<T>(memory: CreativeMemoryRecord): T | null {
    return parseMemoryContent<T>(memory.contentJson);
  }

  // ─── 自动加载 ────────────────────────────────

  if (autoLoad) {
    void loadMemories();
  }

  return {
    memories,
    loading,
    error,
    saving,
    memoriesByType,
    activeMemories,
    stylePreferences,
    negativePreferences,
    brandRules,
    loadMemories,
    saveMemory,
    pauseMemory,
    resumeMemory,
    removeMemory,
    confirmMemoryById,
    saveStylePreference,
    saveNegativePreference,
    getContent,
    memoryTypeLabel,
    memoryStatusLabel,
  };
}
