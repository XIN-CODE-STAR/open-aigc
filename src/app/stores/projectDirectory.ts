import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { setOutputDirectory } from "../../bridge/agent";

const STORAGE_KEY = "aigc-studio.project-dir";

/**
 * 项目目录管理：用户选择的生成输出目录。
 * 选择后，生成的图片会同时保存到该目录和 managed storage。
 */
export const useProjectDirectory = defineStore("projectDirectory", () => {
  const selectedPath = ref<string | null>(null);
  /** 外部设置的上下文名称（如当前会话/项目标题），作为 selectedPath 为空时的显示回退。 */
  const contextName = ref<string | null>(null);

  const displayName = computed(() => {
    if (!selectedPath.value || selectedPath.value.trim().length === 0) {
      // 没有自定义目录时，使用上下文名称
      if (contextName.value && contextName.value.trim().length > 0) return contextName.value;
      return "OPEN AIGC";
    }
    // 提取最后一级文件夹名
    const parts = selectedPath.value.replace(/[/\\]$/, "").split(/[/\\]/);
    const name = parts[parts.length - 1];
    return name && name.trim().length > 0 ? name : "OPEN AIGC";
  });

  const isCustom = computed(() => !!selectedPath.value && selectedPath.value.trim().length > 0);

  /** 工作目录名称：自定义目录时为文件夹名，否则使用上下文名称，默认为 "OPEN AIGC"。 */
  const workspaceName = computed(() => displayName.value);

  /** 恢复持久化的选择 */
  function restore(): void {
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved && saved.trim().length > 0) {
        selectedPath.value = saved;
        // 同步到后端
        void setOutputDirectory(saved);
      }
    } catch {
      // ignore
    }
  }

  /** 打开系统目录选择器 */
  async function selectDirectory(): Promise<void> {
    try {
      const result = await invoke<string | null>("plugin:dialog|open", {
        options: {
          directory: true,
          multiple: false,
          title: "选择项目输出目录",
        },
      });
      if (result && result.trim().length > 0) {
        selectedPath.value = result;
        persist();
        // 同步到后端（独立 try-catch，不阻塞 UI 更新）
        setOutputDirectory(result).catch(() => {
          /* 静默忽略 */
        });
      }
    } catch (e) {
      console.warn("[ProjectDirectory] selectDirectory failed:", e);
    }
  }

  /** 恢复为默认工作区 */
  async function resetToDefault(): Promise<void> {
    selectedPath.value = null;
    persist();
    await setOutputDirectory(null);
  }

  /** 设置上下文名称（切换会话/项目时由外部调用）。 */
  function setContextName(name: string | null): void {
    contextName.value = name && name.trim().length > 0 ? name.trim() : null;
  }

  function persist(): void {
    try {
      if (selectedPath.value) {
        window.localStorage.setItem(STORAGE_KEY, selectedPath.value);
      } else {
        window.localStorage.removeItem(STORAGE_KEY);
      }
    } catch {
      // ignore
    }
  }

  return {
    selectedPath,
    contextName,
    displayName,
    workspaceName,
    isCustom,
    restore,
    selectDirectory,
    resetToDefault,
    setContextName,
  };
});
