import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";

import { parsePreferences, usePreferencesStore } from "./preferences";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("parsePreferences", () => {
  it("returns safe defaults for invalid JSON", () => {
    expect(parsePreferences("not-json")).toEqual({
      autoSaveConversations: true,
      defaultMaxTokens: 4096,
      defaultTemperature: 0.7,
      density: "standard",
      navigationExpanded: true,
      soundEnabled: true,
      streamingEnabled: true,
      theme: "system",
    });
  });

  it("keeps valid fields and rejects unknown values", () => {
    expect(
      parsePreferences(
        JSON.stringify({
          density: "compact",
          navigationExpanded: false,
          theme: "ultraviolet",
        }),
      ),
    ).toEqual({
      autoSaveConversations: true,
      defaultMaxTokens: 4096,
      defaultTemperature: 0.7,
      density: "compact",
      navigationExpanded: false,
      soundEnabled: true,
      streamingEnabled: true,
      theme: "system",
    });
  });

  it("uses defaults when local storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("Storage is unavailable", "SecurityError");
    });
    setActivePinia(createPinia());

    const preferences = usePreferencesStore();

    expect(preferences.theme).toBe("system");
    expect(preferences.density).toBe("standard");
    expect(preferences.navigationExpanded).toBe(true);
  });
});
