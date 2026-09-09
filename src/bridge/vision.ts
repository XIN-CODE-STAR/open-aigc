/**
 * LLM Vision API — 前端桥接层。
 *
 * 封装 Claude/GPT Vision API 的调用，用于 AI Critic Agent 的自动评价。
 */
import { z } from "zod";
import { invokeNative, isNativeRuntimeAvailable, NativeCommandError } from "./native";

// ═══════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════

const visionAdapterSchema = z
  .object({
    id: z.string().min(1),
    name: z.string().min(1),
    provider: z.string().min(1),
    models: z.array(z.string().min(1)),
  })
  .strict();

const evaluateWithVisionRequestSchema = z
  .object({
    projectId: z.string().min(1).uuid(),
    assetId: z.string().min(1),
    imageUrl: z.string().min(1),
    userGoal: z.string().min(1).max(4000).optional(),
    adapterId: z.string().min(1).optional(),
    model: z.string().min(1).optional(),
    apiKey: z.string().min(1).optional(),
  })
  .strict();

// ═══════════════════════════════════════════════════
// 导出类型
// ═══════════════════════════════════════════════════

export type VisionAdapter = z.infer<typeof visionAdapterSchema>;
export type EvaluateWithVisionRequest = z.infer<typeof evaluateWithVisionRequestSchema>;

export { NativeCommandError as VisionCommandError };

// ═══════════════════════════════════════════════════
// IPC 调用函数
// ═══════════════════════════════════════════════════

export function isVisionRuntimeAvailable(): boolean {
  return isNativeRuntimeAvailable();
}

/**
 * 使用 LLM Vision API 评价一张图片。
 *
 * 返回评价报告（与 review_v1_get_report 相同结构）。
 */
export async function evaluateWithVision(
  request: EvaluateWithVisionRequest,
): Promise<Record<string, unknown>> {
  const parsedRequest = evaluateWithVisionRequestSchema.parse(request);
  return invokeNative("review_v1_evaluate_with_vision", z.record(z.string(), z.unknown()), {
    request: parsedRequest,
  });
}

/**
 * 获取支持的 Vision 适配器列表。
 */
export async function listVisionAdapters(): Promise<VisionAdapter[]> {
  return invokeNative("review_v1_list_vision_adapters", z.array(visionAdapterSchema));
}

/**
 * 自动触发 AI Critic 评价（图片生成完成后调用）。
 *
 * 这是一个便捷函数，自动构造请求并调用 Vision API。
 */
export async function autoEvaluateImage(
  projectId: string,
  assetId: string,
  imageUrl: string,
  userGoal?: string,
): Promise<Record<string, unknown>> {
  return evaluateWithVision({
    projectId,
    assetId,
    imageUrl,
    userGoal,
  });
}

/**
 * 将 Vision 适配器翻译为用户可读的标签。
 */
export function visionAdapterLabel(adapterId: string): string {
  switch (adapterId) {
    case "claude-vision":
      return "Claude Vision (Anthropic)";
    case "gpt-vision":
      return "GPT Vision (OpenAI)";
    default:
      return adapterId;
  }
}

/**
 * 获取适配器的默认模型。
 */
export function defaultModelForAdapter(adapterId: string): string {
  switch (adapterId) {
    case "claude-vision":
      return "claude-sonnet-4-20250514";
    case "gpt-vision":
      return "gpt-4o";
    default:
      return "claude-sonnet-4-20250514";
  }
}
