import { invoke, isTauri } from "@tauri-apps/api/core";
import { z } from "zod";

const ipcErrorSchema = z
  .object({
    code: z.string().min(1),
    message: z.string().min(1),
    field: z.string().min(1).optional(),
  })
  .strict();

export class NativeCommandError extends Error {
  readonly code: string;
  readonly field?: string;

  constructor(code: string, message: string, field?: string) {
    super(message);
    this.name = "NativeCommandError";
    this.code = code;
    this.field = field;
  }
}

/**
 * 后端返回 `()` 时 Tauri 会将其序列化为 `null`；
 * `z.void()` 仅接受 `undefined`，因此用本 schema 同时接受 `null` 与 `undefined`。
 */
export const voidResponse = z.union([z.void(), z.null()]);

export function isNativeRuntimeAvailable(): boolean {
  return isTauri();
}

export async function invokeNative<T>(
  command: string,
  responseSchema: z.ZodType<T>,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isNativeRuntimeAvailable()) {
    throw new NativeCommandError(
      "desktop_runtime_required",
      "此功能只能在 OPEN AIGC 桌面应用中使用。",
    );
  }

  try {
    const response = args ? await invoke<unknown>(command, args) : await invoke<unknown>(command);
    const parsed = responseSchema.safeParse(response);
    if (!parsed.success) {
      throw new NativeCommandError(
        "ipc_contract_invalid",
        "桌面服务返回了无法识别的数据，请更新或重启应用。",
      );
    }
    return parsed.data;
  } catch (error) {
    throw normalizeNativeError(error);
  }
}

export function firstValidationError(
  result: z.ZodSafeParseError<unknown>,
  fallbackMessage: string,
): NativeCommandError {
  const issue = result.error.issues[0];
  return new NativeCommandError(
    "validation_failed",
    issue?.message ?? fallbackMessage,
    issue?.path[0]?.toString(),
  );
}

export function hasControlCharacters(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0) ?? 0;
    return codePoint <= 31 || (codePoint >= 127 && codePoint <= 159);
  });
}

function normalizeNativeError(error: unknown): NativeCommandError {
  if (error instanceof NativeCommandError) {
    return error;
  }

  const parsed = ipcErrorSchema.safeParse(error);
  if (parsed.success) {
    return new NativeCommandError(parsed.data.code, parsed.data.message, parsed.data.field);
  }

  // Tauri 参数反序列化失败时返回纯字符串，暴露原始内容便于诊断。
  const raw = typeof error === "string" ? error : JSON.stringify(error);
  console.error("[IPC] unstructured error:", raw);
  return new NativeCommandError(
    "native_command_failed",
    raw ?? "桌面服务未能完成操作，请重试或重启应用。",
  );
}
