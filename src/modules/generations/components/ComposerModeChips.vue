<script setup lang="ts">
/**
 * ComposerModeChips：创作器共用的创作模式选择。
 *
 * - 主模式胶囊 + "更多"下拉（扩展模式）
 * - welcome（居中大胶囊）与 conversation（紧凑芯片）共用一套逻辑
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ChevronDown } from "@lucide/vue";

export interface ComposerModeItem {
  id: string;
  label: string;
  description: string;
  icon: unknown;
}

const props = withDefaults(
  defineProps<{
    primaryModes: ComposerModeItem[];
    extraModes: ComposerModeItem[];
    currentMode: ComposerModeItem;
    creationMode: string;
    showModeMenu: boolean;
    size?: "lg" | "sm";
    /** 更多下拉的弹出方向 */
    menuDirection?: "up" | "down";
  }>(),
  { size: "sm", menuDirection: "up" },
);

const emit = defineEmits<{
  "update:creationMode": [value: string];
  "update:showModeMenu": [value: boolean];
}>();

const moreRef = ref<HTMLElement | null>(null);

const isExtraActive = computed(() => props.extraModes.some((m) => m.id === props.creationMode));

function toggleMore(): void {
  emit("update:showModeMenu", !props.showModeMenu);
}

function pick(mode: ComposerModeItem): void {
  emit("update:creationMode", mode.id);
  emit("update:showModeMenu", false);
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as Node | null;
  if (moreRef.value && target && !moreRef.value.contains(target)) {
    if (props.showModeMenu) emit("update:showModeMenu", false);
  }
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
});
</script>

<template>
  <div class="mode-chips" :class="`mode-chips--${size}`">
    <button
      v-for="mode in primaryModes"
      :key="mode.id"
      type="button"
      class="mode-chips__pill"
      :class="{ 'is-active': creationMode === mode.id }"
      :title="mode.description"
      @click="pick(mode)"
    >
      <component :is="mode.icon" :size="size === 'lg' ? 14 : 13" />
      <span>{{ mode.label }}</span>
    </button>

    <span ref="moreRef" class="mode-chips__more-wrap">
      <button
        type="button"
        class="mode-chips__pill"
        :class="{ 'is-active': isExtraActive }"
        @click="toggleMore"
      >
        <component
          :is="isExtraActive ? currentMode.icon : undefined"
          v-if="isExtraActive"
          :size="13"
        />
        {{ isExtraActive ? currentMode.label : "更多" }}
        <ChevronDown :size="12" class="mode-chips__caret" :class="{ 'is-open': showModeMenu }" />
      </button>
      <Transition name="mode-chips">
        <div
          v-if="showModeMenu"
          class="mode-chips__menu"
          :class="`mode-chips__menu--${menuDirection}`"
        >
          <button
            v-for="mode in extraModes"
            :key="mode.id"
            type="button"
            class="mode-chips__menu-item"
            :class="{ 'is-active': creationMode === mode.id }"
            @click="pick(mode)"
          >
            <component :is="mode.icon" :size="14" />
            <span>{{ mode.label }}</span>
          </button>
        </div>
      </Transition>
    </span>
  </div>
</template>

<style scoped>
.mode-chips {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.mode-chips__more-wrap {
  position: relative;
  display: inline-flex;
}

.mode-chips__pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: var(--font-weight-medium);
  cursor: pointer;
  white-space: nowrap;
  transition:
    background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.mode-chips__pill:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.mode-chips__pill.is-active {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.mode-chips__caret {
  transition: transform var(--duration-fast) var(--ease-out);
}

.mode-chips__caret.is-open {
  transform: rotate(180deg);
}

/* —— 更多下拉 —— */
.mode-chips__menu--up {
  bottom: calc(100% + 6px);
}

.mode-chips__menu--down {
  top: calc(100% + 6px);
}
.mode-chips__menu {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  min-width: 150px;
  padding: 4px;
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  background: var(--color-surface);
  box-shadow: var(--shadow-menu);
  z-index: var(--z-dropdown);
}

.mode-chips__menu-item {
  display: flex;
  width: 100%;
  height: 32px;
  padding: 0 var(--space-2);
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border: none;
  border-radius: 7px;
  background: transparent;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.mode-chips__menu-item:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.mode-chips__menu-item.is-active {
  color: var(--color-accent);
  background: var(--color-accent-soft);
}

.mode-chips-enter-active,
.mode-chips-leave-active {
  transition:
    opacity var(--duration-fast) ease,
    transform var(--duration-fast) var(--ease-out);
}

.mode-chips-enter-from,
.mode-chips-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(4px);
}
</style>
