<script setup lang="ts">
import { computed, ref } from "vue";
import {
  AlertCircle,
  Archive,
  ArchiveRestore,
  CheckCircle2,
  FileDown,
  FileUp,
  Info,
  LoaderCircle,
  ShieldAlert,
} from "@lucide/vue";

import { useWorkspaceStore } from "../../../app/stores/workspace";
import EmptyState from "../../../shared/ui/EmptyState.vue";
import ModalDialog from "../../../shared/ui/ModalDialog.vue";
import {
  createBackup,
  pickBackupRestorePath,
  pickBackupSavePath,
  previewRestore,
  restoreBackup,
  type BackupSummary,
  type RestorePreview,
  type RestoreSummary,
} from "../../../bridge/backup";

const workspace = useWorkspaceStore();

// 创建备份状态
const createNote = ref("");
const createArchivePath = ref("");
const createBusy = ref(false);
const createResult = ref<BackupSummary | null>(null);
const createError = ref<string | null>(null);

// 恢复备份状态
const restoreArchivePath = ref("");
const restorePreviewData = ref<RestorePreview | null>(null);
const restorePreviewBusy = ref(false);
const restoreBusy = ref(false);
const restoreResult = ref<RestoreSummary | null>(null);
const restoreError = ref<string | null>(null);
const restoreConfirmOpen = ref(false);

const canCreate = computed(() => createArchivePath.value.trim() !== "" && !createBusy.value);
const canPreview = computed(
  () => restoreArchivePath.value.trim() !== "" && !restorePreviewBusy.value,
);
const canRestore = computed(() => restorePreviewData.value !== null && !restoreBusy.value);

async function pickCreatePath(): Promise<void> {
  createError.value = null;
  try {
    const path = await pickBackupSavePath();
    if (path) {
      createArchivePath.value = path;
      createResult.value = null;
    }
  } catch (error) {
    createError.value = error instanceof Error ? error.message : "无法打开保存对话框。";
  }
}

async function pickRestorePath(): Promise<void> {
  restoreError.value = null;
  try {
    const path = await pickBackupRestorePath();
    if (path) {
      restoreArchivePath.value = path;
      restorePreviewData.value = null;
      restoreResult.value = null;
    }
  } catch (error) {
    restoreError.value = error instanceof Error ? error.message : "无法打开文件对话框。";
  }
}

async function handleCreate(): Promise<void> {
  if (!canCreate.value) return;
  createBusy.value = true;
  createError.value = null;
  try {
    createResult.value = await createBackup({
      archivePath: createArchivePath.value,
      note: createNote.value.trim() || undefined,
    });
  } catch (error) {
    createError.value = error instanceof Error ? error.message : "创建备份失败，请稍后重试。";
  } finally {
    createBusy.value = false;
  }
}

async function handlePreview(): Promise<void> {
  if (!canPreview.value) return;
  restorePreviewBusy.value = true;
  restoreError.value = null;
  try {
    restorePreviewData.value = await previewRestore(restoreArchivePath.value);
  } catch (error) {
    restorePreviewData.value = null;
    restoreError.value = error instanceof Error ? error.message : "无法读取备份归档。";
  } finally {
    restorePreviewBusy.value = false;
  }
}

function openRestoreConfirm(): void {
  if (!canRestore.value) return;
  restoreConfirmOpen.value = true;
}

