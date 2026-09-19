#!/usr/bin/env node
// 下载并部署 ffmpeg sidecar，供视频合成使用（技术决策 D2：tauri externalBin 随应用分发）。
//
// 用法：
//   node scripts/fetch-ffmpeg.mjs                # 幂等：已部署且可用则跳过
//   node scripts/fetch-ffmpeg.mjs --force        # 强制重新下载覆盖
//   node scripts/fetch-ffmpeg.mjs --zip <path>   # 使用本地 zip（网络受限时手工下载后喂入）
//   node scripts/fetch-ffmpeg.mjs --url <url>    # 覆盖下载源（仅允许 http/https）
//
// 默认源：BtbN FFmpeg-Builds（GitHub Releases，GPL 构建，含 libx264）。
// 二进制不进仓库（.gitignore 的 src-tauri/binaries/）；
// 部署后 tauri dev/build 会把 sidecar 复制到可执行文件同目录并随安装包分发。
// 注意：externalBin 要求文件必须存在，否则 tauri CLI 构建报错——
// 新环境请先运行本脚本再 tauri dev。

import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const DEFAULT_URL =
  "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";

// 与仓库唯一分发目标一致（tauri.conf.json targets: nsis，MSVC 工具链）。
// non-Windows 下 tauri externalBin 使用各自 triple，本脚本仅服务 Windows 分发。
const SIDECAR_NAME = "ffmpeg-x86_64-pc-windows-msvc.exe";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const binariesDir = join(repoRoot, "src-tauri", "binaries");
const sidecarPath = join(binariesDir, SIDECAR_NAME);

function parseArgs(argv) {
  const args = { force: false };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--force") args.force = true;
    else if (argv[i] === "--zip") args.zip = argv[++i];
    else if (argv[i] === "--url") args.url = argv[++i];
    else {
      console.error(`未知参数: ${argv[i]}`);
      process.exit(2);
    }
  }
  return args;
}

/** 验证 ffmpeg 可执行且包含 libx264（合成转码依赖）。 */
function ffmpegHealthy(path) {
  const run = spawnSync(path, ["-version"], { encoding: "utf8" });
  if (run.status !== 0) return { ok: false, reason: "无法执行 ffmpeg -version" };
  const text = `${run.stdout ?? ""}\n${run.stderr ?? ""}`;
  if (!text.includes("enable-libx264")) {
    return { ok: false, reason: "构建不含 libx264（需要 GPL 构建）" };
  }
  return { ok: true };
}

/** 经 curl 下载。Node fetch（undici）不读 HTTP(S)_PROXY 环境变量，
 * 而本机网络依赖本地代理；curl 会遵循代理环境变量，故作为首选下载器。 */
function downloadViaCurl(url, destPath) {
  const exe = process.platform === "win32" ? "C:\\Windows\\System32\\curl.exe" : "curl";
  if (process.platform === "win32" && !existsSync(exe)) return false;
  const run = spawnSync(exe, ["-fSL", "--retry", "3", "-o", destPath, url], {
    stdio: "inherit",
  });
  return run.status === 0;
}

/** 下载 zip 到指定路径。URL 仅允许 http/https。 */
async function downloadZip(url, destPath) {
  if (!/^https?:\/\//.test(url)) {
    throw new Error(`下载源仅允许 http/https: ${url}`);
  }
  console.log(`下载 ${url} ...`);
  const started = Date.now();
  if (downloadViaCurl(url, destPath)) {
    const mb = (statSync(destPath).size / 1024 / 1024).toFixed(1);
    console.log(`下载完成: ${mb} MB，耗时 ${((Date.now() - started) / 1000).toFixed(1)}s`);
    return;
  }
  console.log("curl 下载失败或不可用，回退内置 fetch（不感知代理）...");
  const resp = await fetch(url, { redirect: "follow" });
  if (!resp.ok || !resp.body) {
    throw new Error(`下载失败: HTTP ${resp.status}`);
  }
  await pipeline(Readable.fromWeb(resp.body), createWriteStream(destPath));
  const mb = (statSync(destPath).size / 1024 / 1024).toFixed(1);
  console.log(`下载完成: ${mb} MB，耗时 ${((Date.now() - started) / 1000).toFixed(1)}s`);
}

/** 解压 zip：优先 System32 bsdtar（支持 zip），回退 PowerShell Expand-Archive。 */
function extractZip(zipPath, destDir) {
  mkdirSync(destDir, { recursive: true });
  const bsdtar = "C:\\Windows\\System32\\tar.exe";
  if (process.platform === "win32" && existsSync(bsdtar)) {
    const run = spawnSync(bsdtar, ["-xf", zipPath, "-C", destDir], { stdio: "inherit" });
    if (run.status === 0) return;
    console.error("bsdtar 解压失败，回退 PowerShell Expand-Archive ...");
  }
  const ps = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      "Expand-Archive",
      "-LiteralPath",
      zipPath,
      "-DestinationPath",
      destDir,
      "-Force",
    ],
    { stdio: "inherit" },
  );
  if (ps.status !== 0) throw new Error("解压失败（tar 与 PowerShell 均未成功）");
}

/** 在解压目录里定位 bin/ffmpeg.exe（兼容带顶层目录与平铺两种 zip 结构）。 */
function findFfmpegInExtraction(dir) {
  const candidates = [join(dir, "bin", "ffmpeg.exe")];
  for (const entry of readdirSync(dir)) {
    candidates.push(join(dir, entry, "bin", "ffmpeg.exe"));
  }
  return candidates.find((p) => existsSync(p));
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  if (!args.force && existsSync(sidecarPath)) {
    const health = ffmpegHealthy(sidecarPath);
    if (health.ok) {
      console.log(`已存在可用的 ffmpeg sidecar，跳过下载: ${sidecarPath}`);
      return;
    }
    console.log(`已存在但不可用（${health.reason}），重新部署 ...`);
  }

  // 临时工作区放系统临时目录，避免 tauri CLI 扫描 binaries/ 时看到中间产物。
  const workDir = join(tmpdir(), `fetch-ffmpeg-${Date.now()}`);
  const zipPath = join(workDir, "ffmpeg.zip");
  const extractDir = join(workDir, "extract");
  try {
    mkdirSync(workDir, { recursive: true });
    if (args.zip) {
      if (!existsSync(args.zip)) throw new Error(`本地 zip 不存在: ${args.zip}`);
      console.log(`使用本地 zip: ${args.zip}`);
      rmSync(zipPath, { force: true });
      copyFileSync(args.zip, zipPath);
    } else {
      await downloadZip(args.url ?? DEFAULT_URL, zipPath);
    }

    extractZip(zipPath, extractDir);
    const ffmpegExe = findFfmpegInExtraction(extractDir);
    if (!ffmpegExe) throw new Error("解压产物中未找到 bin/ffmpeg.exe");

    const health = ffmpegHealthy(ffmpegExe);
    if (!health.ok) throw new Error(`下载的 ffmpeg 不可用: ${health.reason}`);

    mkdirSync(binariesDir, { recursive: true });
    copyFileSync(ffmpegExe, sidecarPath);
    const mb = (statSync(sidecarPath).size / 1024 / 1024).toFixed(1);
    console.log(`已部署 ffmpeg sidecar: ${sidecarPath} (${mb} MB)`);
    console.log("下一步：tauri dev / build 会自动将其复制到可执行文件同目录并随包分发。");
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

main().catch((e) => {
  console.error(`[fetch-ffmpeg] 失败: ${e.message}`);
  console.error("可改用本地包: node scripts/fetch-ffmpeg.mjs --zip <路径> （或 --url 换源）");
  process.exit(1);
});
