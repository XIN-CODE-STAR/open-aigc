import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { readProjectMemory, setOutputDirectory } from "../../bridge/agent";

const STORAGE_KEY = "aigc-studio.project-dir";
const RECENTS_KEY = "aigc-studio.project-dirs";
const RECENTS_LIMIT = 8;

/** 取路径的最子级文件夹名（兼容 \\ 与 / 分隔，结尾斜杠容忍）。 */
export function projectDirName(path: string): string {
  const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
  const name = parts[parts.length - 1];
  return name && name.trim().length > 0 ? name : path;
}

/**
 * 项目目录管理：用户选择的生成输出目录。
 * 选择后，生成结果会导出到：
 *   <工作目录>/generated/<对话标题或日期>/
 * 同时仍写入默认 workspace 的 managed-files / downloads（资源库完整性）。
 * 未选择目录时，仅使用默认 workspace 存储。
 *
 * 打开工作目录时后端会在目录内落地 `.openaigc/` 项目文件夹
 * （project.json 元数据 + AIGC.md 项目记忆），对标 .claude/.codex 惯例；
 * AIGC.md 的内容会注入 Agent 系统提示词（见 projectMemory）。
 */
export const useProjectDirectory = defineStore("projectDirectory", () => {
  const selectedPath = ref<string | null>(null);
  /** 外部设置的上下文名称（如当前会话/项目标题），作为 selectedPath 为空时的显示回退。 */
  const contextName = ref<string | null>(null);
  /** 最近打开过的工作目录（最新在前，去重，最多 RECENTS_LIMIT 个）。 */
  const recentDirs = ref<string[]>([]);
  /** 当前工作目录的项目记忆（.openaigc/AIGC.md 内容）。 */
  const projectMemory = ref<string | null>(null);

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

  function loadRecents(): string[] {
    try {
      const raw = window.localStorage.getItem(RECENTS_KEY);
      if (!raw) return [];
      const parsed: unknown = JSON.parse(raw);
      return Array.isArray(parsed)
        ? parsed.filter((v): v is string => typeof v === "string" && v.trim().length > 0)
        : [];
    } catch {
      return [];
    }
  }

  function addRecent(path: string): void {
    const next = [path, ...recentDirs.value.filter((p) => p !== path)].slice(0, RECENTS_LIMIT);
    recentDirs.value = next;
    try {
      window.localStorage.setItem(RECENTS_KEY, JSON.stringify(next));
    } catch {
      // ignore
    }
  }

  /** 读取当前目录的项目记忆（尽力而为，失败时置 null）。 */
  async function refreshMemory(): Promise<void> {
    const dir = selectedPath.value;
    if (!dir || dir.trim().length === 0) {
      projectMemory.value = null;
      return;
    }
    try {
      projectMemory.value = await readProjectMemory(dir);
    } catch {
      projectMemory.value = null;
    }
  }

  /** 恢复持久化的选择 */
  function restore(): void {
    recentDirs.value = loadRecents();
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved && saved.trim().length > 0) {
        selectedPath.value = saved;
        addRecent(saved);
        // 同步到后端
        void setOutputDirectory(saved);
        void refreshMemory();
      }
    } catch {
      // ignore
    }
  }

  /** 打开系统目录选择器（"添加新项目"）。 */
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
        await applySelection(result);
      }
    } catch (e) {
      console.warn("[ProjectDirectory] selectDirectory failed:", e);
    }
  }

  /** 直接选用一个最近打开过的目录（不再弹系统选择器）。 */
  async function selectRecent(path: string): Promise<void> {
    if (!path || path === selectedPath.value) return;
    await applySelection(path);
  }

  /** 应用一次目录选择：更新状态、持久化、同步后端、落地 .openaigc 并刷新记忆。 */
  async function applySelection(path: string): Promise<void> {
    selectedPath.value = path;
    persist();
    addRecent(path);
    // 同步到后端（独立 try-catch，不阻塞 UI 更新）
    setOutputDirectory(path).catch(() => {
      /* 静默忽略 */
    });
    await refreshMemory();
  }

  /** 恢复为默认工作区 */
  async function resetToDefault(): Promise<void> {
    selectedPath.value = null;
    persist();
    projectMemory.value = null;
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
    recentDirs,
    projectMemory,
    displayName,
    workspaceName,
    isCustom,
    restore,
    selectDirectory,
    selectRecent,
    resetToDefault,
    setContextName,
    refreshMemory,
  };
});
