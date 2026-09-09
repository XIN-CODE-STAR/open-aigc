import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";

const [requestedCommand, ...args] = process.argv.slice(2);

if (!requestedCommand) {
  console.error("Usage: node scripts/run-with-rust-path.mjs <command> [...args]");
  process.exit(2);
}

const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
const cargoBin = join(cargoHome, "bin");
const pathEntries = (process.env.PATH ?? "").split(delimiter);

if (existsSync(cargoBin) && !pathEntries.includes(cargoBin)) {
  pathEntries.unshift(cargoBin);
}

const command =
  process.platform === "win32" && requestedCommand === "tauri" ? "tauri.cmd" : requestedCommand;

const result = spawnSync(command, args, {
  env: { ...process.env, PATH: pathEntries.join(delimiter) },
  stdio: "inherit",
});

if (result.error) {
  console.error(`Unable to start ${requestedCommand}: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
