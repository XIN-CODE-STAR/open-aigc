/**
 * useAutoMemoryPrefill — 创意记忆自动预填充 composable。
 *
 * 根据用户的创意记忆（风格偏好、负面偏好、品牌规范），
 * 自动预填充视觉规范、Prompt 参数和审核标准。
 */
import { ref, computed } from "vue";
import type { CreativeMemoryRecord } from "../../../bridge/creative_memory";
import { listCreativeMemories, parseMemoryContent } from "../../../bridge/creative_memory";

export interface UseAutoMemoryPrefillOptions {
  userId: string;
  projectId?: string;
  autoLoad?: boolean;
}

interface StylePreferenceContent {
  color_palette?: string[];
  composition?: string;
  lighting?: string;
  texture?: string;
  camera?: string;
  mood?: string;
}

interface NegativePreferenceContent {
  avoid?: string[];
  never_use?: string[];
}

interface BrandRuleContent {
  brand_colors?: string[];
  logo_rules?: string;
  banned_words?: string[];
  tone_of_voice?: string;
}

export function useAutoMemoryPrefill(options: UseAutoMemoryPrefillOptions) {
  const { userId, autoLoad = true } = options;

  const stylePreferences = ref<CreativeMemoryRecord[]>([]);
  const negativePreferences = ref<CreativeMemoryRecord[]>([]);
  const brandRules = ref<CreativeMemoryRecord[]>([]);
  const loading = ref(false);

  /**
   * 生成预填充的 Visual Spec 片段。
   * 从用户的风格偏好记忆中提取并合并。
   */
  const prefilledVisualSpec = computed(() => {
    const spec: Record<string, unknown> = {};

    for (const memory of stylePreferences.value) {
      const content = parseMemoryContent<StylePreferenceContent>(memory.contentJson);
      if (!content) continue;

      if (content.color_palette) {
        spec.colorPalette = [...((spec.colorPalette as string[]) ?? []), ...content.color_palette];
      }
      if (content.composition) spec.composition = content.composition;
      if (content.lighting) spec.lighting = content.lighting;
      if (content.texture) spec.texture = content.texture;
      if (content.camera) spec.camera = content.camera;
      if (content.mood) spec.mood = content.mood;
    }

    // 去重
    if (spec.colorPalette) {
      spec.colorPalette = [...new Set(spec.colorPalette as string[])];
    }

    return spec;
  });

  /**
   * 生成预填充的 Negative Prompt 片段。
   * 从用户的负面偏好记忆中提取并合并。
   */
  const prefilledNegativePrompt = computed(() => {
    const negatives: string[] = [];

    for (const memory of negativePreferences.value) {
      const content = parseMemoryContent<NegativePreferenceContent>(memory.contentJson);
      if (!content) continue;

      if (content.avoid) negatives.push(...content.avoid);
      if (content.never_use) negatives.push(...content.never_use);
    }

    return [...new Set(negatives)];
  });

  /**
   * 生成预填充的品牌规范。
   * 从品牌规则记忆中提取。
   */
  const prefilledBrandRules = computed(() => {
    const rules: Record<string, unknown> = {};

    for (const memory of brandRules.value) {
      const content = parseMemoryContent<BrandRuleContent>(memory.contentJson);
      if (!content) continue;

      if (content.brand_colors) {
        rules.brandColors = [...((rules.brandColors as string[]) ?? []), ...content.brand_colors];
      }
      if (content.logo_rules) rules.logoRules = content.logo_rules;
      if (content.banned_words) {
        rules.bannedWords = [...((rules.bannedWords as string[]) ?? []), ...content.banned_words];
      }
      if (content.tone_of_voice) rules.toneOfVoice = content.tone_of_voice;
    }

    if (rules.brandColors) {
      rules.brandColors = [...new Set(rules.brandColors as string[])];
    }
    if (rules.bannedWords) {
      rules.bannedWords = [...new Set(rules.bannedWords as string[])];
    }

    return rules;
  });

  /**
   * 生成用于 Prompt 前缀的风格描述文本。
   * 将用户的风格偏好转换为可直接插入 Prompt 的文本片段。
   */
  const stylePromptPrefix = computed(() => {
    const spec = prefilledVisualSpec.value;
    const parts: string[] = [];

    if (spec.mood) parts.push(`${spec.mood} mood`);
    if (spec.lighting) parts.push(`${spec.lighting} lighting`);
    if (spec.composition) parts.push(`${spec.composition} composition`);
    if (spec.colorPalette && (spec.colorPalette as string[]).length > 0) {
      parts.push(`color palette: ${(spec.colorPalette as string[]).join(", ")}`);
    }

    return parts.join(", ");
  });

  /**
   * 生成用于 Negative Prompt 的文本。
   */
  const negativePromptText = computed(() => {
    return prefilledNegativePrompt.value.join(", ");
  });

  /**
   * 加载用户的创意记忆。
   */
  async function loadMemories(): Promise<void> {
    loading.value = true;
    try {
      const allMemories = await listCreativeMemories({ status: "active", limit: 200 });

      // 按用户过滤
      const userMemories = allMemories.filter((m) => m.createdBy === userId);

      // 按类型分组
      stylePreferences.value = userMemories.filter((m) => m.memoryType === "style_preference");
      negativePreferences.value = userMemories.filter(
        (m) => m.memoryType === "negative_preference",
      );
      brandRules.value = userMemories.filter((m) => m.memoryType === "brand_rule");
    } catch {
      // 静默失败，不影响主流程
    } finally {
      loading.value = false;
    }
  }

  /**
   * 将预填充数据应用到 Visual Spec 对象。
   * 返回合并后的新对象（不修改原始对象）。
   */
  function applyToVisualSpec<T extends Record<string, unknown>>(
    spec: T,
  ): T & Record<string, unknown> {
    const prefilled = prefilledVisualSpec.value;
    return { ...prefilled, ...spec };
  }

  /**
   * 将预填充数据应用到 Negative Prompt。
   * 返回合并后的字符串。
   */
  function applyToNegativePrompt(existing: string): string {
    const prefilled = prefilledNegativePrompt.value;
    if (prefilled.length === 0) return existing;

    const existingParts = existing
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const merged = [...new Set([...existingParts, ...prefilled])];
    return merged.join(", ");
  }

  /**
   * 检查内容是否包含品牌禁止词。
   */
  function checkBrandBannedWords(content: string): string[] {
    const banned = prefilledBrandRules.value.bannedWords as string[] | undefined;
    if (!banned || banned.length === 0) return [];

    const lower = content.toLowerCase();
    return banned.filter((word) => lower.includes(word.toLowerCase()));
  }

  // 自动加载
  if (autoLoad) {
    void loadMemories();
  }

  return {
    stylePreferences,
    negativePreferences,
    brandRules,
    loading,
    prefilledVisualSpec,
    prefilledNegativePrompt,
    prefilledBrandRules,
    stylePromptPrefix,
    negativePromptText,
    loadMemories,
    applyToVisualSpec,
    applyToNegativePrompt,
    checkBrandBannedWords,
  };
}
