import { computed, ref, watch } from "vue";

import { useWorkspaceStore } from "../../../app/stores/workspace";
import { submitTask, type GenerationTaskRecord } from "../../../bridge/generations";
import { getWorkspaceStatus } from "../../../bridge/workspace";
import {
  mangaV1CreateProject,
  mangaV1ListProjects,
  mangaV1ListScenes,
  mangaV1ListShots,
  mangaV1CreateScene,
  mangaV1CreateShot,
  mangaV1CreateCharacter,
  mangaV1ListCharacters,
  type MangaProject,
  type Scene,
  type Shot,
  type CharacterProfile,
} from "../../../bridge/manga";
import {
  queueV1SubmitAttempt,
  queueV1ListAttempts,
  queueV1CancelAttempt,
  queueV1RetryAttempt,
  type GenerationAttempt,
} from "../../../bridge/queue";

export type ProjectPhase = "idle" | "loading" | "ready" | "error";

export interface ShotGenerationOptions {
  credentialId: string;
  providerId: string;
  providerName: string;
  modelName: string;
  capability?: string;
}

/**
 * 管理 AI 漫剧项目的生命周期：项目列表、选中项目、场景/镜头、生成任务。
 */
export function useMangaProject() {
  const workspace = useWorkspaceStore();

  const phase = ref<ProjectPhase>("idle");
  const projects = ref<MangaProject[]>([]);
  const currentProject = ref<MangaProject | null>(null);
  const scenes = ref<Scene[]>([]);
  const shots = ref<Shot[]>([]);
  const characters = ref<CharacterProfile[]>([]);
  const errorMessage = ref<string | undefined>(undefined);

  // 生成任务相关
  const selectedShotId = ref<string | null>(null);
  const shotAttempts = ref<GenerationAttempt[]>([]);
  const generating = ref(false);

  const hasProjects = computed(() => projects.value.length > 0);
  const isInProject = computed(() => currentProject.value !== null);

  async function getWorkspaceId(): Promise<string | null> {
    try {
      const status = await getWorkspaceStatus();
      return status.workspace?.workspaceId ?? null;
    } catch {
      return null;
    }
  }

  async function loadProjects(): Promise<void> {
    const wsId = await getWorkspaceId();
    if (!wsId) return;

    phase.value = "loading";
    try {
      projects.value = await mangaV1ListProjects(wsId);
      phase.value = "ready";
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "加载项目失败";
      phase.value = "error";
    }
  }

  async function createProject(title: string, theme?: string): Promise<MangaProject | null> {
    const wsId = await getWorkspaceId();
    if (!wsId) return null;

    try {
      const project = await mangaV1CreateProject({
        workspaceId: wsId,
        title,
        theme,
      });
      projects.value = [project, ...projects.value];
      return project;
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "创建项目失败";
      return null;
    }
  }

  async function selectProject(project: MangaProject): Promise<void> {
    currentProject.value = project;
    await loadProjectData(project.id);
  }

  async function loadProjectData(projectId: string): Promise<void> {
    try {
      const [loadedScenes, loadedCharacters] = await Promise.all([
        mangaV1ListScenes(projectId),
        mangaV1ListCharacters(projectId),
      ]);
      scenes.value = loadedScenes;
      characters.value = loadedCharacters;

      // Load shots for all scenes
      const allShots: Shot[] = [];
      for (const scene of loadedScenes) {
        const sceneShots = await mangaV1ListShots(scene.id);
        allShots.push(...sceneShots);
      }
      shots.value = allShots;

      // Auto-select first shot
      if (allShots.length > 0) {
        selectedShotId.value = allShots[0].id;
        await loadShotAttempts(allShots[0].id);
      }
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "加载项目数据失败";
    }
  }

  function leaveProject(): void {
    currentProject.value = null;
    scenes.value = [];
    shots.value = [];
    characters.value = [];
    selectedShotId.value = null;
    shotAttempts.value = [];
  }

  async function selectShot(shot: Shot): Promise<void> {
    selectedShotId.value = shot.id;
    await loadShotAttempts(shot.id);
  }

  async function loadShotAttempts(shotId: string): Promise<void> {
    try {
      shotAttempts.value = await queueV1ListAttempts(shotId);
    } catch {
      shotAttempts.value = [];
    }
  }

  async function addScene(title: string, index: number): Promise<Scene | null> {
    if (!currentProject.value) return null;

    try {
      const scene = await mangaV1CreateScene({
        projectId: currentProject.value.id,
        index,
        title,
      });
      scenes.value = [...scenes.value, scene];
      return scene;
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "创建场景失败";
      return null;
    }
  }

  async function addShot(sceneId: string, index: number, prompt?: string): Promise<Shot | null> {
    try {
      const shot = await mangaV1CreateShot({
        sceneId,
        index,
        prompt,
      });
      shots.value = [...shots.value, shot];
      return shot;
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "创建镜头失败";
      return null;
    }
  }

  async function addCharacter(
    name: string,
    role?: string,
    appearance?: string,
  ): Promise<CharacterProfile | null> {
    if (!currentProject.value) return null;

    try {
      const character = await mangaV1CreateCharacter({
        projectId: currentProject.value.id,
        name,
        role,
        appearance,
      });
      characters.value = [...characters.value, character];
      return character;
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "创建角色失败";
      return null;
    }
  }

  /**
   * 为镜头创建生成任务。
   */
  async function generateForShot(
    shotId: string,
    prompt: string,
    options: ShotGenerationOptions,
  ): Promise<GenerationAttempt | null> {
    generating.value = true;
    try {
      const task: GenerationTaskRecord = await submitTask({
        providerName: options.providerName,
        modelName: options.modelName,
        promptText: prompt,
      });
      const attempt = await queueV1SubmitAttempt({
        taskId: task.id,
        credentialId: options.credentialId,
        capability: options.capability ?? "text-to-image",
        providerId: options.providerId,
        requestSnapshotJson: JSON.stringify({
          model: options.modelName,
          provider: options.providerName,
          prompt,
          shotId,
          projectId: currentProject.value?.id,
        }),
      });
      shotAttempts.value = [...shotAttempts.value, attempt];
      return attempt;
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "创建生成任务失败";
      return null;
    } finally {
      generating.value = false;
    }
  }

  /**
   * 取消生成任务。
   */
  async function cancelAttempt(attemptId: string): Promise<void> {
    try {
      await queueV1CancelAttempt(attemptId);
      if (selectedShotId.value) {
        await loadShotAttempts(selectedShotId.value);
      }
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "取消任务失败";
    }
  }

  /**
   * 重试失败的生成任务。
   */
  async function retryAttempt(attemptId: string): Promise<void> {
    try {
      await queueV1RetryAttempt(attemptId);
      if (selectedShotId.value) {
        await loadShotAttempts(selectedShotId.value);
      }
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : "重试任务失败";
    }
  }

  function clearError(): void {
    errorMessage.value = undefined;
  }

  // Auto-load when workspace is ready
  watch(
    () => workspace.isReady,
    (ready) => {
      if (ready) {
        loadProjects();
      }
    },
    { immediate: true },
  );

  return {
    phase,
    projects,
    currentProject,
    scenes,
    shots,
    characters,
    errorMessage,
    selectedShotId,
    shotAttempts,
    generating,
    hasProjects,
    isInProject,
    loadProjects,
    createProject,
    selectProject,
    loadProjectData,
    leaveProject,
    selectShot,
    addScene,
    addShot,
    addCharacter,
    generateForShot,
    cancelAttempt,
    retryAttempt,
    clearError,
  };
}
