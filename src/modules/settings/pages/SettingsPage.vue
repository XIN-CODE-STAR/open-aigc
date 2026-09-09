<script setup lang="ts">
import { ref } from "vue";
import type { Component } from "vue";
import {
  AlignJustify,
  Bell,
  Bot,
  Cpu,
  FolderOpen,
  Keyboard,
  List,
  Monitor,
  Moon,
  Rows3,
  Save,
  Sun,
  Volume2,
  VolumeX,
  Waves,
  Zap,
} from "@lucide/vue";

import {
  type DensityPreference,
  type ThemePreference,
  usePreferencesStore,
} from "../../../app/stores/preferences";
import { useProjectDirectory } from "../../../app/stores/projectDirectory";

interface SegmentedOption<T extends string> {
  icon: Component;
  label: string;
  value: T;
}

const preferences = usePreferencesStore();
const projectDir = useProjectDirectory();

const themeOptions: SegmentedOption<ThemePreference>[] = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "浅色", icon: Sun },
  { value: "dark", label: "深色", icon: Moon },
];

const densityOptions: SegmentedOption<DensityPreference>[] = [
  { value: "compact", label: "紧凑", icon: List },
  { value: "standard", label: "标准", icon: AlignJustify },
  { value: "comfortable", label: "舒适", icon: Rows3 },
];

const temperatureStr = ref(preferences.defaultTemperature.toFixed(1));
const maxTokensStr = ref(String(preferences.defaultMaxTokens));

function onTemperatureInput(event: Event): void {
  const val = parseFloat((event.target as HTMLInputElement).value);
  if (!isNaN(val)) {
    preferences.setDefaultTemperature(val);
    temperatureStr.value = preferences.defaultTemperature.toFixed(1);
  }
}

function onMaxTokensInput(event: Event): void {
  const val = parseInt((event.target as HTMLInputElement).value, 10);
  if (!isNaN(val)) {
    preferences.setDefaultMaxTokens(val);
    maxTokensStr.value = String(preferences.defaultMaxTokens);
  }
}
</script>

