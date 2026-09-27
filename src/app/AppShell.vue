<script setup lang="ts">
/**
 * AppShell：应用外壳（编排层）。
 *
 * 布局：左侧导航栏（app/shell/AppSidebar）+ 工作区（顶栏 + 内容路由出口）。
 * 具体的侧栏/顶栏 UI 与历史对话逻辑分别在 app/shell/ 下的子组件中。
 */
import { computed, onMounted } from "vue";
import { RouterView, useRoute } from "vue-router";

import AuroraCanvas from "../shared/visual/AuroraCanvas.vue";
import ToastViewport from "../shared/ui/ToastViewport.vue";
import AppSidebar from "./shell/AppSidebar.vue";
import AppTopbar from "./shell/AppTopbar.vue";
import { useProjectDirectory } from "./stores/projectDirectory";
import { useWorkspaceStore } from "./stores/workspace";

const route = useRoute();
const workspace = useWorkspaceStore();
const projectDir = useProjectDirectory();

/** 创意工坊（/generations）页面自带内部滚动，内容区需要零内边距与侧栏贴合。 */
const isFlushRoute = computed(() => route.path === "/generations");

onMounted(() => {
  void workspace.ensureReady();
  // 启动时恢复用户选定的工作目录，并同步到后端生成导出路径
  projectDir.restore();
});
</script>

<template>
  <div class="app-shell">
    <AppSidebar />

    <section class="app-workspace">
      <AuroraCanvas class="app-workspace__aurora" />
      <AppTopbar />
      <main class="page-scroll" :class="{ 'is-flush': isFlushRoute }" tabindex="-1">
        <RouterView />
      </main>
    </section>

    <ToastViewport />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  width: 100%;
  height: 100%;
  color: var(--color-text);
  background: var(--color-canvas);
}

.app-workspace {
  position: relative;
  display: grid;
  min-width: 0;
  flex: 1;
  grid-template-rows: var(--topbar-height) minmax(0, 1fr);
  background: var(--color-canvas);
  overflow: hidden;
}

.app-workspace__aurora {
  z-index: 0;
  grid-row: 1 / -1;
  grid-column: 1 / -1;
}

/* —— 内容滚动区 —— */
.page-scroll {
  position: relative;
  z-index: 1;
  min-width: 0;
  min-height: 0;
  padding: var(--page-padding);
  overflow: auto;
  background: transparent;
}

/* 创意工坊页面自带内部滚动与背景：去掉内容区留白，让对话与侧栏直接贴合。 */
.page-scroll.is-flush {
  padding: 0;
  overflow: hidden;
}
</style>
