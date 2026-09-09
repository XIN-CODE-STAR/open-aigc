<script setup lang="ts">
import { computed, h, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AlertCircle,
  ExternalLink,
  FileUp,
  FolderOpen,
  Images,
  LoaderCircle,
  Play,
  RefreshCw,
  Search,
  ShieldCheck,
} from "@lucide/vue";
import type { ColumnDef } from "@tanstack/vue-table";

import {
  openAssetFile,
  openAssetFolder,
  type AssetKind,
  type AssetRecord,
  type IntegrityStatus,
  type StorageNamespace,
} from "../../../bridge/assets";
import { useWorkspaceStore } from "../../../app/stores/workspace";
import { getWorkspaceRootPath } from "../../../bridge/workspace";
import EmptyState from "../../../shared/ui/EmptyState.vue";
import TeachingDataTable from "../../../shared/ui/TeachingDataTable.vue";
import ImportAssetsDialog from "../components/ImportAssetsDialog.vue";
import { useAssetLibrary } from "../composables/useAssetLibrary";

const workspace = useWorkspaceStore();
const management = useAssetLibrary();
const {
  phase,
  search,
  storageNamespace,
  assetKind,
  integrityStatus,
  assets,
  summary,
  errorMessage,
  isBusy,
  reverifying,
  reverificationResult,
} = management;

const importOpen = ref(false);
const loaded = ref(false);

const kindLabels: Record<AssetKind, string> = {
  image: "图片",
  video: "视频",
  audio: "音频",
  document: "文档",
  archive: "压缩包",
  other: "其他",
};

const namespaceLabels: Record<StorageNamespace, string> = {
  workspace: "工作区",
  generation: "生成结果",
  "teaching-resource": "教学资源",
  system: "系统",
};

const statusLabels: Record<IntegrityStatus, string> = {
  unverified: "未校验",
  valid: "有效",
  missing: "缺失",
  corrupt: "损坏",
  quarantined: "已隔离",
};

const columns: ColumnDef<AssetRecord>[] = [
  {
    id: "thumbnail",
    header: "预览",
    size: 70,
    enableSorting: false,
    cell: ({ row }) => {
      const asset = row.original;
      const workspaceDir = managedFilesDir.value;
      const filePath = `${workspaceDir}\\${asset.relativePath.replace(/\//g, "\\")}`;

      if (asset.assetKind === "image") {
        let src = "";
        try {
          src = convertFileSrc(filePath);
        } catch {
          /* ignore */
        }
        return h("div", { class: "asset-thumb-wrapper" }, [
          h("img", {
            src,
            alt: asset.displayName,
            class: "asset-thumb",
            onError: () => {
              /* ignore */
            },
          }),
          // 悬浮覆盖层：名称 + 操作按钮
          h("div", { class: "asset-thumb-overlay" }, [
            h("span", { class: "asset-thumb-name" }, asset.displayName),
            h("div", { class: "asset-thumb-actions" }, [
              h(
                "button",
                {
                  class: "asset-thumb-btn",
                  type: "button",
                  title: "打开文件",
                  onClick: (e: Event) => {
                    e.stopPropagation();
                    void openAssetFile(asset.id);
                  },
                },
                h(ExternalLink, { size: 14 }),
              ),
              h(
                "button",
                {
                  class: "asset-thumb-btn",
                  type: "button",
                  title: "打开文件所在目录",
                  onClick: (e: Event) => {
                    e.stopPropagation();
                    void openAssetFolder(asset.id);
                  },
                },
                h(FolderOpen, { size: 14 }),
              ),
            ]),
          ]),
        ]);
      }
      // 非图片资产
      return h(
        "div",
        {
          class: "asset-thumb-wrapper",
        },
        [
          h("div", { class: "asset-icon-placeholder" }, h(Images, { size: 18 })),
          h("div", { class: "asset-thumb-overlay" }, [
            h("span", { class: "asset-thumb-name" }, asset.displayName),
            h("div", { class: "asset-thumb-actions" }, [
              h(
                "button",
                {
                  class: "asset-thumb-btn",
                  type: "button",
                  title: "打开文件",
                  onClick: (e: Event) => {
                    e.stopPropagation();
                    void openAssetFile(asset.id);
                  },
                },
                h(ExternalLink, { size: 14 }),
              ),
              h(
                "button",
                {
                  class: "asset-thumb-btn",
                  type: "button",
                  title: "打开文件所在目录",
                  onClick: (e: Event) => {
                    e.stopPropagation();
                    void openAssetFolder(asset.id);
                  },
                },
                h(FolderOpen, { size: 14 }),
              ),
            ]),
          ]),
        ],
      );
    },
  },
  { accessorKey: "displayName", header: "名称", size: 200 },
  {
    accessorKey: "assetKind",
    header: "类型",
    size: 90,
    cell: ({ row }) => kindLabels[row.original.assetKind],
  },
  {
    accessorKey: "sizeBytes",
    header: "大小",
    size: 90,
    cell: ({ row }) => formatBytes(row.original.sizeBytes),
  },
  {
    accessorKey: "storageNamespace",
    header: "命名空间",
    size: 110,
    cell: ({ row }) => namespaceLabels[row.original.storageNamespace],
  },
  {
    accessorKey: "integrityStatus",
    header: "完整性",
    size: 90,
    cell: ({ row }) => statusLabels[row.original.integrityStatus],
  },
  {
    accessorKey: "updatedAt",
    header: "更新时间",
    size: 170,
    cell: ({ row }) => row.original.updatedAt.replace("T", " ").replace(/\.\d+Z$/, ""),
  },
  {
    id: "actions",
    header: "操作",
    size: 110,
    enableSorting: false,
    cell: ({ row }) => {
      const id = row.original.id;
      const disabled = row.original.integrityStatus === "missing";
      return h("div", { class: "row-actions" }, [
        h(
          "button",
          {
            class: "row-action",
            type: "button",
            title: "打开文件",
            disabled,
            onClick: (event: Event) => {
              event.stopPropagation();
              void openAssetFile(id);
            },
          },
          h(ExternalLink, { size: 14 }),
        ),
        h(
          "button",
          {
            class: "row-action",
            type: "button",
            title: "打开所在目录",
            disabled,
            onClick: (event: Event) => {
              event.stopPropagation();
              void openAssetFolder(id);
            },
          },
          h(FolderOpen, { size: 14 }),
        ),
      ]);
    },
  },
];

