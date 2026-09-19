import { z } from "zod";

import { invokeNative, voidResponse } from "./native";

// ── Schemas ──────────────────────────────────────────

const skillMetaSchema = z
  .object({
    slug: z.string(),
    name: z.string(),
    description: z.string(),
    source: z.string(),
    installedAt: z.string(),
  })
  .passthrough();

const skillCandidateSchema = z
  .object({
    name: z.string(),
    description: z.string(),
    path: z.string(),
  })
  .passthrough();

const mcpServerConfigSchema = z
  .object({
    id: z.string(),
    name: z.string(),
    command: z.string(),
    args: z.array(z.string()),
    env: z.record(z.string(), z.string()),
    enabled: z.boolean(),
  })
  .passthrough();

const mcpScanCandidateSchema = z
  .object({
    name: z.string(),
    command: z.string(),
    args: z.array(z.string()),
    env: z.record(z.string(), z.string()),
    sourcePath: z.string(),
  })
  .passthrough();

const mcpToolInfoSchema = z
  .object({
    name: z.string(),
    description: z.string(),
  })
  .passthrough();

const skillMetaListSchema = z.array(skillMetaSchema);
const skillCandidateListSchema = z.array(skillCandidateSchema);
const mcpServerListSchema = z.array(mcpServerConfigSchema);
const mcpScanCandidateListSchema = z.array(mcpScanCandidateSchema);
const mcpToolListSchema = z.array(mcpToolInfoSchema);

// ── Types ────────────────────────────────────────────

export type SkillMeta = z.infer<typeof skillMetaSchema>;
export type SkillCandidate = z.infer<typeof skillCandidateSchema>;
export type McpServerConfig = z.infer<typeof mcpServerConfigSchema>;
export type McpScanCandidate = z.infer<typeof mcpScanCandidateSchema>;
export type McpToolInfo = z.infer<typeof mcpToolInfoSchema>;

// ── 技能 IPC ─────────────────────────────────────────

export async function listSkills(): Promise<SkillMeta[]> {
  return invokeNative("skill_v1_list", skillMetaListSchema, {});
}

export async function scanLocalSkills(dir: string): Promise<SkillCandidate[]> {
  return invokeNative("skill_v1_scan_local", skillCandidateListSchema, {
    request: { dir },
  });
}

export async function importSkill(path: string): Promise<SkillMeta> {
  return invokeNative("skill_v1_import", skillMetaSchema, { request: { path } });
}

export async function downloadSkill(url: string): Promise<SkillMeta> {
  return invokeNative("skill_v1_download", skillMetaSchema, { request: { url } });
}

export async function deleteSkill(slug: string): Promise<void> {
  await invokeNative("skill_v1_delete", voidResponse, { request: { slug } });
}

export async function getSkillBody(slug: string): Promise<string> {
  return invokeNative("skill_v1_get_body", z.string(), { request: { slug } });
}

// ── MCP IPC ──────────────────────────────────────────

export async function listMcpServers(): Promise<McpServerConfig[]> {
  return invokeNative("mcp_v1_list", mcpServerListSchema, {});
}

export async function addMcpServer(input: {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
}): Promise<McpServerConfig> {
  return invokeNative("mcp_v1_add", mcpServerConfigSchema, {
    request: {
      args: [],
      env: {},
      enabled: true,
      ...input,
    },
  });
}

export async function updateMcpServer(server: McpServerConfig): Promise<void> {
  await invokeNative("mcp_v1_update", voidResponse, { request: { server } });
}

export async function removeMcpServer(id: string): Promise<void> {
  await invokeNative("mcp_v1_remove", voidResponse, { request: { id } });
}

export async function toggleMcpServer(id: string, enabled: boolean): Promise<void> {
  await invokeNative("mcp_v1_toggle", voidResponse, { request: { id, enabled } });
}

export async function scanLocalMcp(): Promise<McpScanCandidate[]> {
  return invokeNative("mcp_v1_scan_local", mcpScanCandidateListSchema, {});
}

export async function probeMcpServer(id: string): Promise<{ tools: McpToolInfo[] }> {
  return invokeNative("mcp_v1_probe", z.object({ tools: mcpToolListSchema }).passthrough(), {
    request: { id },
  });
}
