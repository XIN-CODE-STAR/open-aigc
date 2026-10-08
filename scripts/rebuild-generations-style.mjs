/**
 * P4 修复：用 postcss 权威解析重建 GenerationsPage 样式块。
 * 只保留模板实际使用的选择器（LIVE 集合），一次性写入。
 * 运行：node scripts/rebuild-generations-style.mjs
 */
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

// pnpm 布局下 postcss 是传递依赖，显式定位
const { createRequire } = await import("node:module");
const req = createRequire(import.meta.url);
const postcssPath = req.resolve("postcss", {
  paths: [process.cwd() + "/node_modules/.pnpm/postcss@8.5.19/node_modules/postcss"],
});
const { pathToFileURL } = await import("node:url");
const postcss = (await import(pathToFileURL(postcssPath).href)).default;
const FILE = "src/modules/generations/pages/GenerationsPage.vue";

const LIVE = new Set([
  "grok-shell",
  "grok-welcome",
  "grok-conversation",
  "grok-prefs",
  "prefs-close",
  "prefs-head",
  "prefs-label",
  "prefs-section",
  "analysis-caption",
  "analysis-close",
  "analysis-content",
  "analysis-header",
  "analysis-item",
  "analysis-item-name",
  "analysis-panel",
  "analysis-tag",
  "analysis-tags",
  "analysis-title",
  "analyze-btn",
  "semantic-analysis-section",
  "spin",
  "analysis-panel-enter-active",
  "analysis-panel-enter-from",
  "analysis-panel-leave-active",
  "analysis-panel-leave-to",
  "drawer-enter-active",
  "drawer-enter-from",
  "drawer-leave-active",
  "drawer-leave-to",
]);

// 从清理前的提交取原始样式（模板已在此后独立演进）
// 641aedf = 死样式清理提交的父提交，样式块验证为原始干净状态
const raw = execSync("git show 641aedf:src/modules/generations/pages/GenerationsPage.vue", {
  encoding: "utf8",
  maxBuffer: 32 * 1024 * 1024,
});
const s0 = raw.indexOf("<style scoped>") + "<style scoped>".length;
const e0 = raw.indexOf("</style>");
const original = raw.slice(s0, e0);

function selectorClasses(sel) {
  const out = new Set();
  for (const part of sel.split(",")) {
    for (const m of part.matchAll(/\.([a-zA-Z][\w-]*)/g)) out.add(m[1]);
  }
  return out;
}

function alive(node) {
  const classes = selectorClasses(node.selector ?? "");
  if (classes.size === 0) return true; // 元素/通配选择器保守保留
  for (const c of classes) if (LIVE.has(c)) return true;
  return false;
}

const root = postcss.parse(original);
const kept = [];
for (const node of [...root.nodes]) {
  if (node.type === "rule" && alive(node)) kept.push(node);
  else if (node.type === "atrule" && node.name === "media") {
    const innerKept = [...node.nodes].filter(alive);
    if (innerKept.length > 0) {
      node.nodes = innerKept;
      node.raws.after = "\n  ";
      kept.push(node);
    }
  } else if (node.type === "atrule" && node.name === "keyframes") {
    if (LIVE.has(node.params)) kept.push(node);
  }
}

const container = postcss.root();
container.append(kept);
const css = container.toString().replace(/\n{3,}/g, "\n\n");

// 校验：重新 parse 不抛错且花括号平衡
postcss.parse(css);
if (css.split("{").length !== css.split("}").length) {
  throw new Error("brace imbalance");
}
console.log("kept nodes:", kept.length, "| css lines:", css.split("\n").length);

const current = readFileSync(FILE, "utf8");
const cs = current.indexOf("<style scoped>");
const ce = current.indexOf("</style>");
writeFileSync(
  FILE,
  current.slice(0, cs) + "<style scoped>\n" + css.trim() + "\n" + current.slice(ce),
);
console.log("written");
