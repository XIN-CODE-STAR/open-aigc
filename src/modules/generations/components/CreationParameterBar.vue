<script setup lang="ts">
/**
 * CreationParameterBar：创作参数芯片条。
 *
 * 涵盖四个参数：
 * - 比例（image / video 模式）：含 1:1, 4:3, 16:9, 9:16, smart
 * - 时长（仅 video 模式）：4s / 6s / 8s
 * - 分辨率（仅 video 模式）：720P / 1080P
 * - 帧率（仅 video 模式）：24fps / 30fps / 60fps
 *
 * Phase 1 拆分目标：只通过 props/emit 通信，不直接调 bridge / store。
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { CheckCircle2, ChevronDown, Clapperboard, RefreshCw, Sparkles } from "@lucide/vue";

type CreationMode =
  "agent" | "image" | "video" | "music" | "voiceover" | "digital-human" | "motion";

interface AspectRatioItem {
  id: string;
  label: string;
  /** smart 时为 0/0；否则表示实际宽高比。 */
  width: number;
  height: number;
}

const ASPECT_RATIOS: AspectRatioItem[] = [
  { id: "smart", label: "智能", width: 0, height: 0 },
  { id: "21:9", label: "21:9", width: 21, height: 9 },
  { id: "16:9", label: "16:9", width: 16, height: 9 },
  { id: "3:2", label: "3:2", width: 3, height: 2 },
  { id: "4:3", label: "4:3", width: 4, height: 3 },
  { id: "1:1", label: "1:1", width: 1, height: 1 },
  { id: "3:4", label: "3:4", width: 3, height: 4 },
  { id: "2:3", label: "2:3", width: 2, height: 3 },
  { id: "9:16", label: "9:16", width: 9, height: 16 },
];

const VIDEO_DURATIONS = ["4s", "5s", "8s", "10s", "15s"] as const;
const RESOLUTIONS = ["720P", "1080P", "2K", "4K"] as const;
const FRAME_RATES = ["24fps", "25fps", "30fps", "60fps"] as const;

const props = defineProps<{
  creationMode: CreationMode;
  aspectRatio: string;
  videoDuration: string;
  resolution: string;
  frameRate: string;
}>();

// 引用一次以满足 vue-tsc "declared but never read" 检查
void props;

const emit = defineEmits<{
  "update:aspectRatio": [value: string];
  "update:videoDuration": [value: string];
  "update:resolution": [value: string];
  "update:frameRate": [value: string];
}>();

const aspectMenuOpen = ref(false);
const durationMenuOpen = ref(false);
const resolutionMenuOpen = ref(false);
const frameRateMenuOpen = ref(false);

const aspectMenuRef = ref<HTMLElement | null>(null);
const durationMenuRef = ref<HTMLElement | null>(null);
const resolutionMenuRef = ref<HTMLElement | null>(null);
const frameRateMenuRef = ref<HTMLElement | null>(null);

function closeAll(): void {
  aspectMenuOpen.value = false;
  durationMenuOpen.value = false;
  resolutionMenuOpen.value = false;
  frameRateMenuOpen.value = false;
}

function toggleMenu(target: "aspect" | "duration" | "resolution" | "frameRate"): void {
  closeAll();
  switch (target) {
    case "aspect":
      aspectMenuOpen.value = !aspectMenuOpen.value;
      break;
    case "duration":
      durationMenuOpen.value = !durationMenuOpen.value;
      break;
    case "resolution":
      resolutionMenuOpen.value = !resolutionMenuOpen.value;
      break;
    case "frameRate":
      frameRateMenuOpen.value = !frameRateMenuOpen.value;
      break;
  }
}

function pickAspectRatio(id: string): void {
  emit("update:aspectRatio", id);
  aspectMenuOpen.value = false;
}

function pickVideoDuration(d: string): void {
  emit("update:videoDuration", d);
  durationMenuOpen.value = false;
}

function pickResolution(r: string): void {
  emit("update:resolution", r);
  resolutionMenuOpen.value = false;
}

function pickFrameRate(f: string): void {
  emit("update:frameRate", f);
  frameRateMenuOpen.value = false;
}

function handleDocumentClick(event: MouseEvent): void {
  const target = event.target as Node | null;
  if (!target) return;
  if (aspectMenuRef.value?.contains(target)) return;
  if (durationMenuRef.value?.contains(target)) return;
  if (resolutionMenuRef.value?.contains(target)) return;
  if (frameRateMenuRef.value?.contains(target)) return;
  closeAll();
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") closeAll();
}

onMounted(() => {
  document.addEventListener("click", handleDocumentClick);
  document.addEventListener("keydown", handleKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleDocumentClick);
  document.removeEventListener("keydown", handleKeydown);
});

/** 比例预览尺寸：把比例映射成固定宽度的小色块。 */
function ratioPreviewStyle(item: AspectRatioItem, base: number): Record<string, string> {
  if (item.width === 0 || item.height === 0) return {};
  const width = base;
  const height = Math.round((base * item.height) / item.width);
  return {
    width: `${width}px`,
    height: `${height}px`,
    maxHeight: `${base}px`,
  };
}
</script>

