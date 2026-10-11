import vue from "@vitejs/plugin-vue";
import { defineConfig, type Plugin } from "vitest/config";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

/**
 * HMR 兜底：WebSocket 连不上时改用 HTTP 轮询触发整页刷新。
 *
 * 为什么需要：本机（以及不少企业网络）走系统代理，页面本身的 HTTP 请求能过代理，
 * 但 `ws://` 的 HMR 连接过不去——结果是「页面能打开、但改了代码永远不刷新」。
 * 这个插件额外暴露一个 `GET /__dev_reload`（普通 HTTP），页面侧先探测 ws 是否可用，
 * **只有 ws 不可用时**才开始轮询，避免与 HMR 重复刷新。
 *
 * 仅在 `vite serve`（开发）下生效，生产构建不含。
 */
function devReloadFallback(): Plugin {
  let version = 0;

  const probeScript = `
(function () {
  var HMR_TIMEOUT = 2500;
  var POLL_INTERVAL = 1500;
  var badge = null;
  function paint(text) {
    if (!document.body) return;
    if (!badge) {
      badge = document.createElement("div");
      badge.id = "__dev_badge";
      badge.style.cssText =
        "position:fixed;right:8px;bottom:8px;z-index:2147483647;padding:2px 6px;" +
        "border-radius:6px;background:rgba(0,0,0,.55);color:#fff;" +
        "font:10px/1.4 ui-monospace,SFMono-Regular,Consolas,monospace;" +
        "opacity:.55;pointer-events:none";
      document.body.appendChild(badge);
    }
    badge.textContent = text;
  }
  function wsReachable() {
    return new Promise(function (resolve) {
      var settled = false;
      function done(ok) { if (!settled) { settled = true; resolve(ok); } }
      try {
        var proto = location.protocol === "https:" ? "wss://" : "ws://";
        var ws = new WebSocket(proto + location.host + "/", "vite-hmr");
        ws.onopen = function () { done(true); try { ws.close(); } catch (e) {} };
        ws.onerror = function () { done(false); };
        ws.onclose = function () { done(false); };
        setTimeout(function () { done(false); }, HMR_TIMEOUT);
      } catch (e) { done(false); }
    });
  }
  function startPolling() {
    var seen = null;
    function tick() {
      fetch("/__dev_reload", { cache: "no-store" })
        .then(function (r) { return r.json(); })
        .then(function (data) {
          if (seen === null) { seen = data.version; }
          paint("dev · poll v" + data.version);
          if (data.version !== seen) { location.reload(); }
        })
        .catch(function () {});
    }
    setInterval(tick, POLL_INTERVAL);
    tick();
    console.info("[dev] HMR 不可用，已切换到 HTTP 轮询刷新。");
  }
  wsReachable().then(function (ok) {
    if (ok) {
      paint("dev · hmr");
    } else {
      startPolling();
    }
  });
})();
`;

  return {
    name: "open-aigc-dev-reload-fallback",
    apply: "serve",
    configureServer(server) {
      const bump = (): void => {
        version += 1;
      };
      server.watcher.on("add", bump);
      server.watcher.on("change", bump);
      server.watcher.on("unlink", bump);
      server.middlewares.use((req, res, next) => {
        if (req.url && req.url.startsWith("/__dev_reload")) {
          res.setHeader("Content-Type", "application/json");
          res.setHeader("Cache-Control", "no-store");
          res.end(JSON.stringify({ version }));
          return;
        }
        next();
      });
    },
    transformIndexHtml(html) {
      return html.replace("</body>", `<script>${probeScript}</script></body>`);
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue(), devReloadFallback()],

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
