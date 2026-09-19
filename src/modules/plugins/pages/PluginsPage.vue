<script setup lang="ts">
/**
 * PluginsPage：插件中心。
 *
 * - 技能：列出/扫描本地目录/导入（目录或 .md）/URL 下载（.md、zip、GitHub 仓库），
 *   安装到工作区 skills/ 目录；Agent 通过 list_skills / use_skill 自助调用。
 * - MCP 服务器：扫描本机主流配置（Claude / Cursor / VS Code 等）、手动添加、
 *   启停、探测工具清单；Agent 通过 mcp_list_tools / mcp_call 调用。
 */
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  Download,
  FolderInput,
  LoaderCircle,
  Puzzle,
  Radar,
  RefreshCw,
  Trash2,
  Wrench,
} from "@lucide/vue";

import { useToast } from "../../../shared/ui/useToast";
import {
  addMcpServer,
  deleteSkill,
  downloadSkill,
  getSkillBody,
  importSkill,
  listMcpServers,
  listSkills,
  probeMcpServer,
  removeMcpServer,
  scanLocalMcp,
  scanLocalSkills,
  toggleMcpServer,
  updateMcpServer,
  type McpScanCandidate,
  type McpServerConfig,
  type McpToolInfo,
  type SkillCandidate,
  type SkillMeta,
} from "../../../bridge/plugins";

const toast = useToast();

type TabKey = "skills" | "mcp";
const activeTab = ref<TabKey>("skills");
const busy = ref(false);

// ── 技能 ──

const skills = ref<SkillMeta[]>([]);
const scannedSkills = ref<SkillCandidate[]>([]);
const importingAllSkills = ref(false);
const skillBodyPreview = ref<{ slug: string; body: string } | null>(null);
const downloadUrl = ref("");
const confirmingDeleteSlug = ref<string | null>(null);

