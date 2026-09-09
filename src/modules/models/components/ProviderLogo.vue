<script setup lang="ts">
import { computed } from "vue";

/**
 * 供应商品牌图标：品牌色圆角瓦片 + 白色短标识。
 * 已知供应商使用固定品牌色；未知供应商按名称哈希取色，
 * 标识回退为显示名称的首字母（支持中文）。
 */
const props = withDefaults(
  defineProps<{
    provider?: string;
    label?: string;
    /** 瓦片边长（px） */
    size?: number;
  }>(),
  { provider: "", label: "", size: 28 },
);

interface Brand {
  color: string;
  mark: string;
}

/** 已知供应商品牌色与短标识（providerName 全等匹配，小写）。 */
const PROVIDER_BRANDS: Record<string, Brand> = {
  deepseek: { color: "#4D6BFE", mark: "DS" },
  openai: { color: "#10A37F", mark: "AI" },
  anthropic: { color: "#D97757", mark: "CL" },
  claude: { color: "#D97757", mark: "CL" },
  gemini: { color: "#3E7BFA", mark: "GE" },
  moonshot: { color: "#16A0AB", mark: "K" },
  kimi: { color: "#16A0AB", mark: "K" },
  zhipu: { color: "#3D7BFF", mark: "Z" },
  glm: { color: "#3D7BFF", mark: "Z" },
  qianfan: { color: "#2932E1", mark: "千帆" },
  baidu: { color: "#2932E1", mark: "BD" },
  dashscope: { color: "#615CED", mark: "百炼" },
  minimax: { color: "#F53E3E", mark: "MM" },
  stepfun: { color: "#7C5CFF", mark: "阶跃" },
  doubao: { color: "#4E6EF2", mark: "豆包" },
  seedance: { color: "#0B6EF6", mark: "SD" },
  siliconflow: { color: "#8B5CF6", mark: "Si" },
  openrouter: { color: "#6467F2", mark: "OR" },
  xiaomi: { color: "#FF6900", mark: "Mi" },
  mimo: { color: "#FF6900", mark: "Mi" },
  nvidia: { color: "#76B900", mark: "NV" },
  groq: { color: "#F55036", mark: "GQ" },
  mistral: { color: "#FA500F", mark: "M" },
  cohere: { color: "#5A6B66", mark: "CO" },
  modelscope: { color: "#7D3AFF", mark: "MS" },
  jimeng: { color: "#3D7BFF", mark: "即梦" },
  dashscope_captioner: { color: "#615CED", mark: "百炼" },
};

/** 未知供应商的回退色板。 */
const FALLBACK_COLORS = [
  "#64748B",
  "#0EA5E9",
  "#8B5CF6",
  "#EC4899",
  "#F59E0B",
  "#10B981",
  "#6366F1",
  "#EF4444",
];

function hashString(text: string): number {
  let hash = 0;
  for (let i = 0; i < text.length; i += 1) {
    hash = (hash * 31 + text.charCodeAt(i)) % 0x7fffffff;
  }
  return hash;
}

const brand = computed<Brand>(() => {
  const key = props.provider.trim().toLowerCase();
  if (key && PROVIDER_BRANDS[key]) return PROVIDER_BRANDS[key];
  const seed = key || props.label.trim().toLowerCase() || "?";
  const color = FALLBACK_COLORS[hashString(seed) % FALLBACK_COLORS.length];
  return { color, mark: "" };
});

const display = computed<string>(() => {
  if (brand.value.mark) return brand.value.mark;
  const source = (props.label || props.provider).trim();
  if (!source) return "?";
  // 取显示名称的前 1-2 个字符（中文取 1 个，拉丁取首字母大写）
  const first = source[0];
  return /[\u4e00-\u9fa5]/.test(first) ? first : first.toUpperCase();
});

const fontSize = computed<string>(() => {
  const len = display.value.length;
  const ratio = len >= 2 ? 0.36 : 0.46;
  return `${Math.round(props.size * ratio)}px`;
});
</script>

<template>
  <span
    class="provider-logo"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      flex: `0 0 ${size}px`,
      background: brand.color,
      fontSize,
    }"
    :title="label || provider"
    aria-hidden="true"
  >
    {{ display }}
  </span>
</template>

<style scoped>
.provider-logo {
  display: inline-grid;
  place-items: center;
  color: #fff;
  border-radius: 8px;
  font-weight: 700;
  letter-spacing: -0.02em;
  line-height: 1;
  user-select: none;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 12%);
}
</style>
