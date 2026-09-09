<script setup lang="ts">
/**
 * AssetVersionPanel：资产版本历史面板。
 *
 * 展示资产的版本列表、状态流转、版权信息。
 * 用于镜头详情和资产库中查看版本历史。
 */
import { computed, ref } from "vue";
import {
  Clock,
  CheckCircle,
  XCircle,
  Archive,
  Eye,
  FileText,
  GitBranch,
  LoaderCircle,
} from "@lucide/vue";

import { useAssetVersions, type AssetVersionStatus } from "../composables/useAssetVersions";

const props = defineProps<{
  assetId: string;
  autoLoad?: boolean;
}>();

const emit = defineEmits<{
  "select-version": [versionId: string];
  "view-license": [];
}>();

const {
  versions,
  loading,
  error,
  license,
  loadVersions,
  versionStatusLabel,
  versionStatusColor,
  isTerminal,
  formatFileSize,
  formatDuration,
} = useAssetVersions({
  assetId: props.assetId,
  autoLoad: props.autoLoad ?? true,
});

const showAll = ref(false);

const displayedVersions = computed(() =>
  showAll.value ? versions.value : versions.value.slice(0, 5),
);

function statusIcon(status: AssetVersionStatus) {
  switch (status) {
    case "approved":
    case "selected":
    case "used_in_deliverable":
      return CheckCircle;
    case "rejected":
      return XCircle;
    case "archived":
    case "deprecated":
      return Archive;
    case "reviewing":
      return Eye;
    default:
      return Clock;
  }
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}
</script>

<template>
  <div class="asset-version-panel">
    <!-- 头部 -->
    <div class="asset-version-panel__header">
      <div class="asset-version-panel__title">
        <GitBranch :size="14" />
        <span>版本历史</span>
        <span v-if="versions.length > 0" class="asset-version-panel__count">
          {{ versions.length }}
        </span>
      </div>
      <button
        type="button"
        class="asset-version-panel__refresh"
        :disabled="loading"
        @click="loadVersions()"
      >
        <LoaderCircle v-if="loading" :size="12" class="is-spinning" />
        <span v-else>刷新</span>
      </button>
    </div>

    <!-- 加载状态 -->
    <div v-if="loading && versions.length === 0" class="asset-version-panel__loading">
      <LoaderCircle :size="16" class="is-spinning" />
      <span>加载版本历史...</span>
    </div>

    <!-- 错误状态 -->
    <div v-else-if="error" class="asset-version-panel__error">
      <XCircle :size="14" />
      <span>{{ error }}</span>
    </div>

    <!-- 空状态 -->
    <div v-else-if="versions.length === 0" class="asset-version-panel__empty">
      <FileText :size="24" />
      <span>暂无版本记录</span>
    </div>

    <!-- 版本列表 -->
    <div v-else class="asset-version-panel__list">
      <div
        v-for="version in displayedVersions"
        :key="version.id"
        class="version-item"
        :class="{ 'version-item--terminal': isTerminal(version.status as AssetVersionStatus) }"
        @click="emit('select-version', version.id)"
      >
        <div class="version-item__header">
          <component
            :is="statusIcon(version.status as AssetVersionStatus)"
            :size="14"
            :style="{ color: versionStatusColor(version.status as AssetVersionStatus) }"
          />
          <span class="version-item__version">v{{ version.version }}</span>
          <span
            class="version-item__status"
            :style="{ color: versionStatusColor(version.status as AssetVersionStatus) }"
          >
            {{ versionStatusLabel(version.status as AssetVersionStatus) }}
          </span>
          <span class="version-item__time">{{ formatTime(version.createdAt) }}</span>
        </div>

        <div class="version-item__meta">
          <span v-if="version.sizeBytes > 0" class="version-item__size">
            {{ formatFileSize(version.sizeBytes) }}
          </span>
          <span v-if="version.width && version.height" class="version-item__dimensions">
            {{ version.width }}×{{ version.height }}
          </span>
          <span v-if="version.durationSeconds" class="version-item__duration">
            {{ formatDuration(version.durationSeconds) }}
          </span>
          <span class="version-item__source">{{ version.sourceType }}</span>
        </div>

        <div v-if="version.statusReason" class="version-item__reason">
          {{ version.statusReason }}
        </div>
      </div>

      <button
        v-if="versions.length > 5 && !showAll"
        type="button"
        class="asset-version-panel__show-all"
        @click="showAll = true"
      >
        显示全部 {{ versions.length }} 个版本
      </button>
    </div>

    <!-- 版权信息 -->
    <div v-if="license" class="asset-version-panel__license">
      <div class="license-header">
        <FileText :size="14" />
        <span>版权信息</span>
      </div>
      <div class="license-status" :class="`license-status--${license.commercialUseStatus}`">
        {{
          license.commercialUseStatus === "clear"
            ? "可商用"
            : license.commercialUseStatus === "needs_review"
              ? "需审核"
              : license.commercialUseStatus === "restricted"
                ? "受限"
                : license.commercialUseStatus === "blocked"
                  ? "禁止"
                  : "未知"
        }}
      </div>
      <div v-if="license.exportAllowed" class="license-export">
        <CheckCircle :size="12" />
        <span>允许导出</span>
      </div>
      <div v-else class="license-no-export">
        <XCircle :size="12" />
        <span>{{ license.exportBlockReason || "不允许导出" }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.asset-version-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border-radius: var(--radius-surface);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
}

.asset-version-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.asset-version-panel__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
}

.asset-version-panel__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 50%;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 10px;
  font-weight: 600;
}

.asset-version-panel__refresh {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 11px;
  cursor: pointer;
}

.asset-version-panel__refresh:hover:not(:disabled) {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

/* 加载/错误/空状态 */
.asset-version-panel__loading,
.asset-version-panel__error,
.asset-version-panel__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 20px;
  font-size: 12px;
  color: var(--color-text-tertiary);
}

.asset-version-panel__error {
  color: var(--color-danger);
}

/* 版本列表 */
.asset-version-panel__list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.version-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.version-item:hover {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

.version-item--terminal {
  opacity: 0.6;
}

.version-item__header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}

.version-item__version {
  font-weight: 600;
  color: var(--color-text);
}

.version-item__status {
  font-size: 11px;
  font-weight: 500;
}

.version-item__time {
  margin-left: auto;
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.version-item__meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 10px;
  color: var(--color-text-tertiary);
}

.version-item__reason {
  font-size: 11px;
  color: var(--color-text-secondary);
  font-style: italic;
}

.asset-version-panel__show-all {
  padding: 6px 12px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-accent);
  font-size: 11px;
  cursor: pointer;
  text-align: center;
}

.asset-version-panel__show-all:hover {
  background: var(--color-accent-soft);
}

/* 版权信息 */
.asset-version-panel__license {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-control);
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface-subtle);
}

.license-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text);
}

.license-status {
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: var(--radius-control);
  width: fit-content;
}

.license-status--clear {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.license-status--needs_review {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.license-status--restricted {
  background: var(--color-orange-soft);
  color: var(--color-orange);
}

.license-status--blocked {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.license-export,
.license-no-export {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}

.license-export {
  color: var(--color-success);
}

.license-no-export {
  color: var(--color-danger);
}

.is-spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
