import { ref } from "vue";
import { defineStore } from "pinia";

import {
  getWorkspaceStatus,
  initializeWorkspace,
  isWorkspaceRuntimeAvailable,
} from "../../bridge/workspace";

/**
 * Workspace 内部状态机（仅暴露最小接口）。
 *
 * 历史：这个 store 之前暴露了 phase / status / profile / isInitialized /
 * isStorageHealthy / isBusy / rename 等 UI 状态，Dashboard、设置页、各
 * 业务页面都用它判断应用启动流程。UI 层的"工作空间切换器"概念移除后，
 * store 不再需要让 UI 感知到这些细节，仅保留一个 isReady 标志位。
 *
 * 行为：
 * - ensureReady() 由 AppShell 启动时调用一次，幂等
 * - preview 模式（无 Tauri runtime）视为就绪
 * - 已初始化的 workspace 直接就绪
 * - 未初始化的 workspace 使用默认名称自动创建（消除 UI 入口）
 * - 失败时设置 errorMessage，UI 显示重试按钮
 */
export const useWorkspaceStore = defineStore("workspace", () => {
  const isReady = ref(false);
  const errorMessage = ref<string | null>(null);

  async function ensureReady(): Promise<void> {
    if (isReady.value) {
      return;
    }

    if (!isWorkspaceRuntimeAvailable()) {
      isReady.value = true;
      return;
    }

    try {
      let status = await getWorkspaceStatus();
      if (!status.initialized) {
        const displayName = "OPEN AIGC";
        await initializeWorkspace({
          workspaceName: `${displayName}的工作空间`,
          teacherName: displayName,
        });
        status = await getWorkspaceStatus();
      }
      isReady.value = Boolean(status.initialized);
    } catch (error) {
      errorMessage.value = error instanceof Error ? error.message : "工作空间初始化失败";
    }
  }

  return { isReady, errorMessage, ensureReady };
});
