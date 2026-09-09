/**
 * useAssetVersions — 资产版本管理 composable。
 *
 * 管理资产的版本历史、版本状态流转、版本对比和版权信息。
 */
import { ref, computed } from "vue";
import type { AssetVersionRecord, AssetLicenseRecord } from "../../../bridge/review";
import { listAssetVersions, getAssetLicense } from "../../../bridge/review";

export interface UseAssetVersionsOptions {
  assetId: string;
  autoLoad?: boolean;
}

export type AssetVersionStatus =
  | "draft"
  | "reviewing"
  | "approved"
  | "selected"
  | "used_in_deliverable"
  | "archived"
  | "deprecated"
  | "rejected";

export function useAssetVersions(options: UseAssetVersionsOptions) {
  const { assetId, autoLoad = true } = options;

  const versions = ref<AssetVersionRecord[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const license = ref<AssetLicenseRecord | null>(null);
  const loadingLicense = ref(false);

  const latestVersion = computed(() => versions.value[0] ?? null);
  const approvedVersions = computed(() =>
    versions.value.filter((v) => v.status === "approved" || v.status === "selected"),
  );
  const rejectedVersions = computed(() => versions.value.filter((v) => v.status === "rejected"));

  async function loadVersions(limit = 50): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      versions.value = await listAssetVersions(assetId, limit);
    } catch (e) {
      error.value = e instanceof Error ? e.message : "加载版本历史失败";
    } finally {
      loading.value = false;
    }
  }

  async function loadLicense(): Promise<void> {
    loadingLicense.value = true;
    try {
      license.value = await getAssetLicense(assetId);
    } catch {
      license.value = null;
    } finally {
      loadingLicense.value = false;
    }
  }

  function versionStatusLabel(status: AssetVersionStatus): string {
    const labels: Record<AssetVersionStatus, string> = {
      draft: "草稿",
      reviewing: "审核中",
      approved: "已批准",
      selected: "已选用",
      used_in_deliverable: "已用于成片",
      archived: "已归档",
      deprecated: "已弃用",
      rejected: "已拒绝",
    };
    return labels[status] ?? status;
  }

  function versionStatusColor(status: AssetVersionStatus): string {
    switch (status) {
      case "approved":
      case "selected":
      case "used_in_deliverable":
        return "var(--color-success)";
      case "reviewing":
        return "var(--color-info)";
      case "rejected":
        return "var(--color-danger)";
      case "deprecated":
      case "archived":
        return "var(--color-text-disabled)";
      default:
        return "var(--color-text-secondary)";
    }
  }

  function isTerminal(status: AssetVersionStatus): boolean {
    return ["archived", "deprecated", "rejected"].includes(status);
  }

  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDuration(seconds: number | null): string {
    if (!seconds) return "";
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return m > 0 ? `${m}m${s}s` : `${s}s`;
  }

  if (autoLoad) {
    void loadVersions();
    void loadLicense();
  }

  return {
    versions,
    loading,
    error,
    license,
    loadingLicense,
    latestVersion,
    approvedVersions,
    rejectedVersions,
    loadVersions,
    loadLicense,
    versionStatusLabel,
    versionStatusColor,
    isTerminal,
    formatFileSize,
    formatDuration,
  };
}
