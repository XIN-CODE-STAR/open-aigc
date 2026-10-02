import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. ignore native build output and local QA artifacts
      // .mimosa：安全扫描器 hook-state 基线文件被独占锁定，watch 会以 EBUSY 崩溃
      // services/jimeng-free-api-all*：即梦代理运行时（数千个第三方文件），
      //   纳入 watch 会撑爆 FSWatcher 导致前端服务崩溃（10/2 复盘）
      ignored: [
        "**/src-tauri/**",
        "**/work/**",
        "**/.mimosa/**",
        "**/services/jimeng-free-api-all*/**",
      ],
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test/setup.ts"],
  },
}));
