<script setup lang="ts">
import { nextTick, ref, useId, watch } from "vue";
import { X } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    busy?: boolean;
    description?: string;
    open: boolean;
    title: string;
    width?: "medium" | "wide";
  }>(),
  {
    busy: false,
    description: undefined,
    width: "medium",
  },
);

const emit = defineEmits<{
  close: [];
}>();

const dialog = ref<HTMLElement | null>(null);
const titleId = useId();
const descriptionId = useId();
let returnFocus: HTMLElement | null = null;
const focusableSelector = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

watch(
  () => props.open,
  async (open, wasOpen) => {
    if (open) {
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      await nextTick();
      focusInitialElement();
      return;
    }

    if (wasOpen) {
      await nextTick();
      if (returnFocus?.isConnected) returnFocus.focus();
      returnFocus = null;
    }
  },
  { flush: "post", immediate: true },
);

function requestClose(): void {
  if (!props.busy) emit("close");
}

function focusInitialElement(): void {
  const container = dialog.value;
  if (!container) return;
  const autofocus = container.querySelector<HTMLElement>("[autofocus]");
  const target = autofocus?.matches(focusableSelector)
    ? autofocus
    : container.querySelector<HTMLElement>(focusableSelector);
  (target ?? container).focus();
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    requestClose();
    return;
  }

  if (event.key !== "Tab") return;
  const container = dialog.value;
  if (!container) return;
  const elements = Array.from(container.querySelectorAll<HTMLElement>(focusableSelector));
  if (elements.length === 0) {
    event.preventDefault();
    container.focus();
    return;
  }

  const first = elements[0];
  const last = elements[elements.length - 1];
  const active = document.activeElement;
  if (event.shiftKey && (active === first || !container.contains(active))) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && (active === last || !container.contains(active))) {
    event.preventDefault();
    first?.focus();
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="dialog-backdrop"
      role="presentation"
      @keydown="handleKeydown"
      @mousedown.self="requestClose"
    >
      <section
        ref="dialog"
        class="modal-dialog"
        :class="`modal-dialog--${width}`"
        role="dialog"
        aria-modal="true"
        :aria-describedby="description ? descriptionId : undefined"
        :aria-labelledby="titleId"
        tabindex="-1"
      >
        <header class="dialog-header material-vibrancy">
          <div class="dialog-header__text">
            <h2 :id="titleId" class="dialog-title">{{ title }}</h2>
            <p v-if="description" :id="descriptionId" class="dialog-description">
              {{ description }}
            </p>
          </div>
          <button
            class="close-button"
            type="button"
            aria-label="关闭"
            title="关闭"
            :disabled="busy"
            @click="requestClose"
          >
            <X :size="16" :stroke-width="2.2" />
          </button>
        </header>
        <div class="dialog-content"><slot /></div>
        <footer class="dialog-footer"><slot name="footer" /></footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
/* —— iOS Dimmed Background（apple-design §12：dim to focus）—— */
.dialog-backdrop {
  position: fixed;
  z-index: 1000;
  display: grid;
  inset: 0;
  padding: var(--space-6);
  place-items: center;
  background: var(--color-overlay);
  opacity: 1;
  transition: opacity var(--duration-slow) var(--ease-out);
  @starting-style {
    opacity: 0;
  }
}

/* —— iOS Sheet 风格：20px 大圆角、柔和阴影、入场 scale + opacity —— */
.modal-dialog {
  width: min(520px, 100%);
  max-height: calc(100vh - 48px);
  overflow: hidden auto;
  color: var(--color-text);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-dialog);
  background: var(--color-surface);
  box-shadow: var(--shadow-sheet);
  transform: scale(1);
  opacity: 1;
  transition:
    transform var(--duration-slow) var(--ease-ios),
    opacity var(--duration-slow) var(--ease-out);
  @starting-style {
    transform: scale(0.96);
    opacity: 0;
  }
}

.modal-dialog--wide {
  width: min(720px, 100%);
}

/* —— iOS vibrancy header：半透明材质 + hairline separator —— */
.dialog-header {
  display: flex;
  min-height: 72px;
  padding: var(--space-4) var(--space-5);
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
  border-bottom: 1px solid var(--color-border-subtle);
  background: var(--material-topbar);
  backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
  -webkit-backdrop-filter: blur(var(--material-blur)) saturate(var(--material-saturate));
}

.dialog-header__text {
  min-width: 0;
  flex: 1;
}

/* —— iOS Title：title-3 (20px) semibold + 紧字距 —— */
.dialog-title {
  margin: 0;
  font-family: var(--font-display);
  font-size: var(--text-title-3);
  font-weight: 600;
  line-height: 24px;
  letter-spacing: -0.01em;
}

.dialog-description {
  margin: var(--space-1) 0 0;
  color: var(--color-text-secondary);
  font-size: var(--text-subhead);
  font-weight: 400;
  line-height: 20px;
}

/* —— iOS 风格关闭按钮：圆形，hover 时浅灰背景 —— */
.close-button {
  display: inline-grid;
  width: 28px;
  height: 28px;
  flex: 0 0 28px;
  padding: 0;
  place-items: center;
  color: var(--color-text-secondary);
  border: none;
  border-radius: var(--radius-pill);
  background: var(--color-surface-hover);
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.close-button:hover:not(:disabled) {
  color: var(--color-text);
  background: var(--color-border);
}

.close-button:active:not(:disabled) {
  transform: scale(0.92);
}

.dialog-content {
  min-width: 0;
}

/* —— iOS 风格 footer：action bar，按钮靠右 —— */
.dialog-footer {
  display: flex;
  min-height: 64px;
  padding: var(--space-3) var(--space-5);
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-2);
  border-top: 1px solid var(--color-border-subtle);
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

@media (max-width: 560px) {
  .dialog-backdrop {
    padding: var(--space-3);
  }

  .modal-dialog {
    /* iOS sheet 在小屏上更靠近底部、更大圆角 */
    border-radius: var(--radius-dialog);
  }

  .dialog-header,
  .dialog-footer {
    padding-inline: var(--space-4);
  }
}
</style>
