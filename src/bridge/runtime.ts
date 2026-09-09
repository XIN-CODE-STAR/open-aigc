import { z } from "zod";

// ──────────────────────────────────────────────────────────────────
// Runtime Event schemas (must mirror domain/runtime_event.rs serde output)
// ──────────────────────────────────────────────────────────────────

const runtimePhaseSchema = z.enum([
  "planning",
  "submitting",
  "polling",
  "importing",
  "composing",
  "completed",
  "failed",
]);

const shotStatusSchema = z.enum([
  "pending",
  "submitting",
  "generating",
  "downloaded",
  "imported",
  "composed",
  "failed",
]);

const runStartedSchema = z
  .object({
    type: z.literal("run_started"),
    run_id: z.string(),
    total_shots: z.number().int().nonnegative(),
  })
  .strict();

const phaseChangedSchema = z
  .object({
    type: z.literal("phase_changed"),
    run_id: z.string(),
    phase: runtimePhaseSchema,
    message: z.string(),
  })
  .strict();

const shotUpdatedSchema = z
  .object({
    type: z.literal("shot_updated"),
    run_id: z.string(),
    shot_index: z.number().int().nonnegative(),
    status: shotStatusSchema,
    artifact_id: z.string().nullable(),
    message: z.string(),
  })
  .strict();

const submissionUpdatedSchema = z
  .object({
    type: z.literal("submission_updated"),
    run_id: z.string(),
    shot_index: z.number().int().nonnegative(),
    submission_id: z.string(),
    provider: z.string(),
    model: z.string(),
  })
  .strict();

const runCompletedSchema = z
  .object({
    type: z.literal("run_completed"),
    run_id: z.string(),
    status: z.string(),
    output_asset_id: z.string().nullable(),
    duration_secs: z.number(),
    shot_success_count: z.number().int().nonnegative(),
    shot_total_count: z.number().int().nonnegative(),
  })
  .strict();

const runtimeEventSchema = z.discriminatedUnion("type", [
  runStartedSchema,
  phaseChangedSchema,
  shotUpdatedSchema,
  submissionUpdatedSchema,
  runCompletedSchema,
]);

// ──────────────────────────────────────────────────────────────────
// Exports
// ──────────────────────────────────────────────────────────────────

export type RuntimePhase = z.infer<typeof runtimePhaseSchema>;
export type ShotStatus = z.infer<typeof shotStatusSchema>;
export type RuntimeEvent = z.infer<typeof runtimeEventSchema>;

export const RUNTIME_EVENT_CHANNEL = "runtime://event";

/**
 * 解析 Tauri runtime://event payload。失败时返回 null，静默丢弃脏数据。
 */
export function parseRuntimeEvent(payload: unknown): RuntimeEvent | null {
  const result = runtimeEventSchema.safeParse(payload);
  return result.success ? result.data : null;
}