// ── 受管文件根目录（由后端提供，用于构造资产完整路径） ──
const managedFilesDir = ref("");

watch(
  () => workspace.isReady,
  (ready) => {
    if (ready && !loaded.value) {
      loaded.value = true;
      void management.load();
      void getWorkspaceRootPath()
        .then((root) => {
          managedFilesDir.value = root.managedFilesDir;
        })
        .catch(() => {
          /* 路径获取失败时预览退化为占位图标 */
        });
    } else if (!ready) {
      loaded.value = false;
    }
  },
  { immediate: true },
);

function applyFilter(): void {
  void management.load();
}

function resetFilters(): void {
  search.value = "";
  storageNamespace.value = undefined;
  assetKind.value = undefined;
  integrityStatus.value = undefined;
  void management.load();
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

// ── 媒体资产分组（图片 + 视频统一网格展示） ──
const mediaAssets = computed(() =>
  assets.value.filter((a) => a.assetKind === "image" || a.assetKind === "video"),
);
const nonMediaAssets = computed(() =>
  assets.value.filter((a) => a.assetKind !== "image" && a.assetKind !== "video"),
);

/** 将资产的 relativePath 转为本地文件 URL */
function getAssetSrc(asset: { relativePath: string }): string {
  // relativePath: "assets/xxx.png" → 完整路径: workspace/managed-files/assets/xxx.png
  const workspaceDir = managedFilesDir.value;
  const filePath = `${workspaceDir}\\${asset.relativePath.replace(/\//g, "\\")}`;
  try {
    return convertFileSrc(filePath);
  } catch {
    return "";
  }
}
</script>

<template>
  <div class="assets-page">
    <template v-if="workspace.errorMessage">
      <EmptyState
        :description="workspace.errorMessage"
        :icon="AlertCircle"
        title="工作空间暂时不可用"
      >
        <button class="secondary-button" type="button" @click="workspace.ensureReady()">
          <RefreshCw :size="17" />
          <span>重新检查</span>
        </button>
      </EmptyState>
    </template>

    <template v-else-if="!workspace.isReady">
      <div class="loading-state">
        <LoaderCircle class="is-spinning" :size="20" />
        <span>正在准备本地工作空间...</span>
      </div>
    </template>

    <template v-else>
      <header class="page-toolbar">
        <form class="search-form" role="search" @submit.prevent="applyFilter()">
          <Search :size="16" aria-hidden="true" />
          <input v-model="search" aria-label="搜索资产" maxlength="80" placeholder="搜索资产名称" />
          <button type="submit" :disabled="isBusy" title="搜索资产">搜索</button>
        </form>
        <div class="toolbar-actions">
          <button
            class="secondary-button"
            type="button"
            title="扫描受管文件并更新完整性状态"
            :disabled="isBusy"
            @click="management.reverify()"
          >
            <ShieldCheck :size="17" :class="{ 'is-spinning': reverifying }" />
            <span>{{ reverifying ? "复检中..." : "完整性复检" }}</span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="刷新"
            aria-label="刷新资产列表"
            :disabled="isBusy"
            @click="management.load()"
          >
            <RefreshCw :size="17" :class="{ 'is-spinning': phase === 'loading' }" />
          </button>
          <button
            class="primary-button"
            type="button"
            :disabled="isBusy"
            @click="importOpen = true"
          >
            <FileUp :size="17" />
            <span>导入文件</span>
          </button>
        </div>
      </header>

      <section class="filter-bar" aria-label="资产筛选">
        <label class="filter-field">
          <span>类型</span>
          <select v-model="assetKind" :disabled="isBusy" @change="applyFilter()">
            <option :value="undefined">全部</option>
            <option value="image">图片</option>
            <option value="video">视频</option>
            <option value="audio">音频</option>
            <option value="document">文档</option>
            <option value="archive">压缩包</option>
            <option value="other">其他</option>
          </select>
        </label>
        <label class="filter-field">
          <span>完整性</span>
          <select v-model="integrityStatus" :disabled="isBusy" @change="applyFilter()">
            <option :value="undefined">全部</option>
            <option value="valid">有效</option>
            <option value="unverified">未校验</option>
            <option value="missing">缺失</option>
            <option value="corrupt">损坏</option>
            <option value="quarantined">已隔离</option>
          </select>
        </label>
        <button class="text-button" type="button" :disabled="isBusy" @click="resetFilters">
          重置筛选
        </button>
      </section>

      <p v-if="errorMessage" class="page-error" role="alert">
        <AlertCircle :size="16" />
        <span>{{ errorMessage }}</span>
        <button type="button" @click="management.load()">重试</button>
      </p>

      <p v-if="reverificationResult" class="reverify-result" role="status">
        <ShieldCheck :size="16" />
        <span>
          复检完成：共 {{ reverificationResult.checked }} 项，有效
          {{ reverificationResult.valid }}，缺失 {{ reverificationResult.missing }}，损坏
          {{ reverificationResult.corrupt
          }}<template v-if="reverificationResult.quarantined > 0">
            ，已隔离 {{ reverificationResult.quarantined }}</template
          >
        </span>
      </p>

      <section class="summary-bar" aria-label="资产概览">
        <div class="summary-item">
          <span class="summary-label">资产总数</span>
          <strong>{{ summary.total }}</strong>
        </div>
        <div class="summary-item">
          <span class="summary-label">占用空间</span>
          <strong>{{ formatBytes(summary.totalBytes) }}</strong>
        </div>
        <div v-if="summary.missing > 0" class="summary-item summary-warn">
          <span class="summary-label">缺失</span>
          <strong>{{ summary.missing }}</strong>
        </div>
        <div v-if="summary.corrupt > 0" class="summary-item summary-warn">
          <span class="summary-label">损坏</span>
          <strong>{{ summary.corrupt }}</strong>
        </div>
      </section>

      <div v-if="phase === 'loading' && assets.length === 0" class="loading-state">
        <LoaderCircle class="is-spinning" :size="20" />
        <span>正在读取资产清单...</span>
      </div>

      <EmptyState
        v-else-if="assets.length === 0 && !errorMessage"
        compact
        description="点击右上角导入按钮，将本机文件导入受管目录。"
        :icon="Images"
        title="暂无资产"
      />

      <!-- 媒体资产网格视图（图片 + 视频） -->
      <div
        v-else-if="
          assetKind === 'image' || assetKind === 'video' || (!assetKind && mediaAssets.length > 0)
        "
        class="image-grid"
      >
        <div v-for="asset in mediaAssets" :key="asset.id" class="image-card">
          <video
            v-if="asset.assetKind === 'video'"
            :src="getAssetSrc(asset)"
            class="image-card__img"
            preload="metadata"
            muted
            @mouseenter="($event.target as HTMLVideoElement).play()"
            @mouseleave="
              ($event.target as HTMLVideoElement).pause();
              ($event.target as HTMLVideoElement).currentTime = 0;
            "
          />
          <img
            v-else
            :src="getAssetSrc(asset)"
            :alt="asset.displayName"
            class="image-card__img"
            loading="lazy"
          />
          <!-- 视频播放标识 -->
          <div v-if="asset.assetKind === 'video'" class="image-card__play-badge">
            <Play :size="16" />
          </div>
          <div class="image-card__overlay">
            <span class="image-card__name">{{ asset.displayName }}</span>
            <div class="image-card__actions">
              <button
                class="image-card__btn"
                type="button"
                title="打开文件"
                @click="openAssetFile(asset.id)"
              >
                <ExternalLink :size="14" />
              </button>
              <button
                class="image-card__btn"
                type="button"
                title="打开文件所在目录"
                @click="openAssetFolder(asset.id)"
              >
                <FolderOpen :size="14" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 非媒体资产：表格视图 -->
      <TeachingDataTable
        v-else
        :columns="columns"
        :data="nonMediaAssets"
        empty-text="没有符合条件的资产。"
        :get-row-id="(row) => row.id"
        :row-label="(row) => row.displayName"
      />
    </template>

    <ImportAssetsDialog
      :open="importOpen"
      :namespace="storageNamespace ?? 'workspace'"
      @close="importOpen = false"
      @imported="management.load()"
    />
  </div>
</template>

<style scoped>
.assets-page {
  width: 100%;
  max-width: 1440px;
  min-height: 100%;
}

.page-toolbar,
.toolbar-actions,
.search-form,
.primary-button,
.secondary-button,
.icon-button,
.page-error,
.loading-state {
  display: flex;
  align-items: center;
}

.page-toolbar {
  min-height: 52px;
  padding-bottom: var(--space-4);
  justify-content: space-between;
  gap: var(--space-4);
  border-bottom: 1px solid var(--color-border-subtle);
}

.search-form {
  width: min(420px, 100%);
  height: var(--control-height);
  padding-left: var(--space-3);
  gap: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
}

.search-form input {
  min-width: 0;
  height: 100%;
  padding: 0;
  flex: 1;
  color: var(--color-text);
  border: 0;
  outline: 0;
  background: transparent;
}

.search-form button {
  height: 100%;
  padding: 0 var(--space-3);
  color: var(--color-accent);
  border: 0;
  border-left: 1px solid var(--color-border-subtle);
  background: transparent;
  cursor: pointer;
}

.toolbar-actions {
  gap: var(--space-2);
}

.primary-button,
.secondary-button {
  min-height: var(--control-height);
  padding: 0 var(--space-3);
  justify-content: center;
  gap: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.primary-button {
  color: var(--color-on-accent);
  border-color: var(--color-accent);
  background: var(--color-accent);
}

.secondary-button {
  color: var(--color-text);
  background: var(--color-surface);
}

.icon-button {
  width: var(--control-height);
  height: var(--control-height);
  padding: 0;
  justify-content: center;
  color: var(--color-text-secondary);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  cursor: pointer;
}

.text-button {
  padding: 0;
  color: var(--color-accent);
  border: 0;
  background: transparent;
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.filter-bar {
  display: flex;
  flex-wrap: wrap;
  padding: var(--space-4) 0;
  align-items: flex-end;
  gap: var(--space-4);
  border-bottom: 1px solid var(--color-border-subtle);
}

.filter-field {
  display: grid;
  gap: var(--space-1);
  font-size: 12px;
  color: var(--color-text-secondary);
}

.filter-field select {
  min-width: 120px;
  height: var(--control-height);
  padding: 0 var(--space-2);
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.summary-bar {
  display: flex;
  padding: var(--space-4) 0;
  gap: var(--space-6);
  border-bottom: 1px solid var(--color-border-subtle);
}

.summary-item {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.summary-label {
  color: var(--color-text-secondary);
  font-size: 12px;
}

.summary-item strong {
  font-size: 18px;
  font-weight: 600;
}

.summary-warn strong {
  color: var(--color-danger);
}

.page-error {
  min-height: 44px;
  margin: var(--space-4) 0 0;
  padding: 0 var(--space-3);
  gap: var(--space-2);
  color: var(--color-danger);
  border: 1px solid var(--color-danger);
  border-radius: var(--radius-control);
  font-size: 13px;
}

.page-error button {
  margin-left: auto;
  color: inherit;
  border: 0;
  background: transparent;
  cursor: pointer;
}

.reverify-result {
  display: flex;
  min-height: 40px;
  margin: var(--space-4) 0 0;
  padding: 0 var(--space-3);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-success, var(--color-text));
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font-size: 13px;
}

.loading-state {
  min-height: 140px;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  font-size: 13px;
}

.row-actions {
  display: flex;
  gap: var(--space-1);
  align-items: center;
}

/* ── 资产缩略图 + 悬浮覆盖层 ── */
.asset-thumb-wrapper {
  position: relative;
  width: 48px;
  height: 48px;
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
}

.asset-thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 200ms ease;
}

.asset-thumb-wrapper:hover .asset-thumb {
  transform: scale(1.05);
}

.asset-icon-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-surface-subtle);
  color: var(--color-text-tertiary);
}

.asset-thumb-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  background: rgba(0, 0, 0, 0.65);
  opacity: 0;
  transition: opacity 200ms ease;
  pointer-events: none;
}

