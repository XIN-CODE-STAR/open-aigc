<script setup lang="ts">
/**
 * CreateProjectDialog：创建项目对话框。
 *
 * 输入项目名称、类型、目标时长、目标平台。
 */
import { ref } from "vue";
import { Clapperboard, X } from "@lucide/vue";

defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  create: [data: { title: string; type: string; duration?: number; platform?: string }];
  close: [];
}>();

const title = ref("");
const projectType = ref("promo_video");
const duration = ref(30);
const platform = ref("");

const projectTypes = [
  { id: "promo_video", label: "宣传片" },
  { id: "short_film", label: "短片" },
  { id: "product_video", label: "产品视频" },
  { id: "documentary", label: "纪录片" },
  { id: "social_media", label: "社媒素材" },
  { id: "educational", label: "教学视频" },
];

function onCreate(): void {
  if (!title.value.trim()) return;
  emit("create", {
    title: title.value.trim(),
    type: projectType.value,
    duration: duration.value,
    platform: platform.value || undefined,
  });
  title.value = "";
}
</script>

<template>
  <Transition name="dialog">
    <div v-if="visible" class="dialog-overlay" @click.self="emit('close')">
      <div class="dialog">
        <div class="dialog-header">
          <Clapperboard :size="18" />
          <span class="dialog-title">新建创作项目</span>
          <button type="button" class="dialog-close" @click="emit('close')">
            <X :size="16" />
          </button>
        </div>

        <div class="dialog-body">
          <div class="form-field">
            <label class="form-label">项目名称</label>
            <input
              v-model="title"
              class="form-input"
              type="text"
              placeholder="例如：30秒创业主题宣传片"
              autofocus
              @keydown.enter="onCreate"
            />
          </div>

          <div class="form-field">
            <label class="form-label">项目类型</label>
            <div class="type-grid">
              <button
                v-for="pt in projectTypes"
                :key="pt.id"
                type="button"
                class="type-btn"
                :class="{ 'is-active': projectType === pt.id }"
                @click="projectType = pt.id"
              >
                {{ pt.label }}
              </button>
            </div>
          </div>

          <div class="form-row">
            <div class="form-field form-field--half">
              <label class="form-label">目标时长（秒）</label>
              <input v-model.number="duration" class="form-input" type="number" min="5" max="600" />
            </div>
            <div class="form-field form-field--half">
              <label class="form-label">目标平台</label>
              <input v-model="platform" class="form-input" type="text" placeholder="如：抖音" />
            </div>
          </div>
        </div>

        <div class="dialog-footer">
          <button type="button" class="dialog-btn dialog-btn--secondary" @click="emit('close')">
            取消
          </button>
          <button
            type="button"
            class="dialog-btn dialog-btn--primary"
            :disabled="!title.trim()"
            @click="onCreate"
          >
            创建项目
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
}

.dialog {
  width: 90%;
  max-width: 480px;
  border-radius: var(--radius-dialog);
  background: var(--color-surface);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--color-border-subtle);
  color: var(--color-text);
}

.dialog-title {
  flex: 1;
  font-size: 15px;
  font-weight: 600;
}

.dialog-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
}

.dialog-close:hover {
  background: var(--color-surface-hover);
}

.dialog-body {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-field--half {
  flex: 1;
}

.form-row {
  display: flex;
  gap: 12px;
}

.form-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
}

.form-input {
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-canvas);
  color: var(--color-text);
  font-size: 14px;
  outline: none;
  transition: border-color var(--duration-fast) var(--ease-out);
}

.form-input:focus {
  border-color: var(--color-border-strong);
}

.form-input::placeholder {
  color: var(--color-text-tertiary);
}

.type-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.type-btn {
  padding: 6px 14px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.type-btn:hover {
  border-color: var(--color-border);
  color: var(--color-text);
}

.type-btn.is-active {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.dialog-footer {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
  padding: 16px 20px;
  border-top: 1px solid var(--color-border-subtle);
}

.dialog-btn {
  padding: 8px 16px;
  border: none;
  border-radius: var(--radius-control);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.dialog-btn--secondary {
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
}

.dialog-btn--secondary:hover {
  background: var(--color-surface-hover);
}

.dialog-btn--primary {
  background: var(--color-accent);
  color: var(--color-on-accent);
}

.dialog-btn--primary:hover:not(:disabled) {
  opacity: 0.9;
}

.dialog-btn--primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.dialog-enter-active,
.dialog-leave-active {
  transition: all var(--duration-base) var(--ease-out);
}

.dialog-enter-from,
.dialog-leave-to {
  opacity: 0;
}

.dialog-enter-from .dialog,
.dialog-leave-to .dialog {
  transform: scale(0.95) translateY(10px);
}
</style>
