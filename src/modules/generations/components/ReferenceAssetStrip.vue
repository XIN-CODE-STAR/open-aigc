<script setup lang="ts">
/**
 * ReferenceAssetStrip：已上传参考图/附件横向预览条。
 *
 * 在 PromptComposer 内部使用，展示当前已加入的参考文件名，
 * 支持单条移除。
 */
import { FileText, Image as ImageIcon, X } from "@lucide/vue";

defineProps<{
  /** 参考图/附件列表。 */
  referenceImages: { name: string; mimeType: string }[];
}>();

const emit = defineEmits<{
  remove: [index: number];
}>();

function onRemove(index: number): void {
  emit("remove", index);
}

function isImage(mimeType: string): boolean {
  return mimeType.startsWith("image/");
}
</script>

<template>
  <Transition name="ref-strip">
    <div v-if="referenceImages.length > 0" class="ref-strip">
      <div v-for="(img, idx) in referenceImages" :key="img.name" class="ref-thumb">
        <ImageIcon v-if="isImage(img.mimeType)" :size="14" />
        <FileText v-else :size="14" />
        <span class="ref-name" :title="img.name">{{ img.name }}</span>
        <button
          type="button"
          class="ref-remove"
          :title="`移除附件 ${idx + 1}`"
          @click="onRemove(idx)"
        >
          <X :size="9" />
        </button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.ref-strip {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px 4px 0;
  min-width: 0;
  overflow: hidden;
}

.ref-thumb {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 4px 3px 8px;
  border-radius: 6px;
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
  font-size: 11px;
  color: var(--color-text-secondary);
  max-width: min(220px, 100%);
  min-width: 0;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.ref-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: min(140px, 60vw);
  min-width: 0;
  flex-shrink: 1;
}

.ref-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.ref-remove:hover {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.ref-strip-enter-active,
.ref-strip-leave-active {
  transition:
    opacity 180ms ease,
    max-height 180ms ease;
  overflow: hidden;
}

.ref-strip-enter-from,
.ref-strip-leave-to {
  opacity: 0;
  max-height: 0;
}
</style>