<template>
  <div class="settings-page">
    <p class="settings-page__description">应用偏好保存在当前设备的本地存储中。</p>

    <!-- ═══ 外观 ═══ -->
    <section class="settings-section" aria-labelledby="theme-title">
      <div class="setting-copy">
        <h2 id="theme-title">主题</h2>
        <p>选择应用使用的颜色模式。</p>
      </div>
      <div class="segmented-control" role="radiogroup" aria-labelledby="theme-title">
        <button
          v-for="option in themeOptions"
          :key="option.value"
          type="button"
          role="radio"
          :aria-checked="preferences.theme === option.value"
          :class="{ 'is-selected': preferences.theme === option.value }"
          @click="preferences.setTheme(option.value)"
        >
          <component :is="option.icon" :size="17" :stroke-width="1.8" />
          <span>{{ option.label }}</span>
        </button>
      </div>
    </section>

    <section class="settings-section" aria-labelledby="density-title">
      <div class="setting-copy">
        <h2 id="density-title">界面密度</h2>
        <p>调整导航、工具栏和内容区域的间距。</p>
      </div>
      <div class="segmented-control" role="radiogroup" aria-labelledby="density-title">
        <button
          v-for="option in densityOptions"
          :key="option.value"
          type="button"
          role="radio"
          :aria-checked="preferences.density === option.value"
          :class="{ 'is-selected': preferences.density === option.value }"
          @click="preferences.setDensity(option.value)"
        >
          <component :is="option.icon" :size="17" :stroke-width="1.8" />
          <span>{{ option.label }}</span>
        </button>
      </div>
    </section>

    <section class="settings-section" aria-labelledby="navigation-title">
      <div class="setting-copy">
        <h2 id="navigation-title">展开主导航</h2>
        <p>宽窗口启动时显示图标和模块名称。</p>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          aria-labelledby="navigation-title"
          :checked="preferences.navigationExpanded"
          @change="preferences.toggleNavigation"
        />
        <span aria-hidden="true"></span>
        <b>{{ preferences.navigationExpanded ? "已开启" : "已关闭" }}</b>
      </label>
    </section>

    <!-- ═══ Agent 设置 ═══ -->
    <div class="settings-divider">
      <Bot :size="16" :stroke-width="1.8" />
      <span>Agent 设置</span>
    </div>

    <section class="settings-section" aria-labelledby="streaming-title">
      <div class="setting-copy">
        <h2 id="streaming-title">
          <Waves :size="15" :stroke-width="1.8" class="setting-icon" />
          流式输出
        </h2>
        <p>开启后 Agent 回复将逐字显示，关闭则等待完整回复后一次性展示。</p>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          aria-labelledby="streaming-title"
          :checked="preferences.streamingEnabled"
          @change="preferences.toggleStreaming"
        />
        <span aria-hidden="true"></span>
        <b>{{ preferences.streamingEnabled ? "已开启" : "已关闭" }}</b>
      </label>
    </section>

    <section class="settings-section" aria-labelledby="temperature-title">
      <div class="setting-copy">
        <h2 id="temperature-title">
          <Zap :size="15" :stroke-width="1.8" class="setting-icon" />
          创作温度
        </h2>
        <p>控制 Agent 回复的创造性。0 = 精确保守，1 = 平衡，2 = 最大创意。</p>
      </div>
      <div class="range-row">
        <input
          type="range"
          min="0"
          max="2"
          step="0.1"
          :value="preferences.defaultTemperature"
          class="range-input"
          aria-labelledby="temperature-title"
          @input="onTemperatureInput"
        />
        <span class="range-value">{{ temperatureStr }}</span>
      </div>
    </section>

    <section class="settings-section" aria-labelledby="max-tokens-title">
      <div class="setting-copy">
        <h2 id="max-tokens-title">
          <Cpu :size="15" :stroke-width="1.8" class="setting-icon" />
          最大输出长度
        </h2>
        <p>单次 Agent 回复的最大 token 数量。较大的值允许更长的回复。</p>
      </div>
      <div class="range-row">
        <input
          type="range"
          min="256"
          max="32768"
          step="256"
          :value="preferences.defaultMaxTokens"
          class="range-input"
          aria-labelledby="max-tokens-title"
          @input="onMaxTokensInput"
        />
        <span class="range-value">{{ maxTokensStr }}</span>
      </div>
    </section>

    <section class="settings-section" aria-labelledby="auto-save-title">
      <div class="setting-copy">
        <h2 id="auto-save-title">
          <Save :size="15" :stroke-width="1.8" class="setting-icon" />
          自动保存会话
        </h2>
        <p>关闭后退出应用时不再保留对话历史。</p>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          aria-labelledby="auto-save-title"
          :checked="preferences.autoSaveConversations"
          @change="preferences.toggleAutoSave"
        />
        <span aria-hidden="true"></span>
        <b>{{ preferences.autoSaveConversations ? "已开启" : "已关闭" }}</b>
      </label>
    </section>

    <!-- ═══ 通知与声音 ═══ -->
    <div class="settings-divider">
      <Bell :size="16" :stroke-width="1.8" />
      <span>通知与声音</span>
    </div>

    <section class="settings-section" aria-labelledby="sound-title">
      <div class="setting-copy">
        <h2 id="sound-title">
          <Volume2
            v-if="preferences.soundEnabled"
            :size="15"
            :stroke-width="1.8"
            class="setting-icon"
          />
          <VolumeX v-else :size="15" :stroke-width="1.8" class="setting-icon" />
          提示音
        </h2>
        <p>Agent 回复完成或生成失败时播放提示音。</p>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          aria-labelledby="sound-title"
          :checked="preferences.soundEnabled"
          @change="preferences.toggleSound"
        />
        <span aria-hidden="true"></span>
        <b>{{ preferences.soundEnabled ? "已开启" : "已关闭" }}</b>
      </label>
    </section>

    <!-- ═══ 存储 ═══ -->
    <div class="settings-divider">
      <FolderOpen :size="16" :stroke-width="1.8" />
      <span>存储</span>
    </div>

    <section class="settings-section" aria-labelledby="output-dir-title">
      <div class="setting-copy">
        <h2 id="output-dir-title">默认输出目录</h2>
        <p>图片、视频等生成产物的保存位置。可在创意工坊中按会话单独修改。</p>
      </div>
      <div class="dir-display">
        <span class="dir-path" :title="projectDir.selectedPath ?? '默认工作区'">
          {{ projectDir.displayName }}
        </span>
        <button type="button" class="btn-secondary" @click="projectDir.selectDirectory()">
          更换
        </button>
        <button
          v-if="projectDir.isCustom"
          type="button"
          class="btn-ghost"
          @click="projectDir.resetToDefault()"
        >
          恢复默认
        </button>
      </div>
    </section>

    <!-- ═══ 快捷键 ═══ -->
    <div class="settings-divider">
      <Keyboard :size="16" :stroke-width="1.8" />
      <span>快捷键</span>
    </div>

    <section class="settings-section settings-section--shortcuts" aria-labelledby="shortcuts-title">
      <div class="setting-copy">
        <h2 id="shortcuts-title">常用快捷键</h2>
        <p>提升创作效率的键盘快捷操作。</p>
      </div>
      <div class="shortcuts-grid">
        <div class="shortcut-item">
          <span class="shortcut-desc">发送消息</span>
          <kbd>Enter</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">换行</span>
          <kbd>Shift + Enter</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">粘贴图片</span>
          <kbd>Ctrl + V</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">展开侧边栏</span>
          <kbd>Ctrl + B</kbd>
        </div>
      </div>
    </section>

    <!-- ═══ 关于 ═══ -->
    <div class="settings-divider">
      <span>关于</span>
    </div>

    <section class="settings-section" aria-labelledby="about-title">
      <div class="setting-copy">
        <h2 id="about-title">OPEN AIGC Studio</h2>
        <p>AI 驱动的创意工作台，支持图片、视频、音乐、数字人等多模态创作。</p>
      </div>
      <div class="about-info">
        <span class="about-version">v0.2.0</span>
        <span class="about-platform">Tauri + Vue 3</span>
      </div>
    </section>
  </div>
