/**
 * 技能（Skills）定义：可开关的提示词修饰片段。
 *
 * 选中的技能不污染用户消息，而是作为 system_prompt_override
 * 通过 sendMessage IPC 追加到 Agent 会话的系统提示词上，
 * 影响规划与回答的分析/生成风格。
 */

export interface SkillItem {
  id: string;
  name: string;
  description: string;
  /** 选中时注入系统提示词的指令片段 */
  promptModifier: string;
  /** 直接生成模式下追加到创作提示词的风格提示 */
  generationHint: string;
}

export const EXTENSION_SKILLS: SkillItem[] = [
  {
    id: "pro-photographer",
    name: "专业摄影师",
    description: "以专业摄影视角分析和生成图片，关注构图、光线、色调",
    promptModifier:
      "你是一位专业摄影师。分析和描述图片时关注构图法则、光线方向、色温、景深等摄影要素。生成图片提示词时使用专业摄影术语。",
    generationHint: "专业摄影质感：注重构图法则、光线方向、色温与景深层次",
  },
  {
    id: "ecommerce-copy",
    name: "电商文案",
    description: "以电商运营视角优化图片文案和产品描述",
    promptModifier:
      "你是一位资深电商文案策划。分析图片时关注产品卖点、营销话术、目标人群和转化要素。生成内容时使用吸引眼球的表达。",
    generationHint: "电商视觉风格：突出产品主体与卖点，画面干净、商业转化导向",
  },
  {
    id: "brand-designer",
    name: "品牌设计师",
    description: "以品牌视觉规范视角审视设计，关注一致性和调性",
    promptModifier:
      "你是一位品牌视觉设计师。分析图片时关注品牌一致性、色彩体系、字体规范和视觉层级。确保生成内容符合品牌调性。",
    generationHint: "品牌设计规范：色彩体系统一、视觉层级清晰、整体调性一致",
  },
  {
    id: "storyboard-director",
    name: "分镜导演",
    description: "以影视导演视角分析画面，关注镜头语言和叙事节奏",
    promptModifier:
      "你是一位影视分镜导演。分析图片时关注镜头角度、景别、画面节奏和叙事意图。生成内容时考虑视觉叙事的连贯性。",
    generationHint: "影视分镜语言：明确的镜头角度与景别，画面有叙事张力",
  },
];

const STORAGE_KEY = "aigc.selectedSkillIds";

/** 从 localStorage 读取上次选中的技能 id（损坏/缺失时返回空数组）。 */
export function loadSelectedSkillIds(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const known = new Set(EXTENSION_SKILLS.map((s) => s.id));
    return parsed.filter(
      (v): v is string => typeof v === "string" && known.has(v),
    );
  } catch {
    return [];
  }
}

/** 持久化选中的技能 id，重启后恢复。 */
export function saveSelectedSkillIds(ids: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
  } catch {
    // 存储不可用（如配额）时静默忽略，技能选择退化为会话内有效
  }
}

/** 将选中技能编译为直接生成模式的风格提示；未选中时返回 undefined。 */
export function buildGenerationHint(selectedIds: string[]): string | undefined {
  const parts: string[] = [];
  for (const id of selectedIds) {
    const skill = EXTENSION_SKILLS.find((s) => s.id === id);
    if (skill) parts.push(skill.generationHint);
  }
  if (parts.length === 0) return undefined;
  return `风格技能:${parts.join("；")}`;
}

/** 将选中技能编译为系统提示词覆盖片段；未选中任何技能时返回 undefined。 */
export function buildSkillOverride(selectedIds: string[]): string | undefined {
  const parts: string[] = [];
  for (const id of selectedIds) {
    const skill = EXTENSION_SKILLS.find((s) => s.id === id);
    if (skill) parts.push(`- ${skill.promptModifier}`);
  }
  if (parts.length === 0) return undefined;
  return `[已启用技能（本次对话始终遵循）]\n${parts.join("\n")}`;
}

/**
 * 组合技能覆盖与项目记忆（.openaigc/AIGC.md）为最终 systemPromptOverride。
 * 两者都为空时返回 undefined。
 */
export function buildSystemPromptOverride(
  selectedIds: string[],
  projectMemory?: string | null,
): string | undefined {
  const skillPart = buildSkillOverride(selectedIds);
  const memory = projectMemory?.trim();
  const memoryPart =
    memory && memory.length > 0
      ? `[项目说明（来自 .openaigc/AIGC.md，始终遵循）]\n${memory}`
      : undefined;
  if (skillPart && memoryPart) return `${skillPart}\n\n${memoryPart}`;
  return skillPart ?? memoryPart;
}
