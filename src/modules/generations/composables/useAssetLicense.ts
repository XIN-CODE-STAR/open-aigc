/**
 * useAssetLicense — 版权管理 composable。
 *
 * 管理资产的版权信息、商用状态、导出权限和风险标记。
 */
import { ref, computed } from "vue";
import type { AssetLicenseRecord } from "../../../bridge/review";
import { getAssetLicense } from "../../../bridge/review";

export interface UseAssetLicenseOptions {
  assetId: string;
  autoLoad?: boolean;
}

export type CommercialUseStatus = "clear" | "needs_review" | "restricted" | "blocked" | "unknown";
export type LicenseSourceType = "ai_generated" | "user_uploaded" | "third_party" | "derived";

export function useAssetLicense(options: UseAssetLicenseOptions) {
  const { assetId, autoLoad = true } = options;

  const license = ref<AssetLicenseRecord | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const canExport = computed(() => {
    if (!license.value) return false;
    return license.value.exportAllowed && license.value.commercialUseStatus !== "blocked";
  });

  const needsReview = computed(() => {
    if (!license.value) return false;
    return license.value.commercialUseStatus === "needs_review" || license.value.reviewRequired;
  });

  const riskFlags = computed(() => {
    if (!license.value?.riskFlagsJson) return [];
    try {
      return JSON.parse(license.value.riskFlagsJson) as string[];
    } catch {
      return [];
    }
  });

  async function loadLicense(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      license.value = await getAssetLicense(assetId);
    } catch (e) {
      error.value = e instanceof Error ? e.message : "加载版权信息失败";
    } finally {
      loading.value = false;
    }
  }

  function commercialStatusLabel(status: CommercialUseStatus): string {
    const labels: Record<CommercialUseStatus, string> = {
      clear: "可商用",
      needs_review: "需审核",
      restricted: "受限使用",
      blocked: "禁止商用",
      unknown: "未知",
    };
    return labels[status] ?? status;
  }

  function commercialStatusColor(status: CommercialUseStatus): string {
    switch (status) {
      case "clear":
        return "var(--color-success)";
      case "needs_review":
        return "var(--color-warning)";
      case "restricted":
        return "var(--color-orange)";
      case "blocked":
        return "var(--color-danger)";
      default:
        return "var(--color-text-disabled)";
    }
  }

  function sourceTypeLabel(type: LicenseSourceType): string {
    const labels: Record<LicenseSourceType, string> = {
      ai_generated: "AI 生成",
      user_uploaded: "用户上传",
      third_party: "第三方素材",
      derived: "衍生作品",
    };
    return labels[type] ?? type;
  }

  function riskFlagLabel(flag: string): string {
    const labels: Record<string, string> = {
      third_party_reference: "使用了第三方参考图",
      person_likeness: "包含人物肖像",
      brand_logo: "包含品牌标识",
      copyrighted_material: "可能包含版权素材",
      music_copyright: "音乐版权待确认",
      font_license: "字体授权待确认",
    };
    return labels[flag] ?? flag;
  }

  if (autoLoad) {
    void loadLicense();
  }

  return {
    license,
    loading,
    error,
    canExport,
    needsReview,
    riskFlags,
    loadLicense,
    commercialStatusLabel,
    commercialStatusColor,
    sourceTypeLabel,
    riskFlagLabel,
  };
}