</template>

<style scoped>
.settings-page {
  max-width: 920px;
}

.settings-page__description {
  margin: 0;
  padding-bottom: var(--space-5);
  color: var(--color-text-secondary);
  border-bottom: 1px solid var(--color-border-subtle);
  font-size: 14px;
  line-height: 20px;
}

.settings-divider {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 28px 0 4px;
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.settings-section {
  display: grid;
  min-height: 72px;
  padding: 16px 0;
  align-items: center;
  grid-template-columns: minmax(220px, 1fr) minmax(280px, auto);
  gap: var(--space-6);
  border-bottom: 1px solid var(--color-border-subtle);
}

.settings-section--shortcuts {
  align-items: start;
}

.setting-copy h2 {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 24px;
}

.setting-icon {
  opacity: 0.6;
}

.setting-copy p {
  margin: 4px 0 0;
  color: var(--color-text-secondary);
  font-size: 12px;
  line-height: 18px;
}

/* ─── 分段控件 ─── */
.segmented-control {
  display: grid;
  min-width: 280px;
  height: var(--control-height);
  padding: 2px;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.segmented-control button {
  display: flex;
  min-width: 0;
  padding: 0 var(--space-2);
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--color-text-secondary);
  border: 0;
  border-radius: 3px;
  background: transparent;
  cursor: pointer;
  font-size: 13px;
  line-height: 20px;
  white-space: nowrap;
}

.segmented-control button:hover {
  color: var(--color-text);
  background: var(--color-surface-hover);
}

.segmented-control button.is-selected {
  color: var(--color-text);
  background: var(--color-surface);
  box-shadow: 0 1px 2px rgb(0 0 0 / 12%);
  font-weight: 600;
}

/* ─── 开关 ─── */
.switch {
  display: inline-flex;
  justify-self: end;
  align-items: center;
  gap: var(--space-2);
  cursor: pointer;
}

.switch input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
}

