import type { Component } from "vue";
import { BookOpenText, Images, Settings, Sparkles } from "@lucide/vue";

/**
 * 导航模块定义。
 *
 * 仅保留仍需要作为模块元数据存在的顶级入口。
 * 管理类功能（班级、学生、项目、AI 供应商、账号池、提示词库、教学资源、
 * 插件、备份、管理端）已收敛到「管理中心」(`/management`)，不再单独出现在主导航。
 *
 * AppShell 使用 Codex 风格侧栏：上方保留资源库、提示词库、管理中心；
 * 概览由管理中心 tab（`/management?tab=overview`）承载，默认工作区为创意工坊（`/generations`）。
 *
 * 旧路由 `/classrooms` 等仍保留在 router.ts 中作为重定向，兼容书签与深链接。
 */
export type NavigationGroupId = "teaching" | "ai" | "content" | "system";

export interface ModuleDefinition {
  description: string;
  emptyDescription: string;
  emptyTitle: string;
  group: NavigationGroupId;
  icon: Component;
  label: string;
  path: string;
}

export const navigationGroupLabels: Record<NavigationGroupId, string> = {
  teaching: "教学管理",
  ai: "AI 工作流",
  content: "内容与知识",
  system: "系统",
};

export const moduleDefinitions: ModuleDefinition[] = [
  {
    path: "/generations",
    label: "创意工坊",
    group: "ai",
    icon: Sparkles,
    description: "对话式 AI 创作工坊",
    emptyTitle: "开始你的第一次创作",
    emptyDescription: "在下方输入提示词，AI 将为你生成作品。",
  },
  {
    path: "/assets",
    label: "资源库",
    group: "content",
    icon: Images,
    description: "输入素材、生成结果与提交作品",
    emptyTitle: "暂无资产",
    emptyDescription: "导入和生成的受管文件将在这里显示。",
  },
  {
    path: "/prompts",
    label: "提示词库",
    group: "content",
    icon: BookOpenText,
    description: "提示词模板、变量与版本",
    emptyTitle: "暂无提示词",
    emptyDescription: "可复用提示词和版本将在这里集中管理。",
  },
  {
    path: "/settings",
    label: "设置",
    group: "system",
    icon: Settings,
    description: "外观与本机偏好",
    emptyTitle: "",
    emptyDescription: "",
  },
];
