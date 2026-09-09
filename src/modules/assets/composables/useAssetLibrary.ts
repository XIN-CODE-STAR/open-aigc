import { computed, ref } from "vue";

import {
  AssetCommandError,
  listAssets,
  reverifyAssets,
  type AssetKind,
  type AssetRecord,
  type AssetReverificationSummary,
  type IntegrityStatus,
  type StorageNamespace,
} from "../../../bridge/assets";

export type AssetLibraryPhase = "idle" | "loading" | "ready" | "error";

export function useAssetLibrary() {
  const phase = ref<AssetLibraryPhase>("idle");
  const search = ref("");
  const storageNamespace = ref<StorageNamespace | undefined>(undefined);
  const assetKind = ref<AssetKind | undefined>(undefined);
  const integrityStatus = ref<IntegrityStatus | undefined>(undefined);
  const assets = ref<AssetRecord[]>([]);
  const errorMessage = ref<string>();
  const errorField = ref<string>();
  const reverifying = ref(false);
  const reverificationResult = ref<AssetReverificationSummary | null>(null);
  let listRequest = 0;

  const isBusy = computed(() => phase.value === "loading" || reverifying.value);

  const summary = computed(() => {
    const records = assets.value;
    const byKind = new Map<AssetKind, number>();
    let totalBytes = 0;
    let missing = 0;
    let corrupt = 0;
    for (const record of records) {
      byKind.set(record.assetKind, (byKind.get(record.assetKind) ?? 0) + 1);
      totalBytes += record.sizeBytes;
      if (record.integrityStatus === "missing") missing += 1;
      if (record.integrityStatus === "corrupt") corrupt += 1;
    }
    return {
      total: records.length,
      totalBytes,
      missing,
      corrupt,
      byKind,
    };
  });

  async function load(): Promise<void> {
    const request = ++listRequest;
    clearError();
    phase.value = "loading";
    try {
      const records = await listAssets({
        search: search.value || undefined,
        storageNamespace: storageNamespace.value,
        assetKind: assetKind.value,
        integrityStatus: integrityStatus.value,
      });
      if (request !== listRequest) return;
      assets.value = records;
      phase.value = "ready";
    } catch (error) {
      if (request !== listRequest) return;
      assets.value = [];
      setError(error);
    }
  }

  async function reverify(): Promise<void> {
    reverifying.value = true;
    reverificationResult.value = null;
    try {
      const result = await reverifyAssets();
      reverificationResult.value = result;
      await load();
    } catch (error) {
      setError(error);
    } finally {
      reverifying.value = false;
    }
  }

  function clearError(): void {
    errorMessage.value = undefined;
    errorField.value = undefined;
  }

  function setError(error: unknown): void {
    phase.value = "error";
    if (error instanceof AssetCommandError) {
      errorMessage.value = error.message;
      errorField.value = error.field;
    } else {
      errorMessage.value = "资源库读取失败，请稍后重试。";
    }
  }

  return {
    phase,
    search,
    storageNamespace,
    assetKind,
    integrityStatus,
    assets,
    summary,
    errorMessage,
    errorField,
    isBusy,
    reverifying,
    reverificationResult,
    load,
    reverify,
    clearError,
  };
}
