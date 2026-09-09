import { z } from "zod";

import { invokeNative } from "./native";

// ── Schemas ──

const mangaProjectStatusSchema = z.enum(["draft", "in-progress", "completed", "archived"]);
const shotStatusSchema = z.enum(["draft", "ready", "generating", "completed", "failed"]);

const mangaProjectSchema = z
  .object({
    id: z.string(),
    workspaceId: z.string(),
    classroomId: z.string().nullable(),
    title: z.string(),
    theme: z.string().nullable(),
    teachingGoal: z.string().nullable(),
    status: mangaProjectStatusSchema,
    revision: z.number().int(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

const sceneSchema = z
  .object({
    id: z.string(),
    projectId: z.string(),
    index: z.number().int(),
    title: z.string(),
    summary: z.string().nullable(),
    location: z.string().nullable(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

const shotSchema = z
  .object({
    id: z.string(),
    sceneId: z.string(),
    index: z.number().int(),
    shotType: z.string().nullable(),
    cameraMotion: z.string().nullable(),
    duration: z.string().nullable(),
    prompt: z.string().nullable(),
    negativePrompt: z.string().nullable(),
    status: shotStatusSchema,
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

const characterSchema = z
  .object({
    id: z.string(),
    projectId: z.string(),
    name: z.string(),
    role: z.string().nullable(),
    appearance: z.string().nullable(),
    personality: z.string().nullable(),
    consistencyPrompt: z.string().nullable(),
    version: z.number().int(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

const storyBibleSchema = z
  .object({
    id: z.string(),
    projectId: z.string(),
    logline: z.string().nullable(),
    synopsis: z.string().nullable(),
    styleGuide: z.string().nullable(),
    visualStyle: z.string().nullable(),
    tone: z.string().nullable(),
    version: z.number().int(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

// ── Types ──

export type MangaProjectStatus = z.infer<typeof mangaProjectStatusSchema>;
export type ShotStatus = z.infer<typeof shotStatusSchema>;
export type MangaProject = z.infer<typeof mangaProjectSchema>;
export type Scene = z.infer<typeof sceneSchema>;
export type Shot = z.infer<typeof shotSchema>;
export type CharacterProfile = z.infer<typeof characterSchema>;
export type StoryBible = z.infer<typeof storyBibleSchema>;

// ── IPC functions ──

export async function mangaV1CreateProject(input: {
  workspaceId: string;
  title: string;
  classroomId?: string;
  theme?: string;
  teachingGoal?: string;
}): Promise<MangaProject> {
  return invokeNative("manga_v1_create_project", mangaProjectSchema, { ...input });
}

export async function mangaV1ListProjects(workspaceId: string): Promise<MangaProject[]> {
  return invokeNative("manga_v1_list_projects", z.array(mangaProjectSchema), {
    workspaceId,
  });
}

export async function mangaV1GetProject(id: string): Promise<MangaProject | null> {
  return invokeNative("manga_v1_get_project", mangaProjectSchema.nullable(), { id });
}

export async function mangaV1DeleteProject(id: string): Promise<void> {
  await invokeNative("manga_v1_delete_project", z.null(), { id });
}

export async function mangaV1CreateScene(input: {
  projectId: string;
  index: number;
  title: string;
  summary?: string;
  location?: string;
}): Promise<Scene> {
  return invokeNative("manga_v1_create_scene", sceneSchema, { ...input });
}

export async function mangaV1ListScenes(projectId: string): Promise<Scene[]> {
  return invokeNative("manga_v1_list_scenes", z.array(sceneSchema), { projectId });
}

export async function mangaV1CreateShot(input: {
  sceneId: string;
  index: number;
  shotType?: string;
  cameraMotion?: string;
  duration?: string;
  prompt?: string;
  negativePrompt?: string;
}): Promise<Shot> {
  return invokeNative("manga_v1_create_shot", shotSchema, { ...input });
}

export async function mangaV1ListShots(sceneId: string): Promise<Shot[]> {
  return invokeNative("manga_v1_list_shots", z.array(shotSchema), { sceneId });
}

export async function mangaV1CreateCharacter(input: {
  projectId: string;
  name: string;
  role?: string;
  appearance?: string;
  personality?: string;
  consistencyPrompt?: string;
}): Promise<CharacterProfile> {
  return invokeNative("manga_v1_create_character", characterSchema, { ...input });
}

export async function mangaV1ListCharacters(projectId: string): Promise<CharacterProfile[]> {
  return invokeNative("manga_v1_list_characters", z.array(characterSchema), { projectId });
}

export async function mangaV1UpsertStoryBible(input: {
  projectId: string;
  logline?: string;
  synopsis?: string;
  styleGuide?: string;
  visualStyle?: string;
  tone?: string;
}): Promise<StoryBible> {
  return invokeNative("manga_v1_upsert_story_bible", storyBibleSchema, { ...input });
}
