import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { computed, onMounted, onUnmounted, reactive, ref, type Ref } from "vue";

import {
  parseRuntimeEvent,
  RUNTIME_EVENT_CHANNEL,
  type RuntimeEvent,
  type RuntimePhase,
  type ShotStatus,
} from "../../../bridge/runtime";

// ─── Types ───

export interface ShotState {
  shotIndex: number;
  status: ShotStatus;
  artifactId: string | null;
  submissionId: string | null;
  provider: string | null;
  model: string | null;
  message: string;
}

export interface RuntimeLogEntry {
  timestamp: number;
  phase: RuntimePhase | "unknown";
  message: string;
}

// ─── Composable ───

/**
 * 创作流水线运行时状态 composable。
 *
 * 监听 `runtime://event` 频道，维护 CreativeRuntimeService 的实时状态。
 * 与 useAgentConversation 分离（频道独立，不混用 agent://event）。
 *
 * 必须在 Vue 组件 setup 阶段调用。
 */
export function useCreativeRuntime() {
  // ── Reactive state ──
  const currentRunId: Ref<string | null> = ref(null);
  const currentPhase: Ref<RuntimePhase | null> = ref(null);
  const isRunning = ref(false);
  const totalShots = ref(0);
  const shotSuccessCount = ref(0);
  const durationSecs = ref(0);
  const outputAssetId: Ref<string | null> = ref(null);
  const finalStatus: Ref<string | null> = ref(null);

  const shotStates = reactive<Map<number, ShotState>>(new Map());
  const logEntries = reactive<RuntimeLogEntry[]>([]);

  // ── Computed ──
  const sortedShots = computed(() =>
    [...shotStates.entries()].sort(([a], [b]) => a - b).map(([, state]) => state),
  );

  const progressPercent = computed(() => {
    if (totalShots.value === 0) return 0;
    return Math.round((shotSuccessCount.value / totalShots.value) * 100);
  });

  const phaseLabel = computed(() => {
    switch (currentPhase.value) {
      case "planning":
        return "意图解析";
      case "submitting":
        return "提交生成";
      case "polling":
        return "等待生成";
      case "importing":
        return "导入资产";
      case "composing":
        return "合成作品";
      case "completed":
        return "已完成";
      case "failed":
        return "失败";
      default:
        return "空闲";
    }
  });

  // ── Event handler ──
  function handleEvent(event: RuntimeEvent) {
    switch (event.type) {
      case "run_started": {
        currentRunId.value = event.run_id;
        totalShots.value = event.total_shots;
        shotSuccessCount.value = 0;
        durationSecs.value = 0;
        outputAssetId.value = null;
        finalStatus.value = null;
        isRunning.value = true;
        currentPhase.value = "submitting";
        shotStates.clear();
        logEntries.length = 0;

        // Initialize all shots as pending
        for (let i = 1; i <= event.total_shots; i++) {
          shotStates.set(i, {
            shotIndex: i,
            status: "pending",
            artifactId: null,
            submissionId: null,
            provider: null,
            model: null,
            message: "等待中",
          });
        }
        addLog("submitting", `流水线启动，共 ${event.total_shots} 个镜头`);
        break;
      }

      case "phase_changed": {
        currentPhase.value = event.phase;
        addLog(event.phase, event.message);
        break;
      }

      case "shot_updated": {
        const existing = shotStates.get(event.shot_index);
        if (existing) {
          existing.status = event.status;
          existing.artifactId = event.artifact_id;
          existing.message = event.message;
        } else {
          shotStates.set(event.shot_index, {
            shotIndex: event.shot_index,
            status: event.status,
            artifactId: event.artifact_id,
            submissionId: null,
            provider: null,
            model: null,
            message: event.message,
          });
        }

        if (event.status === "imported" || event.status === "composed") {
          shotSuccessCount.value = [...shotStates.values()].filter(
            (s) => s.status === "imported" || s.status === "composed",
          ).length;
        }

        addLog(currentPhase.value ?? "unknown", `镜头 ${event.shot_index}: ${event.message}`);
        break;
      }

      case "submission_updated": {
        const shot = shotStates.get(event.shot_index);
        if (shot) {
          shot.submissionId = event.submission_id;
          shot.provider = event.provider;
          shot.model = event.model;
          shot.status = "submitting";
          shot.message = `已提交 → ${event.provider}/${event.model}`;
        }
        addLog("submitting", `镜头 ${event.shot_index} 已提交: ${event.provider}/${event.model}`);
        break;
      }

      case "run_completed": {
        isRunning.value = false;
        currentPhase.value = event.status === "failed" ? "failed" : "completed";
        durationSecs.value = event.duration_secs;
        outputAssetId.value = event.output_asset_id;
        finalStatus.value = event.status;
        shotSuccessCount.value = event.shot_success_count;

        addLog(
          currentPhase.value,
          `流水线结束: ${event.shot_success_count}/${event.shot_total_count} 成功, ${event.duration_secs.toFixed(1)}s`,
        );
        break;
      }
    }
  }

  function addLog(phase: RuntimePhase | "unknown", message: string) {
    logEntries.push({ timestamp: Date.now(), phase, message });
    // Keep last 200 entries
    if (logEntries.length > 200) {
      logEntries.splice(0, logEntries.length - 200);
    }
  }

  /**
   * 重置所有状态（用于新建创作流程前清理）。
   */
  function reset() {
    currentRunId.value = null;
    currentPhase.value = null;
    isRunning.value = false;
    totalShots.value = 0;
    shotSuccessCount.value = 0;
    durationSecs.value = 0;
    outputAssetId.value = null;
    finalStatus.value = null;
    shotStates.clear();
    logEntries.length = 0;
  }

  // ── Lifecycle ──
  let unlisten: UnlistenFn | null = null;

  onMounted(async () => {
    try {
      unlisten = await listen<unknown>(RUNTIME_EVENT_CHANNEL, (event) => {
        const parsed = parseRuntimeEvent(event.payload);
        if (parsed) {
          handleEvent(parsed);
        }
      });
    } catch (error) {
      console.error("[useCreativeRuntime] Failed to register listener:", error);
    }
  });

  onUnmounted(() => {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  });

  return {
    // State (readonly for consumers)
    currentRunId,
    currentPhase,
    isRunning,
    totalShots,
    shotSuccessCount,
    durationSecs,
    outputAssetId,
    finalStatus,
    shotStates,
    logEntries,
    // Computed
    sortedShots,
    progressPercent,
    phaseLabel,
    // Actions
    reset,
  };
}
