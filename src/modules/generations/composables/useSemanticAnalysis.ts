/**
 * useSemanticAnalysis — 图片语义分析 composable。
 *
 * 提供对图片进行 captioning 分析和语义搜索的能力。
 * 通过 SemanticPipeline（DashScope Qwen-VL / Florence-2）生成语义画像。
 */

import { ref } from "vue";
import {
  analyzeAsset,
  analyzeAssetsBatch,
  searchAssetsSemantic,
  type AnalysisResult,
  type RetrievalResult,
} from "../../../bridge/agent";

export function useSemanticAnalysis() {
  const isAnalyzing = ref(false);
  const analysisError = ref<string | null>(null);
  const lastResult = ref<AnalysisResult | null>(null);
  const searchResults = ref<RetrievalResult[]>([]);

  /**
   * 分析单张图片。
   */
  async function analyze(
    assetId: string,
    imageDataUrl: string,
    preferredAdapter?: string,
  ): Promise<AnalysisResult | null> {
    isAnalyzing.value = true;
    analysisError.value = null;
    try {
      const result = await analyzeAsset(assetId, imageDataUrl, preferredAdapter);
      lastResult.value = result;
      return result;
    } catch (e) {
      analysisError.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      isAnalyzing.value = false;
    }
  }

  /**
   * 批量分析图片。
   */
  async function analyzeBatch(
    items: [string, string][],
    preferredAdapter?: string,
  ): Promise<AnalysisResult[]> {
    isAnalyzing.value = true;
    analysisError.value = null;
    try {
      const results = await analyzeAssetsBatch(items, preferredAdapter);
      return results;
    } catch (e) {
      analysisError.value = e instanceof Error ? e.message : String(e);
      return [];
    } finally {
      isAnalyzing.value = false;
    }
  }

  /**
   * 语义搜索资源。
   */
  async function search(query: string, limit = 10): Promise<RetrievalResult[]> {
    analysisError.value = null;
    try {
      const results = await searchAssetsSemantic(query, limit);
      searchResults.value = results;
      return results;
    } catch (e) {
      analysisError.value = e instanceof Error ? e.message : String(e);
      return [];
    }
  }

  return {
    isAnalyzing,
    analysisError,
    lastResult,
    searchResults,
    analyze,
    analyzeBatch,
    search,
  };
}
