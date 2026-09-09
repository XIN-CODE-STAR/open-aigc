import { z } from "zod";

import { invokeNative } from "./native";

// ── Schemas ──

const attemptStatusSchema = z.enum([
  "pending",
  "submitted",
  "polling",
  "downloading",
  "succeeded",
  "failed",
  "cancelled",
  "timed-out",
]);

const generationAttemptSchema = z
  .object({
    id: z.string(),
    taskId: z.string(),
    credentialId: z.string(),
    capability: z.string(),
    requestSnapshotJson: z.string(),
    remoteJobId: z.string().nullable(),
    status: attemptStatusSchema,
    progress: z.number().int(),
    errorCode: z.string().nullable(),
    errorMessage: z.string().nullable(),
    resultAssetId: z.string().nullable(),
    providerId: z.string(),
    consecutiveFailures: z.number().int().min(0),
    lastPollAt: z.string().nullable(),
    startedAt: z.string().nullable(),
    finishedAt: z.string().nullable(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict();

// ── Types ──

export type AttemptStatus = z.infer<typeof attemptStatusSchema>;
export type GenerationAttempt = z.infer<typeof generationAttemptSchema>;

// ── IPC functions ──

export async function queueV1SubmitAttempt(input: {
  taskId: string;
  credentialId: string;
  capability: string;
  requestSnapshotJson: string;
  providerId: string;
}): Promise<GenerationAttempt> {
  return invokeNative("queue_v1_submit_attempt", generationAttemptSchema, { ...input });
}

export async function queueV1GetAttempt(id: string): Promise<GenerationAttempt | null> {
  return invokeNative("queue_v1_get_attempt", generationAttemptSchema.nullable(), { id });
}

export async function queueV1ListAttempts(taskId: string): Promise<GenerationAttempt[]> {
  return invokeNative("queue_v1_list_attempts", z.array(generationAttemptSchema), {
    taskId,
  });
}

export async function queueV1ListActive(): Promise<GenerationAttempt[]> {
  return invokeNative("queue_v1_list_active", z.array(generationAttemptSchema), {});
}

export async function queueV1CancelAttempt(id: string): Promise<GenerationAttempt> {
  return invokeNative("queue_v1_cancel_attempt", generationAttemptSchema, { id });
}

export async function queueV1RetryAttempt(id: string): Promise<GenerationAttempt> {
  return invokeNative("queue_v1_retry_attempt", generationAttemptSchema, { id });
}
