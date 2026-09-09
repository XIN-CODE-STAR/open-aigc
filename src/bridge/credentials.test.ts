import { invoke, isTauri } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createCredential,
  deleteCredential,
  listCredentials,
  updateCredential,
} from "./credentials";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedIsTauri = vi.mocked(isTauri);

const credentialId = "11111111-2222-4333-8444-555555555555";

const credential = {
  id: credentialId,
  providerName: "Seedance",
  displayName: "种子舞蹈",
  baseUrl: "https://api.seedance.com",
  modelName: "seedance-v2",
  credentialKey: "aigc-studio/credential-11111111-2222-4333-8444-555555555555",
  // Phase A：schema 解析时注入默认值（credentialType/api_key、scope/user）
  credentialType: "api_key",
  scope: "user",
  enabled: true,
  createdAt: "2026-07-16T00:00:00Z",
  updatedAt: "2026-07-16T00:00:00Z",
  // Phase 3: 新增的 authMode / status 字段在后端响应中默认填充
  authMode: "api_key",
  status: "ok",
};

beforeEach(() => {
  mockedInvoke.mockReset();
  mockedIsTauri.mockReturnValue(true);
});

describe("credentials bridge", () => {
  it("lists credentials and validates the response", async () => {
    mockedInvoke.mockResolvedValue([credential]);

    await expect(listCredentials()).resolves.toEqual([credential]);
    expect(mockedInvoke).toHaveBeenCalledWith("credential_v1_list", {
      request: {},
    });
  });

  it("creates a credential and trims whitespace", async () => {
    mockedInvoke.mockResolvedValue(credential);

    await expect(
      createCredential({
        providerName: "  Seedance  ",
        displayName: "  种子舞蹈  ",
        baseUrl: "  https://api.seedance.com  ",
        modelName: "  seedance-v2  ",
        apiKey: "  sk-test-key  ",
      }),
    ).resolves.toEqual(credential);

    expect(mockedInvoke).toHaveBeenCalledWith("credential_v1_create", {
      request: {
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "sk-test-key",
      },
    });
  });

  it("rejects empty provider name", async () => {
    await expect(
      createCredential({
        providerName: "  ",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "sk-test-key",
      }),
    ).rejects.toMatchObject({ code: "validation_failed", field: "providerName" });
  });

  it("rejects empty api key on create", async () => {
    await expect(
      createCredential({
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "  ",
      }),
    ).rejects.toMatchObject({ code: "validation_failed", field: "apiKey" });
  });

  it("rejects invalid credential id on update", async () => {
    await expect(
      updateCredential({
        id: "not-a-uuid",
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
      }),
    ).rejects.toMatchObject({ code: "validation_failed" });
  });

  it("allows update without api key (keeps existing key)", async () => {
    mockedInvoke.mockResolvedValue(credential);

    await expect(
      updateCredential({
        id: credentialId,
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
      }),
    ).resolves.toEqual(credential);

    expect(mockedInvoke).toHaveBeenCalledWith("credential_v1_update", {
      request: {
        id: credentialId,
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
      },
    });
  });

  it("forwards api key when provided on update", async () => {
    mockedInvoke.mockResolvedValue(credential);

    await updateCredential({
      id: credentialId,
      providerName: "Seedance",
      displayName: "种子舞蹈",
      baseUrl: "https://api.seedance.com",
      modelName: "seedance-v2",
      apiKey: "  sk-new-key  ",
    });

    expect(mockedInvoke).toHaveBeenCalledWith("credential_v1_update", {
      request: {
        id: credentialId,
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "sk-new-key",
      },
    });
  });

  it("rejects empty api key string on update", async () => {
    await expect(
      updateCredential({
        id: credentialId,
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "   ",
      }),
    ).rejects.toMatchObject({ code: "validation_failed", field: "apiKey" });
  });

  it("deletes a credential by id", async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await expect(deleteCredential(credentialId)).resolves.toBeUndefined();
    expect(mockedInvoke).toHaveBeenCalledWith("credential_v1_delete", {
      request: { id: credentialId },
    });
  });

  it("rejects invalid id on delete", async () => {
    await expect(deleteCredential("not-a-uuid")).rejects.toMatchObject({
      code: "validation_failed",
    });
  });

  it("rejects unknown fields in create request", async () => {
    await expect(
      createCredential({
        providerName: "Seedance",
        displayName: "种子舞蹈",
        baseUrl: "https://api.seedance.com",
        modelName: "seedance-v2",
        apiKey: "sk-test-key",
        // @ts-expect-error: intentional unknown field
        extra: "field",
      }),
    ).rejects.toMatchObject({ code: "validation_failed" });
  });

  it("rejects response with unknown fields", async () => {
    mockedInvoke.mockResolvedValue({ ...credential, extra: "field" });
    await expect(listCredentials()).rejects.toMatchObject({ code: "ipc_contract_invalid" });
  });

  it("surfaces keychain_unavailable error code from backend", async () => {
    const error = { code: "credential_keychain_unavailable", message: "系统密钥存储不可用。" };
    mockedInvoke.mockRejectedValue(error);
    await expect(listCredentials()).rejects.toMatchObject({
      code: "credential_keychain_unavailable",
    });
  });
});
