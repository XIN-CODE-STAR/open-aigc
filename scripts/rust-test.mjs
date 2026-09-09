import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Windows 下 cargo test 的测试二进制不带 manifest，加载器会绑定 comctl32 v5，
// 而 tao 通过导入表引用的 TaskDialogIndirect 是 comctl32 v6 专有导出，
// 测试进程启动即失败（STATUS_ENTRYPOINT_NOT_FOUND / 0xc0000139）。
// 本脚本在 cargo test 之前为所有测试可执行文件放置同名外部 manifest
// （外部 manifest 优先于默认内嵌 manifest），再交给 cargo 运行测试。

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const depsDir = resolve(repoRoot, "src-tauri", "target", "debug", "deps");

const TEST_MANIFEST = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
`;

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (result.error) {
    console.error(`Unable to start ${command}: ${result.error.message}`);
    process.exit(2);
  }
  return result.status ?? 1;
}

function collectTestExecutables() {
  const output = runSyncCapture("node", [
    join(scriptDir, "run-with-rust-path.mjs"),
    "cargo",
    "test",
    "--manifest-path",
    join("src-tauri", "Cargo.toml"),
    "--no-run",
    "--message-format=json",
  ]);
  const executables = [];
  for (const line of output.split(/\r?\n/)) {
    if (!line.startsWith("{")) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    if (
      message.reason === "compiler-artifact" &&
      message.profile?.test === true &&
      message.executable
    ) {
      executables.push(message.executable);
    }
  }
  return executables;
}

function runSyncCapture(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["inherit", "pipe", "inherit"],
    // 冷编译时 cargo 的警告输出很大，默认 1MB 缓冲会 ENOBUFS。
    maxBuffer: 256 * 1024 * 1024,
  });
  if (result.error) {
    console.error(`Unable to start ${command}: ${result.error.message}`);
    process.exit(2);
  }
  if (result.status !== 0) {
    // 编译失败：重放捕获的 stdout，保留 cargo 的可读输出后退出。
    process.stdout.write(result.stdout ?? "");
    process.exit(result.status ?? 1);
  }
  return result.stdout ?? "";
}

function placeManifests(executables) {
  if (process.platform !== "win32") return;
  const expected = new Set();
  for (const executable of executables) {
    const manifestPath = `${executable}.manifest`;
    writeFileSync(manifestPath, TEST_MANIFEST);
    expected.add(manifestPath);
  }
  // 清理历史构建残留的 manifest 文件。
  if (existsSync(depsDir)) {
    for (const entry of readdirSync(depsDir)) {
      if (entry.endsWith(".exe.manifest")) {
        const manifestPath = join(depsDir, entry);
        if (!expected.has(manifestPath)) {
          rmSync(manifestPath);
        }
      }
    }
  }
}

function main() {
  if (process.platform === "win32") {
    const executables = collectTestExecutables();
    placeManifests(executables);
    if (executables.length === 0) {
      console.error("No test executables found after cargo test --no-run.");
      process.exit(1);
    }
  }
  process.exit(
    run("node", [
      join(scriptDir, "run-with-rust-path.mjs"),
      "cargo",
      "test",
      "--manifest-path",
      join("src-tauri", "Cargo.toml"),
    ]),
  );
}

main();
