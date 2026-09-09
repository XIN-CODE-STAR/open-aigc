<script setup lang="ts">
/**
 * ToolConfirmationDialog：工具确认对话框。
 *
 * 高成本操作前的确认弹窗，展示工具信息、副作用和确认/取消按钮。
 */
import { AlertTriangle, CheckCircle, X } from "@lucide/vue";

defineProps<{
  visible: boolean;
  toolName: string;
  toolDescription: string;
  sideEffects: string[];
  parameters?: Record<string, unknown>;
}>();

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();
</script>

<template>
  <Transition name="dialog">
    <div v-if="visible" class="dialog-overlay" @click.self="emit('cancel')">
      <div class="dialog">
        <div class="dialog-header">
          <AlertTriangle :size="20" class="dialog-warning-icon" />
          <h3 class="dialog-title">确认操作</h3>
          <button type="button" class="dialog-close" @click="emit('cancel')">
            <X :size="16" />
          </button>
        </div>

        <div class="dialog-body">
          <p class="dialog-tool-name">{{ toolName }}</p>
          <p class="dialog-description">{{ toolDescription }}</p>

          <div v-if="sideEffects.length > 0" class="dialog-effects">
            <p class="dialog-effects-label">此操作将：</p>
            <ul class="dialog-effects-list">
              <li v-for="effect in sideEffects" :key="effect" class="dialog-effect">
                <span class="dialog-effect-dot">•</span>
                {{ effect }}
              </li>
            </ul>
          </div>

          <div v-if="parameters && Object.keys(parameters).length > 0" class="dialog-params">
            <p class="dialog-params-label">参数：</p>
            <div class="dialog-params-grid">
              <div v-for="(value, key) in parameters" :key="key" class="dialog-param">
                <span class="dialog-param-key">{{ key }}:</span>
                <span class="dialog-param-value">{{ String(value) }}</span>
              </div>
            </div>
          </div>
        </div>

        <div class="dialog-footer">
          <button type="button" class="dialog-btn dialog-btn--cancel" @click="emit('cancel')">
            取消
          </button>
          <button type="button" class="dialog-btn dialog-btn--confirm" @click="emit('confirm')">
            <CheckCircle :size="14" />
            确认执行
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
  -webkit-backdrop-filter: blur(4px);
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
  gap: 10px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--color-border-subtle);
}

.dialog-warning-icon {
  color: var(--color-warning);
}

.dialog-title {
  flex: 1;
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text);
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
  transition: all var(--duration-fast) var(--ease-out);
}

.dialog-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.dialog-body {
  padding: 20px;
}

.dialog-tool-name {
  margin: 0 0 8px;
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
}

.dialog-description {
  margin: 0 0 16px;
  font-size: 14px;
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.dialog-effects {
  margin-bottom: 16px;
}

.dialog-effects-label {
  margin: 0 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.dialog-effects-list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.dialog-effect {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 13px;
  color: var(--color-warning);
}

.dialog-effect-dot {
  flex-shrink: 0;
  width: 12px;
  text-align: center;
}

.dialog-params {
  margin-top: 16px;
}

.dialog-params-label {
  margin: 0 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.dialog-params-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.dialog-param {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: var(--radius-pill);
  background: var(--color-surface-subtle);
  font-size: 12px;
}

.dialog-param-key {
  color: var(--color-text-tertiary);
  font-weight: 500;
}

.dialog-param-value {
  color: var(--color-text);
}

.dialog-footer {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
  padding: 16px 20px;
  border-top: 1px solid var(--color-border-subtle);
}

.dialog-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border: none;
  border-radius: var(--radius-control);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.dialog-btn--cancel {
  background: var(--color-surface-subtle);
  color: var(--color-text-secondary);
}

.dialog-btn--cancel:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.dialog-btn--confirm {
  background: var(--color-accent);
  color: var(--color-on-accent);
}

.dialog-btn--confirm:hover {
  opacity: 0.9;
}

/* 过渡动画 */
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