async function confirmRestore(): Promise<void> {
  if (!restorePreviewData.value) return;
  restoreBusy.value = true;
  restoreError.value = null;
  try {
    restoreResult.value = await restoreBackup(restoreArchivePath.value);
    restoreConfirmOpen.value = false;
  } catch (error) {
    restoreError.value = error instanceof Error ? error.message : "恢复备份失败，请稍后重试。";
  } finally {
    restoreBusy.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatTimestamp(iso: string): string {
  return iso.replace("T", " ").replace(/\.\d+Z$/, "");
}
</script>

<template>
  <div class="backup-page">
    <template v-if="workspace.errorMessage">
      <EmptyState
        :description="workspace.errorMessage"
        :icon="AlertCircle"
        title="工作空间暂时不可用"
      />
    </template>

    <template v-else-if="!workspace.isReady">
      <div class="loading-state">
        <LoaderCircle class="is-spinning" :size="20" />
        <span>正在准备本地工作空间...</span>
      </div>
    </template>

    <template v-else>
      <p class="backup-page__description">
        将 SQLite 数据库快照、资产 manifest
        与受管文件打包为完整工作区备份。恢复前会自动创建安全备份。
      </p>

      <!-- 创建备份 -->
      <section class="settings-section" aria-labelledby="create-title">
        <div class="setting-copy">
          <h2 id="create-title">创建备份</h2>
          <p>把当前工作区的数据库与受管文件打包到一个 zip 归档。</p>
        </div>
        <div class="backup-panel">
          <label class="field">
            <span>备注（可选）</span>
            <textarea
              v-model="createNote"
              maxlength="500"
              placeholder="如：期末教学归档"
              rows="2"
            ></textarea>
          </label>

          <div class="path-picker">
            <input
              class="path-input"
              :value="createArchivePath || '未选择保存位置'"
              readonly
              aria-label="备份保存位置"
            />
            <button
              class="secondary-button"
              type="button"
              :disabled="createBusy"
              @click="pickCreatePath"
            >
              <FileDown :size="16" />
              <span>选择位置</span>
            </button>
          </div>

          <p v-if="createError" class="panel-error" role="alert">
            <AlertCircle :size="16" />
            <span>{{ createError }}</span>
          </p>

          <div class="panel-actions">
            <button
              class="primary-button"
              type="button"
              :disabled="!canCreate"
              @click="handleCreate"
            >
              <LoaderCircle v-if="createBusy" class="is-spinning" :size="16" />
              <Archive v-else :size="16" />
              <span>{{ createBusy ? "创建中..." : "创建备份" }}</span>
            </button>
          </div>

          <p v-if="createResult" class="panel-success" role="status">
            <CheckCircle2 :size="16" />
            <span>
              备份已创建：{{ formatBytes(createResult.archiveSize) }}（{{
                createResult.managedFileCount
              }}
              个受管文件，{{ formatTimestamp(createResult.createdAt) }}）
            </span>
          </p>
        </div>
      </section>

      <!-- 恢复备份 -->
      <section class="settings-section" aria-labelledby="restore-title">
        <div class="setting-copy">
          <h2 id="restore-title">恢复备份</h2>
          <p>从备份归档恢复工作区。恢复前会自动创建当前状态的安全备份。</p>
        </div>
        <div class="backup-panel">
          <div class="path-picker">
            <input
              class="path-input"
              :value="restoreArchivePath || '未选择备份归档'"
              readonly
              aria-label="备份归档路径"
            />
            <button
              class="secondary-button"
              type="button"
              :disabled="restorePreviewBusy || restoreBusy"
              @click="pickRestorePath"
            >
              <FileUp :size="16" />
              <span>选择归档</span>
            </button>
          </div>

          <div class="panel-actions">
            <button
              class="secondary-button"
              type="button"
              :disabled="!canPreview"
              @click="handlePreview"
            >
              <LoaderCircle v-if="restorePreviewBusy" class="is-spinning" :size="16" />
              <Info v-else :size="16" />
              <span>{{ restorePreviewBusy ? "读取中..." : "预览归档" }}</span>
            </button>
            <button
              class="primary-button is-danger"
              type="button"
              :disabled="!canRestore"
              @click="openRestoreConfirm"
            >
              <ArchiveRestore :size="16" />
              <span>恢复工作区</span>
            </button>
          </div>

          <p v-if="restoreError" class="panel-error" role="alert">
            <AlertCircle :size="16" />
            <span>{{ restoreError }}</span>
          </p>

          <div v-if="restorePreviewData" class="manifest-card">
            <div class="manifest-card__header">
              <Info :size="16" />
              <h3>归档内容</h3>
            </div>
            <dl class="info-list">
              <div class="info-row">
                <dt>工作空间</dt>
                <dd>{{ restorePreviewData.manifest.workspaceName }}</dd>
              </div>
              <div class="info-row">
                <dt>教师</dt>
                <dd>{{ restorePreviewData.manifest.teacherName }}</dd>
              </div>
              <div class="info-row">
                <dt>Schema</dt>
                <dd>v{{ restorePreviewData.manifest.schemaVersion }}</dd>
              </div>
              <div class="info-row">
                <dt>资产</dt>
                <dd>
                  {{ restorePreviewData.manifest.assetCount }} 个 ·
                  {{ formatBytes(restorePreviewData.managedTotalBytes) }}
                </dd>
              </div>
              <div class="info-row">
                <dt>备份时间</dt>
                <dd>{{ formatTimestamp(restorePreviewData.manifest.createdAt) }}</dd>
              </div>
              <div v-if="restorePreviewData.manifest.note" class="info-row">
                <dt>备注</dt>
                <dd>{{ restorePreviewData.manifest.note }}</dd>
              </div>
            </dl>
          </div>

          <div v-if="restoreResult" class="panel-success" role="status">
            <CheckCircle2 :size="16" />
            <div class="restore-result-text">
              <p>
                恢复完成：{{ restoreResult.restoredFileCount }} 个文件，
                {{ formatBytes(restoreResult.restoredTotalBytes) }}。
              </p>
              <p v-if="restoreResult.safetyBackupPath" class="restore-result-hint">
                安全备份：{{ restoreResult.safetyBackupPath }}
              </p>
              <p v-if="restoreResult.requiresRestart" class="restore-result-hint">
                请重启应用以完成恢复。
              </p>
            </div>
          </div>
        </div>
      </section>
    </template>

    <!-- 恢复确认对话框 -->
    <ModalDialog
      :open="restoreConfirmOpen"
      :busy="restoreBusy"
      title="确认恢复工作区"
      description="此操作将用备份内容覆盖当前工作区数据。"
      width="wide"
      @close="!restoreBusy ? (restoreConfirmOpen = false) : undefined"
    >
      <div class="confirm-body">
        <div class="confirm-warning">
          <ShieldAlert :size="20" />
          <div>
            <p class="confirm-warning__title">恢复将覆盖当前数据</p>
            <p class="confirm-warning__text">
              恢复前会自动创建当前工作区的安全备份。恢复完成后可能需要重启应用。
            </p>
          </div>
        </div>
        <dl v-if="restorePreviewData" class="info-list">
          <div class="info-row">
            <dt>备份来源</dt>
            <dd>{{ restorePreviewData.manifest.workspaceName }}</dd>
          </div>
          <div class="info-row">
            <dt>备份时间</dt>
            <dd>{{ formatTimestamp(restorePreviewData.manifest.createdAt) }}</dd>
          </div>
        </dl>
      </div>
      <template #footer>
        <button
          class="secondary-button"
          type="button"
          :disabled="restoreBusy"
          @click="restoreConfirmOpen = false"
        >
          取消
        </button>
        <button
          class="primary-button is-danger"
          type="button"
          :disabled="restoreBusy"
          @click="confirmRestore"
        >
          <LoaderCircle v-if="restoreBusy" class="is-spinning" :size="16" />
          <ArchiveRestore v-else :size="16" />
          <span>{{ restoreBusy ? "恢复中..." : "确认恢复" }}</span>
        </button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.backup-page {
  max-width: 920px;
}

.backup-page__description {
  margin: 0;
  padding-bottom: var(--space-5);
  color: var(--color-text-secondary);
  border-bottom: 1px solid var(--color-border-subtle);
  font-size: 14px;
  line-height: 20px;
}

.settings-section {
  display: grid;
  min-height: 96px;
  padding: var(--space-5) 0;
  align-items: start;
  grid-template-columns: minmax(220px, 1fr) minmax(360px, auto);
  gap: var(--space-6);
  border-bottom: 1px solid var(--color-border-subtle);
}

.setting-copy h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  line-height: 24px;
}

.setting-copy p {
  margin: var(--space-1) 0 0;
  color: var(--color-text-secondary);
  font-size: 13px;
  line-height: 20px;
}

.backup-panel {
  display: grid;
  gap: var(--space-3);
}

.info-list {
  display: grid;
  margin: 0;
  gap: var(--space-2);
}

.info-row {
  display: grid;
  grid-template-columns: 88px minmax(0, 1fr);
  gap: var(--space-3);
  align-items: baseline;
}

.info-row dt {
  color: var(--color-text-secondary);
  font-size: 12px;
  line-height: 18px;
}

.info-row dd {
  margin: 0;
  color: var(--color-text);
  font-size: 13px;
  line-height: 18px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.field {
  display: grid;
  gap: var(--space-1);
  font-size: 12px;
  color: var(--color-text-secondary);
}

.field textarea {
  min-height: 56px;
  padding: var(--space-2) var(--space-3);
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  font: inherit;
  font-size: 13px;
  line-height: 20px;
  resize: vertical;
}

.field textarea:focus-visible {
  outline: 2px solid var(--color-focus);
  outline-offset: 1px;
}

.path-picker {
  display: flex;
  gap: var(--space-2);
  align-items: stretch;
}

.path-input {
  min-width: 0;
  flex: 1;
  height: var(--control-height);
  padding: 0 var(--space-3);
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font: inherit;
  font-size: 12px;
  line-height: 20px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.primary-button,
.secondary-button {
  display: inline-flex;
  min-height: var(--control-height);
  padding: 0 var(--space-3);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
  white-space: nowrap;
}

.primary-button {
  color: var(--color-on-accent);
  border-color: var(--color-accent);
  background: var(--color-accent);
}

.primary-button.is-danger {
  border-color: var(--color-danger);
  background: var(--color-danger);
}

.secondary-button {
  color: var(--color-text);
}

.primary-button:hover:not(:disabled),
.secondary-button:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.primary-button:hover:not(:disabled) {
  background: var(--color-accent);
  filter: brightness(0.96);
}

.primary-button.is-danger:hover:not(:disabled) {
  background: var(--color-danger);
  filter: brightness(0.96);
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.panel-actions {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.panel-error {
  display: flex;
  margin: 0;
  padding: var(--space-2) var(--space-3);
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--color-danger);
  border: 1px solid var(--color-danger);
  border-radius: var(--radius-control);
  font-size: 13px;
  line-height: 20px;
}

.panel-error svg {
  flex: 0 0 16px;
  margin-top: 2px;
}

.panel-success {
  display: flex;
  margin: 0;
  padding: var(--space-2) var(--space-3);
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--color-success, var(--color-text));
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font-size: 13px;
  line-height: 20px;
}

.panel-success svg {
  flex: 0 0 16px;
  margin-top: 2px;
}

.restore-result-text {
  display: grid;
  gap: var(--space-1);
}

.restore-result-text p {
  margin: 0;
}

.restore-result-hint {
  color: var(--color-text-secondary);
  font-size: 12px;
}

.manifest-card {
  padding: var(--space-3);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.manifest-card__header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
  color: var(--color-text-secondary);
}

.manifest-card__header h3 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  line-height: 18px;
}

.loading-state {
  display: flex;
  min-height: 140px;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  font-size: 13px;
}

.confirm-body {
  display: grid;
  padding: var(--space-4);
  gap: var(--space-4);
}

.confirm-warning {
  display: flex;
  gap: var(--space-3);
  align-items: flex-start;
  padding: var(--space-3);
  border: 1px solid var(--color-warning, var(--color-danger));
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  color: var(--color-warning, var(--color-danger));
}

.confirm-warning__title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 20px;
}

.confirm-warning__text {
  margin: var(--space-1) 0 0;
  font-size: 13px;
  line-height: 20px;
  color: var(--color-text-secondary);
}

.is-spinning {
  animation: spin 900ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 1100px) {
  .settings-section {
    grid-template-columns: 1fr;
    gap: var(--space-3);
  }
}

@media (max-width: 680px) {
  .path-picker {
    flex-direction: column;
  }

  .path-input {
    width: 100%;
  }

  .panel-actions {
    flex-direction: column;
  }

  .primary-button,
  .secondary-button {
    width: 100%;
  }
}
</style>
