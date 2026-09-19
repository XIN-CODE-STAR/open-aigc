#!/usr/bin/env node
// 三通道（kling / seedance / 即梦代理）健康检查编排脚本。
//
// 用法：
//   node scripts/channel-health.mjs             # 免费检查：代理连通性 + 各通道 health_check
//   node scripts/channel-health.mjs --submit    # 追加最低档真实生成提交（计费真实发生，谨慎使用）
//
// 凭据全部来自环境变量（技术决策 D8：不写死、不读应用凭据库）：
//   KLING_ACCESS_KEY / KLING_SECRET_KEY   可灵 AK/SK（缺失 → 跳过 kling）
//   SEEDANCE_API_KEY                      火山方舟 API Key（缺失 → 跳过 seedance）
//   JIMENG_SESSION                        即梦会话 ID（缺失 → 只探测代理进程存活）
//
// kling/seedance 的 health/submit 复用 Rust adapter（与业务同一代码路径），
// 以 #[ignore] 测试形式存在（src/tests/channel_health.rs），经 rust-test.mjs 触发
// 以获得测试可执行文件所需的 comctl32 外部 manifest。
// 即梦代理是本仓库固定端口 5100 的常驻本地服务（jimeng_connector.rs 同款端点），
// 非用户输入 URL，故不受外链 SSRF 约束限制。

import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const submit = process.argv.includes("--submit");
const JIMENG_PROXY = "http://127.0.0.1:5100";

const failures = [];

function mark(ok, label, detail) {
  console.log(`[${ok ? "PASS" : "FAIL"}] ${label}: ${detail}`);
  if (!ok) failures.push(label);
}

function markSkip(label, detail) {
  console.log(`[SKIP] ${label}: ${detail}`);
}

async function fetchWithTimeout(url, options = {}, ms = 10_000) {
  return fetch(url, { ...options, signal: AbortSignal.timeout(ms) });
}

// ── 即梦代理 ──

async function checkJimeng() {
  // 存活探测：与 account_health_worker 同一端点；拿到任意 HTTP 响应即说明代理在跑。
  let resp;
  try {
    resp = await fetchWithTimeout(`${JIMENG_PROXY}/token/points`, { method: "POST" });
  } catch (e) {
    mark(false, "jimeng", `代理不可达（${e.cause?.code ?? e.message}）；服务应常驻 127.0.0.1:5100`);
    return;
  }
  const session = process.env.JIMENG_SESSION;
  const body = await resp.text().catch(() => "");
  if (!session) {
    mark(true, "jimeng", `代理存活（HTTP ${resp.status}）；未设置 JIMENG_SESSION，跳过会话验证`);
    return;
  }
  // 响应形如 [{ token, points: { totalCredit, ... } }] 时会话有效
  const sessionOk = body.trim().startsWith("[") && body.includes("totalCredit");
  if (sessionOk) {
    const credit = body.match(/"totalCredit":\s*(\d+)/)?.[1];
    mark(true, "jimeng", `会话有效${credit ? `，剩余积分 ${credit}` : ""}`);
  } else {
    mark(false, "jimeng", `会话无效或登录失效（HTTP ${resp.status}）`);
  }

  if (!submit) return;
  // 最低档提交：代理同步返回最终视频 URL（代理内部轮询，最长可能 30 分钟）
  console.log("[jimeng] --submit：提交最低档视频生成（jimeng-video-seedance-2.0 / 720p / 5s）...");
  try {
    const submitResp = await fetchWithTimeout(
      `${JIMENG_PROXY}/v1/videos/generations`,
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${session}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          model: "jimeng-video-seedance-2.0",
          prompt: "a calm ocean wave, health check test clip",
          ratio: "16:9",
          resolution: "720p",
          duration: 5,
        }),
      },
      30 * 60 * 1000,
    );
    const json = await submitResp.json().catch(() => null);
    const url = json?.data?.[0]?.url;
    mark(
      submitResp.ok && Boolean(url),
      "jimeng submit",
      url ? `成功: ${url.slice(0, 80)}...` : `失败（HTTP ${submitResp.status}）`,
    );
  } catch (e) {
    mark(false, "jimeng submit", `提交失败: ${e.message}`);
  }
}

// ── kling / seedance（经 Rust adapter 冒烟测试）──

function checkRustChannels() {
  const hasKling = Boolean(process.env.KLING_ACCESS_KEY && process.env.KLING_SECRET_KEY);
  const hasSeedance = Boolean(process.env.SEEDANCE_API_KEY);
  if (!hasKling) markSkip("kling", "未设置 KLING_ACCESS_KEY / KLING_SECRET_KEY");
  if (!hasSeedance) markSkip("seedance", "未设置 SEEDANCE_API_KEY");
  if (!hasKling && !hasSeedance) return;

  console.log(
    submit
      ? "[rust] 运行 kling/seedance health + 最低档提交（AIGC_CHANNEL_SUBMIT=1）..."
      : "[rust] 运行 kling/seedance health_check（不计费）...",
  );
  const env = { ...process.env };
  if (submit) env.AIGC_CHANNEL_SUBMIT = "1";
  const result = spawnSync(
    process.execPath,
    [join(scriptDir, "rust-test.mjs"), "--", "--ignored", "channel_health", "--nocapture"],
    { stdio: "inherit", env },
  );
  mark(
    result.status === 0,
    "kling/seedance",
    result.status === 0 ? "冒烟测试通过（缺失凭据的通道已自动跳过）" : `cargo test 退出码 ${result.status}`,
  );
}

await checkJimeng();
checkRustChannels();

if (failures.length > 0) {
  console.error(`\n存在失败项：${failures.join("、")}`);
  process.exit(1);
}
console.log("\n全部通道检查完成。");