async function refreshSkills(): Promise<void> {
  busy.value = true;
  try {
    skills.value = await listSkills();
  } catch (e) {
    toast.error(`技能列表加载失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function pickDirectory(): Promise<string | null> {
  const response = await invoke<string | string[] | null>("plugin:dialog|open", {
    options: { directory: true, title: "选择要扫描的目录" },
  });
  return typeof response === "string" ? response : null;
}

async function pickSkillFile(): Promise<string | null> {
  const response = await invoke<string | string[] | null>("plugin:dialog|open", {
    options: {
      title: "选择技能目录或 SKILL.md / .md 文件",
      filters: [
        { name: "技能", extensions: ["md"] },
        { name: "所有文件", extensions: ["*"] },
      ],
    },
  });
  return typeof response === "string" ? response : null;
}

async function scanSkills(): Promise<void> {
  const dir = await pickDirectory();
  if (!dir) return;
  busy.value = true;
  try {
    scannedSkills.value = await scanLocalSkills(dir);
    toast.success(
      scannedSkills.value.length > 0
        ? `扫描到 ${scannedSkills.value.length} 个技能目录。`
        : "该目录下未发现技能（需包含 SKILL.md）。",
    );
  } catch (e) {
    toast.error(`扫描失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

/** 一键导入全部扫描候选：逐个导入并移除，最后统一刷新与汇总。 */
async function importAllScannedSkills(): Promise<void> {
  if (importingAllSkills.value || scannedSkills.value.length === 0) return;
  importingAllSkills.value = true;
  let imported = 0;
  let failed = 0;
  const targets = [...scannedSkills.value];
  for (const candidate of targets) {
    try {
      await importSkill(candidate.path);
      scannedSkills.value = scannedSkills.value.filter((c) => c.path !== candidate.path);
      imported += 1;
    } catch {
      failed += 1;
    }
  }
  importingAllSkills.value = false;
  await refreshSkills();
  toast.success(
    failed > 0
      ? `批量导入完成：成功 ${imported} 个，失败 ${failed} 个。`
      : `已一键导入 ${imported} 个技能。`,
  );
}

async function importSkillAt(path: string, source: string): Promise<void> {
  busy.value = true;
  try {
    const meta = await importSkill(path);
    toast.success(`已导入技能「${meta.name}」。`);
    if (source === "scan") {
      scannedSkills.value = scannedSkills.value.filter((c) => c.path !== path);
    }
    await refreshSkills();
  } catch (e) {
    toast.error(`导入失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function importSkillFile(): Promise<void> {
  const path = await pickSkillFile();
  if (!path) return;
  await importSkillAt(path, "file");
}

async function downloadFromUrl(): Promise<void> {
  const url = downloadUrl.value.trim();
  if (!url) {
    toast.warning("请输入下载地址（.md / zip / GitHub 仓库页）。");
    return;
  }
  busy.value = true;
  try {
    const meta = await downloadSkill(url);
    toast.success(`已下载安装技能「${meta.name}」。`);
    downloadUrl.value = "";
    await refreshSkills();
  } catch (e) {
    toast.error(`下载失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function viewSkill(slug: string): Promise<void> {
  if (skillBodyPreview.value?.slug === slug) {
    skillBodyPreview.value = null;
    return;
  }
  try {
    const body = await getSkillBody(slug);
    skillBodyPreview.value = { slug, body };
  } catch (e) {
    toast.error(`读取技能失败：${String(e)}`);
  }
}

async function removeSkill(slug: string): Promise<void> {
  if (confirmingDeleteSlug.value !== slug) {
    confirmingDeleteSlug.value = slug;
    setTimeout(() => {
      if (confirmingDeleteSlug.value === slug) confirmingDeleteSlug.value = null;
    }, 3000);
    return;
  }
  confirmingDeleteSlug.value = null;
  try {
    await deleteSkill(slug);
    toast.success("技能已删除。");
    if (skillBodyPreview.value?.slug === slug) skillBodyPreview.value = null;
    await refreshSkills();
  } catch (e) {
    toast.error(`删除失败：${String(e)}`);
  }
}

// ── MCP 服务器 ──

const servers = ref<McpServerConfig[]>([]);
const scannedMcp = ref<McpScanCandidate[]>([]);
const importingAllMcp = ref(false);
const probingId = ref<string | null>(null);
const probeTools = ref<Record<string, McpToolInfo[]>>({});
const editingId = ref<string | null>(null);

const formName = ref("");
const formCommand = ref("");
const formArgs = ref("");
const formEnv = ref("");
const formEnabled = ref(true);

const formValid = computed(() => formName.value.trim() !== "" && formCommand.value.trim() !== "");

function parseArgsText(text: string): string[] {
  return text
    .split("\n")
    .flatMap((line) => line.split(" "))
    .map((token) => token.trim())
    .filter((token) => token.length > 0);
}

function parseEnvText(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const separator = trimmed.indexOf("=");
    if (separator <= 0) continue;
    env[trimmed.slice(0, separator).trim()] = trimmed.slice(separator + 1).trim();
  }
  return env;
}

function formatArgs(args: string[]): string {
  return args.join(" ");
}

function formatEnv(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

function resetForm(): void {
  formName.value = "";
  formCommand.value = "";
  formArgs.value = "";
  formEnv.value = "";
  formEnabled.value = true;
  editingId.value = null;
}

function startEdit(server: McpServerConfig): void {
  editingId.value = server.id;
  formName.value = server.name;
  formCommand.value = server.command;
  formArgs.value = formatArgs(server.args);
  formEnv.value = formatEnv(server.env);
  formEnabled.value = server.enabled;
}

async function refreshServers(): Promise<void> {
  busy.value = true;
  try {
    servers.value = await listMcpServers();
    probeTools.value = {};
  } catch (e) {
    toast.error(`MCP 列表加载失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function submitMcpForm(): Promise<void> {
  if (!formValid.value) {
    toast.warning("名称与启动命令不能为空。");
    return;
  }
  busy.value = true;
  try {
    if (editingId.value) {
      const current = servers.value.find((s) => s.id === editingId.value);
      if (current) {
        await updateMcpServer({
          id: current.id,
          name: formName.value.trim(),
          command: formCommand.value.trim(),
          args: parseArgsText(formArgs.value),
          env: parseEnvText(formEnv.value),
          enabled: formEnabled.value,
        });
        toast.success("MCP 服务器已更新。");
      }
    } else {
      await addMcpServer({
        name: formName.value.trim(),
        command: formCommand.value.trim(),
        args: parseArgsText(formArgs.value),
        env: parseEnvText(formEnv.value),
        enabled: formEnabled.value,
      });
      toast.success("MCP 服务器已添加。");
    }
    resetForm();
    await refreshServers();
  } catch (e) {
    toast.error(`保存失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function scanMcpConfigs(): Promise<void> {
  busy.value = true;
  try {
    scannedMcp.value = await scanLocalMcp();
    toast.success(
      scannedMcp.value.length > 0
        ? `在本机配置中发现 ${scannedMcp.value.length} 个 MCP 服务器。`
        : "未在本机常见位置发现 MCP 配置。",
    );
  } catch (e) {
    toast.error(`扫描失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

async function importMcpCandidate(candidate: McpScanCandidate): Promise<void> {
  busy.value = true;
  try {
    await addMcpServer({
      name: candidate.name,
      command: candidate.command,
      args: candidate.args,
      env: candidate.env,
      enabled: true,
    });
    scannedMcp.value = scannedMcp.value.filter(
      (c) => !(c.name === candidate.name && c.command === candidate.command),
    );
    toast.success(`已添加 MCP 服务器「${candidate.name}」。`);
    await refreshServers();
  } catch (e) {
    toast.error(`添加失败：${String(e)}`);
  } finally {
    busy.value = false;
  }
}

/** 一键添加全部扫描到的 MCP 服务器（同名冲突计为跳过）。 */
async function importAllMcpCandidates(): Promise<void> {
  if (importingAllMcp.value || scannedMcp.value.length === 0) return;
  importingAllMcp.value = true;
  let added = 0;
  let failed = 0;
  const targets = [...scannedMcp.value];
  for (const candidate of targets) {
    try {
      await addMcpServer({
        name: candidate.name,
        command: candidate.command,
        args: candidate.args,
        env: candidate.env,
        enabled: true,
      });
      scannedMcp.value = scannedMcp.value.filter(
        (c) => !(c.name === candidate.name && c.command === candidate.command),
      );
      added += 1;
    } catch {
      failed += 1;
    }
  }
  importingAllMcp.value = false;
  await refreshServers();
  toast.success(
    failed > 0
      ? `批量添加完成：成功 ${added} 个，跳过/失败 ${failed} 个（同名已存在）。`
      : `已一键添加 ${added} 个 MCP 服务器。`,
  );
}

async function toggleServer(server: McpServerConfig): Promise<void> {
  try {
    await toggleMcpServer(server.id, !server.enabled);
    await refreshServers();
  } catch (e) {
    toast.error(`切换失败：${String(e)}`);
  }
}

async function deleteServer(server: McpServerConfig): Promise<void> {
  try {
    await removeMcpServer(server.id);
    toast.success(`已删除「${server.name}」。`);
    if (editingId.value === server.id) resetForm();
    await refreshServers();
  } catch (e) {
    toast.error(`删除失败：${String(e)}`);
  }
}

async function probe(server: McpServerConfig): Promise<void> {
  probingId.value = server.id;
  try {
    const result = await probeMcpServer(server.id);
    probeTools.value = { ...probeTools.value, [server.id]: result.tools };
    toast.success(
      result.tools.length > 0
        ? `「${server.name}」提供 ${result.tools.length} 个工具。`
        : `「${server.name}」连接成功，但未提供工具。`,
    );
  } catch (e) {
    probeTools.value = { ...probeTools.value, [server.id]: [] };
    toast.error(`探测失败：${String(e)}`);
  } finally {
    probingId.value = null;
  }
}

onMounted(() => {
  void refreshSkills();
  void refreshServers();
});
</script>

<template>
  <div class="plugins-page">
    <p class="plugins-page__description">
      管理用户技能与 MCP 服务器：支持本地扫描、本地导入、URL 下载；已启用的能力会出现在对话 Agent
      的工具列表中（list_skills / use_skill / mcp_list_tools / mcp_call）。
    </p>

    <div class="plugins-tabs" role="tablist">
      <button
        class="plugins-tab"
        :class="{ 'is-active': activeTab === 'skills' }"
        type="button"
        role="tab"
        @click="activeTab = 'skills'"
      >
        <Puzzle :size="14" />
        技能（{{ skills.length }}）
      </button>
      <button
        class="plugins-tab"
        :class="{ 'is-active': activeTab === 'mcp' }"
        type="button"
        role="tab"
        @click="activeTab = 'mcp'"
      >
        <Wrench :size="14" />
        MCP 服务器（{{ servers.length }}）
      </button>
    </div>

    <!-- ── 技能 ── -->
    <section v-show="activeTab === 'skills'" class="plugins-section">
      <div class="plugins-actions">
        <button class="plugins-btn" type="button" :disabled="busy" @click="scanSkills">
          <Radar :size="14" />
          扫描本地目录
        </button>
        <button class="plugins-btn" type="button" :disabled="busy" @click="importSkillFile">
          <FolderInput :size="14" />
          导入技能
        </button>
        <span class="plugins-download">
          <input
            v-model="downloadUrl"
            class="plugins-input"
            type="text"
            placeholder="下载地址：.md / zip / GitHub 仓库页"
            @keydown.enter="downloadFromUrl"
          />
          <button
            class="plugins-btn"
            type="button"
            :disabled="busy || downloadUrl.trim() === ''"
            @click="downloadFromUrl"
          >
            <Download :size="14" />
            下载
          </button>
        </span>
        <button class="plugins-btn" type="button" :disabled="busy" @click="refreshSkills">
          <RefreshCw :size="14" />
          刷新
        </button>
      </div>

      <div v-if="scannedSkills.length > 0" class="plugin-card plugin-card--scan">
        <div class="plugin-card__header">
          <h3 class="plugin-card__title">扫描结果（{{ scannedSkills.length }}）</h3>
          <button
            class="plugins-btn plugins-btn--primary"
            type="button"
            :disabled="busy || importingAllSkills"
            @click="importAllScannedSkills"
          >
            <LoaderCircle v-if="importingAllSkills" :size="13" class="is-spinning" />
            全部导入
          </button>
        </div>
        <div v-for="candidate in scannedSkills" :key="candidate.path" class="plugin-row">
          <div class="plugin-row__main">
            <span class="plugin-row__name">{{ candidate.name }}</span>
            <span class="plugin-row__meta">{{ candidate.description || "无描述" }}</span>
            <span class="plugin-row__meta plugin-row__meta--path">{{ candidate.path }}</span>
          </div>
          <button
            class="plugins-btn"
            type="button"
            :disabled="busy"
            @click="importSkillAt(candidate.path, 'scan')"
          >
            导入
          </button>
        </div>
      </div>

      <div v-if="!busy && skills.length === 0" class="plugins-empty">
        暂无技能：从本地目录导入、选择技能文件夹，或粘贴下载地址安装。
      </div>

      <div v-for="skill in skills" :key="skill.slug" class="plugin-card">
        <div class="plugin-row">
          <div class="plugin-row__main">
            <span class="plugin-row__name">{{ skill.name }}</span>
            <span class="plugin-row__meta">{{ skill.description || "无描述" }}</span>
            <span class="plugin-row__meta plugin-row__meta--path">
              {{ skill.slug }} · 来源 {{ skill.source }}
            </span>
          </div>
          <div class="plugin-row__actions">
            <button class="plugins-btn" type="button" @click="viewSkill(skill.slug)">
              {{ skillBodyPreview?.slug === skill.slug ? "收起" : "查看" }}
            </button>
            <button
              class="plugins-btn plugins-btn--danger"
              type="button"
              @click="removeSkill(skill.slug)"
            >
              <Trash2 :size="13" />
              {{ confirmingDeleteSlug === skill.slug ? "确认删除？" : "删除" }}
            </button>
          </div>
        </div>
        <pre v-if="skillBodyPreview?.slug === skill.slug" class="plugin-body">{{
          skillBodyPreview.body
        }}</pre>
      </div>
    </section>

    <!-- ── MCP ── -->
    <section v-show="activeTab === 'mcp'" class="plugins-section">
      <div class="plugin-card">
        <h3 class="plugin-card__title">
          {{ editingId ? "编辑 MCP 服务器" : "添加 MCP 服务器（stdio）" }}
        </h3>
        <div class="mcp-form">
          <label class="mcp-form__field">
            <span>名称</span>
            <input v-model="formName" class="plugins-input" type="text" placeholder="fetch" />
          </label>
          <label class="mcp-form__field">
            <span>启动命令</span>
            <input v-model="formCommand" class="plugins-input" type="text" placeholder="npx" />
          </label>
          <label class="mcp-form__field">
            <span>参数（每行一个或空格分隔）</span>
            <textarea
              v-model="formArgs"
              class="plugins-input"
              rows="2"
              placeholder="-y mcp-fetch"
            />
          </label>
          <label class="mcp-form__field">
            <span>环境变量（每行 KEY=VALUE）</span>
            <textarea v-model="formEnv" class="plugins-input" rows="2" placeholder="API_KEY=xxx" />
          </label>
          <label class="mcp-form__check">
            <input v-model="formEnabled" type="checkbox" />
            启用（启用后 Agent 可调用其工具）
          </label>
          <div class="mcp-form__actions">
            <button
              class="plugins-btn plugins-btn--primary"
              type="button"
              :disabled="busy || !formValid"
              @click="submitMcpForm"
            >
              {{ editingId ? "保存修改" : "添加" }}
            </button>
            <button v-if="editingId" class="plugins-btn" type="button" @click="resetForm">
              取消编辑
            </button>
          </div>
        </div>
      </div>

      <div class="plugins-actions">
        <button class="plugins-btn" type="button" :disabled="busy" @click="scanMcpConfigs">
          <Radar :size="14" />
          扫描本机 MCP 配置
        </button>
        <button class="plugins-btn" type="button" :disabled="busy" @click="refreshServers">
          <RefreshCw :size="14" />
          刷新
        </button>
      </div>

      <div v-if="scannedMcp.length > 0" class="plugin-card plugin-card--scan">
        <div class="plugin-card__header">
          <h3 class="plugin-card__title">本机配置中发现（{{ scannedMcp.length }}）</h3>
          <button
            class="plugins-btn plugins-btn--primary"
            type="button"
            :disabled="busy || importingAllMcp"
            @click="importAllMcpCandidates"
          >
            <LoaderCircle v-if="importingAllMcp" :size="13" class="is-spinning" />
            全部添加
          </button>
        </div>
        <div
          v-for="candidate in scannedMcp"
          :key="candidate.sourcePath + candidate.name"
          class="plugin-row"
        >
          <div class="plugin-row__main">
            <span class="plugin-row__name">{{ candidate.name }}</span>
            <span class="plugin-row__meta">
              {{ candidate.command }} {{ formatArgs(candidate.args) }}
            </span>
            <span class="plugin-row__meta plugin-row__meta--path">{{ candidate.sourcePath }}</span>
          </div>
          <button
            class="plugins-btn"
            type="button"
            :disabled="busy"
            @click="importMcpCandidate(candidate)"
          >
            添加
          </button>
        </div>
      </div>

      <div v-if="!busy && servers.length === 0" class="plugins-empty">
        暂无 MCP 服务器：手动添加（命令 + 参数），或扫描本机已有配置导入。
      </div>

      <div v-for="server in servers" :key="server.id" class="plugin-card">
        <div class="plugin-row">
          <div class="plugin-row__main">
            <span class="plugin-row__name">
              {{ server.name }}
              <span class="plugin-badge" :class="server.enabled ? 'is-on' : 'is-off'">
                {{ server.enabled ? "已启用" : "已停用" }}
              </span>
            </span>
            <span class="plugin-row__meta">
              {{ server.command }} {{ formatArgs(server.args) }}
            </span>
          </div>
          <div class="plugin-row__actions">
            <button
              class="plugins-btn"
              type="button"
              :disabled="probingId === server.id"
              @click="probe(server)"
            >
              <LoaderCircle v-if="probingId === server.id" :size="13" class="is-spinning" />
              <Radar v-else :size="13" />
              探测
            </button>
            <button class="plugins-btn" type="button" @click="toggleServer(server)">
              {{ server.enabled ? "停用" : "启用" }}
            </button>
            <button class="plugins-btn" type="button" @click="startEdit(server)">编辑</button>
            <button
              class="plugins-btn plugins-btn--danger"
              type="button"
              @click="deleteServer(server)"
            >
              <Trash2 :size="13" />
              删除
            </button>
          </div>
        </div>
        <div v-if="probeTools[server.id]" class="plugin-tools">
          <span
            v-for="tool in probeTools[server.id]"
            :key="tool.name"
            class="plugin-tools__item"
            :title="tool.description"
          >
            {{ tool.name }}
          </span>
          <span v-if="probeTools[server.id]!.length === 0" class="plugin-row__meta">
            未提供工具或探测失败。
          </span>
        </div>
      </div>
    </section>

    <div v-if="busy" class="plugins-busy">
      <LoaderCircle :size="16" class="is-spinning" />
      处理中…
    </div>
  </div>
</template>

<style scoped>
.plugins-page {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  max-width: 860px;
  padding: var(--space-4) var(--space-5);
}

.plugins-page__description {
  color: var(--color-text-secondary, var(--color-text));
  font-size: 12px;
  line-height: 1.6;
}

.plugins-tabs {
  display: inline-flex;
  gap: 4px;
  align-self: flex-start;
  padding: 3px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
}

.plugins-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 var(--space-3);
  color: var(--color-text-secondary, var(--color-text));
  border: none;
  border-radius: 7px;
  background: transparent;
  font-size: 12px;
  cursor: pointer;
}

.plugins-tab.is-active {
  color: #fff;
  background: var(--color-accent);
}

.plugins-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.plugins-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.plugins-download {
  display: inline-flex;
  flex: 1;
  min-width: 260px;
  gap: var(--space-2);
}

.plugins-input {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  color: var(--color-text);
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font-size: 12px;
  font-family: inherit;
  outline: none;
}

.plugins-input:focus {
  border-color: var(--color-accent);
}

textarea.plugins-input {
  resize: vertical;
}

.plugins-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 var(--space-3);
  color: var(--color-text-secondary, var(--color-text));
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-control);
  background: var(--color-surface-subtle);
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out);
}

.plugins-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.plugins-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.plugins-btn--primary {
  color: #fff;
  background: var(--color-accent);
  border-color: transparent;
}

.plugins-btn--primary:hover:not(:disabled) {
  color: #fff;
  filter: brightness(1.08);
  background: var(--color-accent);
}

.plugins-btn--danger:hover:not(:disabled) {
  color: #f87171;
  border-color: rgb(248 113 113 / 40%);
}

.plugin-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-border-subtle);
  border-radius: 10px;
  background: var(--color-surface-subtle);
}

.plugin-card--scan {
  border-style: dashed;
  background: transparent;
}

.plugin-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.plugin-card__title {
  color: var(--color-text);
  font-size: 12px;
  font-weight: 600;
}

.plugin-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.plugin-row + .plugin-row {
  margin-top: var(--space-2);
}

.plugin-row__main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.plugin-row__name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}

.plugin-row__meta {
  color: var(--color-text-tertiary);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plugin-row__meta--path {
  opacity: 0.75;
}

.plugin-row__actions {
  display: inline-flex;
  flex-shrink: 0;
  gap: var(--space-2);
}

.plugin-badge {
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 500;
}

.plugin-badge.is-on {
  color: #34d399;
  background: rgb(52 211 153 / 12%);
}

.plugin-badge.is-off {
  color: #94a3b8;
  background: rgb(148 163 184 / 12%);
}

.plugin-body {
  max-height: 320px;
  padding: var(--space-3);
  overflow: auto;
  border-radius: 8px;
  background: rgb(0 0 0 / 25%);
  color: var(--color-text-secondary, var(--color-text));
  font-size: 11px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}

.plugin-tools {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.plugin-tools__item {
  padding: 2px 8px;
  border: 1px solid var(--color-border-subtle);
  border-radius: 999px;
  color: var(--color-text-secondary, var(--color-text));
  font-size: 11px;
}

.mcp-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.mcp-form__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  color: var(--color-text-secondary, var(--color-text));
  font-size: 11px;
}

.mcp-form__check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text-secondary, var(--color-text));
  font-size: 12px;
}

.mcp-form__actions {
  display: flex;
  gap: var(--space-2);
}

.plugins-empty {
  padding: var(--space-5);
  border: 1px dashed var(--color-border-subtle);
  border-radius: 10px;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.plugins-busy {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.is-spinning {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
