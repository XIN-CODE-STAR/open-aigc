import type { RouteRecordRaw } from "vue-router";
import { Inbox, ListTodo } from "@lucide/vue";
import { createRouter, createWebHashHistory } from "vue-router";

// Lazy-loaded page components for code splitting
const AssetsPage = () => import("../modules/assets/pages/AssetsPage.vue");
const GenerationsPage = () => import("../modules/generations/pages/GenerationsPage.vue");
const PromptsPage = () => import("../modules/prompts/pages/PromptsPage.vue");
const SettingsPage = () => import("../modules/settings/pages/SettingsPage.vue");
const BackupPage = () => import("../modules/backup/pages/BackupPage.vue");
const ModelsPage = () => import("../modules/models/pages/ModelsPage.vue");
const ModuleLandingPage = () => import("./pages/ModuleLandingPage.vue");

// OPEN AIGC 仅保留 Agent 创作相关功能：
// 创意工坊（/generations）为默认工作区，资源库、提示词库、备份、设置为独立顶级入口。
// 登录与管理中心（班级/学生/管理端）已随教学/管理功能一并移除。

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    redirect: "/generations",
  },
  {
    path: "/assets",
    component: AssetsPage,
    meta: { title: "资产库" },
  },
  {
    path: "/generations",
    component: GenerationsPage,
    meta: { title: "创意工坊" },
  },
  {
    path: "/prompts",
    component: PromptsPage,
    meta: { title: "提示词库" },
  },
  {
    path: "/backup",
    component: BackupPage,
    meta: { title: "备份" },
  },
  {
    path: "/models",
    component: ModelsPage,
    meta: { title: "模型管理" },
  },
  {
    path: "/tasks",
    component: ModuleLandingPage,
    meta: { title: "任务中心" },
    props: {
      description: "后台任务与处理状态",
      emptyDescription: "生成、导入、下载和导出任务会在这里集中显示。",
      emptyTitle: "暂无后台任务",
      icon: ListTodo,
    },
  },
  {
    path: "/notifications",
    component: ModuleLandingPage,
    meta: { title: "通知" },
    props: {
      description: "需要关注的系统消息",
      emptyDescription: "任务异常与系统状态消息会在这里显示。",
      emptyTitle: "暂无通知",
      icon: Inbox,
    },
  },
  {
    path: "/settings",
    component: SettingsPage,
    meta: { title: "设置" },
  },
  {
    path: "/:pathMatch(.*)*",
    redirect: "/generations",
  },
];

export function createAppRouter() {
  const router = createRouter({
    history: createWebHashHistory(),
    routes,
    scrollBehavior: () => ({ left: 0, top: 0 }),
  });

  return router;
}

export const router = createAppRouter();
