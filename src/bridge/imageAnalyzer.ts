import { z } from "zod";

import { invokeNative } from "./native";

// ── Schemas ──────────────────────────────────────────

const colorInfoSchema = z.object({
  name: z.string(),
  hex: z.string(),
  percentage: z.number(),
});

const imageAnalysisSchema = z.object({
  width: z.number(),
  height: z.number(),
  fileSizeBytes: z.number(),
  format: z.string(),
  aspectRatio: z.string(),
  dominantColors: z.array(colorInfoSchema),
  description: z.string(),
});

// ── Types ────────────────────────────────────────────

export type ImageAnalysis = z.infer<typeof imageAnalysisSchema>;
export type ColorInfo = z.infer<typeof colorInfoSchema>;

// ── IPC ──────────────────────────────────────────────

/**
 * 本地分析图片文件，提取尺寸、格式、主色调等元信息。
 * 不调用 LLM，纯本地计算，用于为 Agent 提供画布上下文。
 */
export async function analyzeImage(path: string): Promise<ImageAnalysis> {
  return invokeNative("image_v1_analyze", imageAnalysisSchema, { path });
}