<template>
  <div class="param-row">
    <!-- 比例（image / video） -->
    <div
      v-if="creationMode === 'image' || creationMode === 'video'"
      ref="aspectMenuRef"
      class="param-wrap"
    >
      <button type="button" class="param-chip" @click="toggleMenu('aspect')">
        <span
          v-if="aspectRatio !== 'smart'"
          class="param-ratio"
          :style="
            ratioPreviewStyle(
              ASPECT_RATIOS.find((a) => a.id === aspectRatio) ?? ASPECT_RATIOS[0],
              12,
            )
          "
        ></span>
        <Sparkles v-else :size="11" />
        <span>{{ aspectRatio }}</span>
        <ChevronDown :size="10" class="param-caret" :class="{ 'is-open': aspectMenuOpen }" />
      </button>
      <Transition name="menu">
        <div v-if="aspectMenuOpen" class="param-pop">
          <button
            v-for="ar in ASPECT_RATIOS"
            :key="ar.id"
            type="button"
            class="param-pop-item"
            :class="{ 'is-selected': ar.id === aspectRatio }"
            @click="pickAspectRatio(ar.id)"
          >
            <span
              v-if="ar.id !== 'smart'"
              class="param-pop-ratio"
              :style="ratioPreviewStyle(ar, 16)"
            ></span>
            <Sparkles v-else :size="13" />
            <span>{{ ar.label }}</span>
            <CheckCircle2 v-if="ar.id === aspectRatio" :size="11" class="param-pop-check" />
          </button>
        </div>
      </Transition>
    </div>

    <!-- 时长（仅 video） -->
    <div v-if="creationMode === 'video'" ref="durationMenuRef" class="param-wrap">
      <button type="button" class="param-chip" @click="toggleMenu('duration')">
        <Clapperboard :size="11" />
        <span>{{ videoDuration }}</span>
        <ChevronDown :size="10" class="param-caret" :class="{ 'is-open': durationMenuOpen }" />
      </button>
      <Transition name="menu">
        <div v-if="durationMenuOpen" class="param-pop">
          <button
            v-for="d in VIDEO_DURATIONS"
            :key="d"
            type="button"
            class="param-pop-item"
            :class="{ 'is-selected': d === videoDuration }"
            @click="pickVideoDuration(d)"
          >
            <span>{{ d }}</span>
            <CheckCircle2 v-if="d === videoDuration" :size="11" class="param-pop-check" />
          </button>
        </div>
      </Transition>
    </div>

    <!-- 分辨率（仅 video） -->
    <div v-if="creationMode === 'video'" ref="resolutionMenuRef" class="param-wrap">
      <button type="button" class="param-chip" @click="toggleMenu('resolution')">
        <span class="param-badge">{{ resolution.replace("P", "") }}</span>
        <span>{{ resolution }}</span>
        <ChevronDown :size="10" class="param-caret" :class="{ 'is-open': resolutionMenuOpen }" />
      </button>
      <Transition name="menu">
        <div v-if="resolutionMenuOpen" class="param-pop">
          <button
            v-for="r in RESOLUTIONS"
            :key="r"
            type="button"
            class="param-pop-item"
            :class="{ 'is-selected': r === resolution }"
            @click="pickResolution(r)"
          >
            <span>{{ r }}</span>
            <CheckCircle2 v-if="r === resolution" :size="11" class="param-pop-check" />
          </button>
        </div>
      </Transition>
    </div>

    <!-- 帧率（仅 video） -->
    <div v-if="creationMode === 'video'" ref="frameRateMenuRef" class="param-wrap">
      <button type="button" class="param-chip" @click="toggleMenu('frameRate')">
        <RefreshCw :size="11" />
        <span>{{ frameRate }}</span>
        <ChevronDown :size="10" class="param-caret" :class="{ 'is-open': frameRateMenuOpen }" />
      </button>
      <Transition name="menu">
        <div v-if="frameRateMenuOpen" class="param-pop">
          <button
            v-for="f in FRAME_RATES"
            :key="f"
            type="button"
            class="param-pop-item"
            :class="{ 'is-selected': f === frameRate }"
            @click="pickFrameRate(f)"
          >
            <span>{{ f }}</span>
            <CheckCircle2 v-if="f === frameRate" :size="11" class="param-pop-check" />
          </button>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.param-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.param-wrap {
  position: relative;
}

.param-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.param-chip:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

:root:not([data-theme="dark"]) .param-chip {
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.8);
}

:root:not([data-theme="dark"]) .param-chip:hover {
  background: rgba(0, 0, 0, 0.06);
  border-color: rgba(0, 0, 0, 0.2);
}

.param-ratio {
  display: inline-block;
  background: currentColor;
  opacity: 0.7;
  border-radius: 1px;
}

.param-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 14px;
  padding: 0 4px;
  border-radius: 4px;
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.param-caret {
  opacity: 0.6;
  transition: transform var(--duration-fast) var(--ease-out);
}

.param-caret.is-open {
  transform: rotate(180deg);
}

.param-pop {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 140px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(28, 28, 30, 0.98);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 30;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

:root:not([data-theme="dark"]) .param-pop {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
}

.param-pop-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-out);
}

.param-pop-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.param-pop-item.is-selected {
  color: var(--color-accent);
}

.param-pop-ratio {
  display: inline-block;
  background: currentColor;
  opacity: 0.7;
  border-radius: 1px;
}

.param-pop-check {
  margin-left: auto;
}

:root:not([data-theme="dark"]) .param-pop-item {
  color: rgba(0, 0, 0, 0.85);
}

:root:not([data-theme="dark"]) .param-pop-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

/* —— 过渡：菜单 —— */
.menu-enter-active,
.menu-leave-active {
  transition:
    opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(4px);
}

@media (prefers-reduced-motion: reduce) {
  .menu-enter-active,
  .menu-leave-active {
    transition-duration: 0.01ms !important;
  }
}
</style>