.switch > span {
  position: relative;
  display: block;
  width: 40px;
  height: 20px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface-subtle);
  transition:
    background 160ms ease,
    border-color 160ms ease;
}

.switch > span::after {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--color-text-secondary);
  content: "";
  transition:
    transform 160ms ease,
    background 160ms ease;
}

.switch input:checked + span {
  border-color: var(--color-accent);
  background: var(--color-accent);
}

.switch input:checked + span::after {
  background: var(--color-surface);
  transform: translateX(20px);
}

.switch input:focus-visible + span {
  outline: 2px solid var(--color-focus);
  outline-offset: 2px;
}

.switch b {
  min-width: 48px;
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 400;
  line-height: 20px;
}

/* ─── 滑块 ─── */
.range-row {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-self: end;
  min-width: 240px;
}

.range-input {
  flex: 1;
  height: 4px;
  appearance: none;
  background: var(--color-border);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
}

.range-input::-webkit-slider-thumb {
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--color-accent);
  border: 2px solid var(--color-surface);
  box-shadow: 0 1px 3px rgb(0 0 0 / 20%);
  cursor: pointer;
}

.range-input::-webkit-slider-thumb:hover {
  transform: scale(1.1);
}

.range-value {
  min-width: 48px;
  text-align: right;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
  font-variant-numeric: tabular-nums;
}

/* ─── 目录显示 ─── */
.dir-display {
  display: flex;
  align-items: center;
  gap: 8px;
  justify-self: end;
}

.dir-path {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: var(--color-text-secondary);
  padding: 4px 10px;
  background: var(--color-surface-subtle);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
}

.btn-secondary {
  padding: 4px 12px;
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text);
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-control);
  cursor: pointer;
  transition: background 120ms ease;
}

.btn-secondary:hover {
  background: var(--color-surface-hover);
}

.btn-ghost {
  padding: 4px 10px;
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text-secondary);
  background: transparent;
  border: none;
  border-radius: var(--radius-control);
  cursor: pointer;
  transition: color 120ms ease;
}

.btn-ghost:hover {
  color: var(--color-text);
}

/* ─── 快捷键 ─── */
.shortcuts-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

.shortcut-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 10px;
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.shortcut-desc {
  font-size: 12px;
  color: var(--color-text-secondary);
}

kbd {
  display: inline-block;
  padding: 2px 6px;
  font-size: 11px;
  font-family: inherit;
  font-weight: 500;
  color: var(--color-text);
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  box-shadow: 0 1px 0 var(--color-border);
  white-space: nowrap;
}

/* ─── 关于 ─── */
.about-info {
  display: flex;
  align-items: center;
  gap: 16px;
  justify-self: end;
}

.about-version {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
}

.about-platform {
  font-size: 12px;
  color: var(--color-text-secondary);
  padding: 2px 8px;
  background: var(--color-surface-subtle);
  border-radius: var(--radius-control);
}

@media (max-width: 1100px) {
  .settings-section {
    grid-template-columns: 1fr;
    gap: var(--space-3);
  }

  .segmented-control {
    min-width: 0;
    width: min(100%, 420px);
  }

  .switch,
  .range-row,
  .dir-display,
  .about-info {
    justify-self: start;
  }

  .range-row {
    min-width: 0;
    width: min(100%, 320px);
  }
}
</style>
