<script setup lang="ts">
import { ref, watch } from "vue";
import { AlertCircle, CheckCircle, FileUp, LoaderCircle } from "@lucide/vue";

import {
  importAssets,
  openAssetFileDialog,
  type AssetImportSummary,
  type StorageNamespace,
} from "../../../bridge/assets";
import ModalDialog from "../../../shared/ui/ModalDialog.vue";

const props = defineProps<{
  open: boolean;
  namespace: StorageNamespace;
}>();

const emit = defineEmits<{
  close: [];
  imported: [];
}>();

const busy = ref(false);
const summary = ref<AssetImportSummary | null>(null);
const errorMessage = ref<string>();

watch(
  () => props.open,
  (open) => {
    if (open) {
      summary.value = null;
      errorMessage.value = undefined;
      busy.value = false;
    }
  },
);

async function selectAndImport(): Promise<void> {
  busy.value = true;
  errorMessage.value = undefined;
  try {
    const paths = await openAssetFileDialog({ multiple: true, title: "选择要导入的文件" });
    if (paths.length === 0) {
      busy.value = false;
      return;
    }
    summary.value = await importAssets(paths, props.namespace);
    emit("imported");
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "导入失败，请重试。";
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ModalDialog
    :busy="busy"
    description="导入的文件会被复制到受管目录并计算校验值。"
    :open="open"
    title="导入文件"
    width="wide"
    @close="$emit('close')"
  >
    <div class="import-body">
      <div v-if="!summary && !errorMessage" class="import-hint">
        <FileUp :size="28" />
        <p>点击下方按钮选择本机文件，导入到“{{ props.namespace }}”命名空间。</p>
        <button class="primary-button" type="button" :disabled="busy" @click="selectAndImport">
          <FileUp :size="16" />
          <span>选择文件</span>
        </button>
      </div>

      <div v-if="busy" class="import-progress">
        <LoaderCircle class="is-spinning" :size="20" />
        <span>正在导入文件...</span>
      </div>

      <div v-if="errorMessage" class="import-error" role="alert">
        <AlertCircle :size="18" />
        <span>{{ errorMessage }}</span>
      </div>

      <div v-if="summary" class="import-summary">
        <div class="summary-row">
          <CheckCircle :size="18" class="summary-icon-ok" />
          <span>已导入 {{ summary.imported.length }} 个文件</span>
        </div>
        <div v-if="summary.skipped.length > 0" class="summary-row">
          <AlertCircle :size="18" class="summary-icon-warn" />
          <span>跳过 {{ summary.skipped.length }} 个（重复或冲突）</span>
        </div>
        <div v-if="summary.failures.length > 0" class="summary-row">
          <AlertCircle :size="18" class="summary-icon-err" />
          <span>失败 {{ summary.failures.length }} 个</span>
        </div>

        <details v-if="summary.skipped.length > 0" class="summary-details">
          <summary>跳过的文件</summary>
          <ul>
            <li v-for="item in summary.skipped" :key="item.sourcePath">
              <span class="path">{{ item.sourcePath }}</span>
              <span class="reason">{{ item.reason }}</span>
            </li>
          </ul>
        </details>

        <details v-if="summary.failures.length > 0" class="summary-details">
          <summary>失败的文件</summary>
          <ul>
            <li v-for="item in summary.failures" :key="item.sourcePath">
              <span class="path">{{ item.sourcePath }}</span>
              <span class="reason">{{ item.reason }}</span>
            </li>
          </ul>
        </details>

        <button class="primary-button" type="button" :disabled="busy" @click="selectAndImport">
          <FileUp :size="16" />
          <span>继续导入</span>
        </button>
      </div>
    </div>

    <template #footer>
      <button class="secondary-button" type="button" :disabled="busy" @click="$emit('close')">
        {{ summary ? "完成" : "取消" }}
      </button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.import-body {
  padding: var(--space-4);
}

.import-hint {
  display: grid;
  padding: var(--space-6) var(--space-4);
  gap: var(--space-3);
  justify-items: center;
  text-align: center;
  color: var(--color-text-secondary);
}

.import-hint p {
  margin: 0;
  max-width: 360px;
  font-size: 13px;
  line-height: 20px;
}

.import-progress {
  display: flex;
  padding: var(--space-6) var(--space-4);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  font-size: 13px;
}

.import-error {
  display: flex;
  padding: var(--space-3) var(--space-4);
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--color-danger);
  border: 1px solid var(--color-danger);
  border-radius: var(--radius-control);
  font-size: 13px;
}

.import-error span {
  flex: 1;
}

.import-summary {
  display: grid;
  gap: var(--space-3);
}

.summary-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 13px;
}

.summary-icon-ok {
  color: var(--color-success);
}

.summary-icon-warn {
  color: #b7791f;
}

.summary-icon-err {
  color: var(--color-danger);
}

.summary-details {
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  font-size: 12px;
}

.summary-details summary {
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
  color: var(--color-text-secondary);
}

.summary-details ul {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  list-style: none;
  border-top: 1px solid var(--color-border-subtle);
}

.summary-details li {
  display: grid;
  padding: var(--space-1) 0;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-3);
  line-height: 18px;
}

.summary-details .path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.summary-details .reason {
  color: var(--color-text-secondary);
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

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.is-spinning {
  animation: spin 900ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
