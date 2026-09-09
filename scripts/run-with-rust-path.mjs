import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [requestedCommand, ...args] = process.argv.slice(2);

if (!requestedCommand) {
  console.error("Usage: node scripts/run-with-rust-path.mjs <command> [...args]");
  process.exit(2);
}

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
const cargoBin = join(cargoHome, "bin");
const pathEntries = (process.env.PATH ?? "").split(delimiter);

if (existsSync(cargoBin) && !pathEntries.includes(cargoBin)) {
  pathEntries.unshift(cargoBin);
}

// Node 20.12+ (CVE-2024-27980) 禁止 spawnSync 直接执行 .cmd/.bat 文件（EINVAL），
// 因此 Windows 上 tauri 命令改为通过 node 直接执行其 JS 入口。
let command = requestedCommand;
let commandArgs = args;

if (process.platform === "win32" && requestedCommand === "tauri") {
  const tauriCliEntry = join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!existsSync(tauriCliEntry)) {
    console.error(`Unable to locate tauri CLI entry: ${tauriCliEntry}`);
    process.exit(1);
  }
  command = process.execPath;
  commandArgs = [tauriCliEntry, ...args];
}

const result = spawnSync(command, commandArgs, {
  env: { ...process.env, PATH: pathEntries.join(delimiter) },
  stdio: "inherit",
});

if (result.error) {
  console.error(`Unable to start ${requestedCommand}: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
