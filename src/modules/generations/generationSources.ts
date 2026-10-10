/**
 * 哪些「生成来源」能产出哪类媒体。
 *
 * 用于「生成来源」选择器在**图片/视频模式**下只列出真正能生成的来源——
 * 大语言模型凭据（如 zhipu / glm-4.5-air）在 Agent 模式里设置即可，
 * 不该出现在生成模式下（2026-10-10 用户反馈的缺陷）。
 *
 * ## 为什么是镜像而不是问后端
 *
 * 后端的权威是 `ProviderRegistry`（`generation_engine.rs` 的
 * `register_configured_generation_providers`），但**它只在应用启动与备份恢复时重建**
 * （`ServiceReloader` 只被 `ipc/backup.rs` 调用）——凭据增删后注册表是陈旧的。
 * 用它当选择器的数据源，用户新加的凭据要重启应用才会出现，比本缺陷更糟。
 * 所以这里镜像后端那套**名字谓词**，工作在实时刷新的凭据/账号列表上。
 *
 * 谓词与 needle 清单照抄 `src-tauri/src/application/generation_engine.rs`
 * （`is_grok_credential` / `is_kling_credential` / `is_seedance_credential` /
 * `is_jimeng_account_credential`），匹配语义也照抄：**小写 + 子串匹配
 * `provider_name + display_name + model_name`**。各 adapter 的 `capabilities()`
 * 决定媒体类型。改动任一侧都要同步另一侧——`generationSources.test.ts` 会钉住这张表。
 */

export type GenerationMediaKind = "image" | "video";

interface GenerationSourceRule {
  /** 命中任一 needle 即算该来源（小写子串匹配）。 */
  readonly needles: readonly string[];
  /** 该来源能产出的媒体类型。 */
  readonly media: readonly GenerationMediaKind[];
}

/**
 * 顺序有意义：先匹配「图 + 视频」的来源，最后才是只出图的 grok——
 * 与后端 `if (is_grok) … if (is_kling) … if (is_seedance) …` 的判定顺序一致。
 */
const GENERATION_SOURCE_RULES: readonly GenerationSourceRule[] = [
  // KlingVideoAdapter: TextToVideo / ImageToVideo / TextToImage
  { needles: ["kling", "可灵", "快影"], media: ["image", "video"] },
  // SeedanceVideoAdapter: TextToVideo / ImageToVideo / TextToImage / ImageToImage
  {
    needles: ["seedance", "seedream", "volcengine", "ark", "火山", "豆包", "即梦"],
    media: ["image", "video"],
  },
  // 即梦账号（session 类凭据 / resource_accounts）：图片 + 视频
  { needles: ["jimeng", "dreamina", "即梦"], media: ["image", "video"] },
  // GrokUnifiedAdapter: TextToImage（**不出视频**）
  { needles: ["grok", "xai"], media: ["image"] },
];

/** 参与匹配的三个字段，与后端 `credential_matches_any` 一致。 */
export interface GenerationSourceIdentity {
  providerName: string;
  displayName?: string;
  modelName?: string;
}

/**
 * 该来源能产出的媒体类型；**空数组表示它不是生成来源**（例如 LLM 凭据），
 * 因此不应出现在图片/视频模式的「生成来源」里。
 */
export function generationMediaKinds(
  source: GenerationSourceIdentity,
): readonly GenerationMediaKind[] {
  const haystack = [source.providerName, source.displayName ?? "", source.modelName ?? ""]
    .join(" ")
    .toLowerCase();
  const rule = GENERATION_SOURCE_RULES.find((candidate) =>
    candidate.needles.some((needle) => haystack.includes(needle)),
  );
  return rule?.media ?? [];
}

/** 该来源是否能在给定模式下生成（`kind` 为 null 表示不过滤，如 Agent 模式）。 */
export function isGenerationSourceFor(
  source: GenerationSourceIdentity,
  kind: GenerationMediaKind | null,
): boolean {
  if (!kind) return true;
  return generationMediaKinds(source).includes(kind);
}
