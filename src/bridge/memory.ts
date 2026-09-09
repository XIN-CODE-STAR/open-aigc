import { z } from "zod";

import { invokeNative } from "./native";

// ──────────────────────────────────────────────────────────────────
// Schemas
// ──────────────────────────────────────────────────────────────────

const memoryStatusSchema = z
  .object({
    available: z.boolean(),
    status: z.string(),
  })
  .strict();

const memorySearchItemSchema = z
  .object({
    sourceType: z.string(),
    content: z.string(),
    score: z.number(),
  })
  .strict();

const statusRequestSchema = z.object({}).strict();

const searchRequestSchema = z
  .object({
    query: z.string().min(1),
    limit: z.number().int().positive().optional(),
  })
  .strict();

void statusRequestSchema;
void searchRequestSchema;

// ──────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────

export type MemoryStatus = z.infer<typeof memoryStatusSchema>;
export type MemorySearchItem = z.infer<typeof memorySearchItemSchema>;

// ──────────────────────────────────────────────────────────────────
// IPC functions
// ──────────────────────────────────────────────────────────────────

export async function memoryV1Status(): Promise<MemoryStatus> {
  return invokeNative("memory_v1_status", memoryStatusSchema, {});
}

export async function memoryV1Search(query: string, limit?: number): Promise<MemorySearchItem[]> {
  return invokeNative("memory_v1_search", z.array(memorySearchItemSchema), {
    query,
    limit,
  });
}