.asset-thumb-wrapper:hover .asset-thumb-overlay {
  opacity: 1;
  pointer-events: auto;
}

.asset-thumb-name {
  font-size: 10px;
  color: #fff;
  text-align: center;
  padding: 0 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 46px;
}

.asset-thumb-actions {
  display: flex;
  gap: 4px;
}

.asset-thumb-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
  cursor: pointer;
  padding: 0;
}

.asset-thumb-btn:hover {
  background: rgba(255, 255, 255, 0.35);
}

.row-action {
  display: inline-grid;
  width: 28px;
  height: 28px;
  padding: 0;
  place-items: center;
  color: var(--color-text-secondary);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  cursor: pointer;
}

.row-action:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.row-action:disabled {
  cursor: not-allowed;
  opacity: 0.4;
}

.is-spinning {
  animation: spin 900ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 640px) {
  .page-toolbar {
    align-items: stretch;
    flex-direction: column;
  }

  .search-form {
    width: 100%;
  }

  .toolbar-actions {
    justify-content: flex-end;
  }

  .summary-bar {
    flex-wrap: wrap;
    gap: var(--space-4);
  }
}

/* ── 图片网格视图 ── */
.image-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  padding: 4px 0;
}

.image-card {
  position: relative;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid var(--color-border-subtle);
  background: var(--color-surface);
  cursor: pointer;
  transition:
    border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}

.image-card:hover {
  border-color: var(--color-border);
  box-shadow: var(--shadow-md);
}

.image-card__img {
  width: 100%;
  aspect-ratio: 1;
  object-fit: cover;
  display: block;
}

.image-card__play-badge {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  pointer-events: none;
  transition: opacity 200ms ease;
}

.image-card:hover .image-card__play-badge {
  opacity: 0;
}

.image-card__overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  padding: 8px;
  background: linear-gradient(to top, rgba(0, 0, 0, 0.7) 0%, transparent 50%);
  opacity: 0;
  transition: opacity 200ms ease;
}

.image-card:hover .image-card__overlay {
  opacity: 1;
}

.image-card__name {
  font-size: 11px;
  color: #fff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: 6px;
}

.image-card__actions {
  display: flex;
  gap: 6px;
}

.image-card__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
  cursor: pointer;
  padding: 0;
  transition: background 120ms ease;
}

.image-card__btn:hover {
  background: rgba(255, 255, 255, 0.35);
}
</style>
