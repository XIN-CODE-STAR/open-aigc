import { computed, ref, watch } from "vue";
import { defineStore } from "pinia";

export type DensityPreference = "compact" | "standard" | "comfortable";
export type ThemePreference = "system" | "light" | "dark";

export interface PreferenceSnapshot {
  density: DensityPreference;
  navigationExpanded: boolean;
  theme: ThemePreference;
  soundEnabled: boolean;
  autoSaveConversations: boolean;
  defaultTemperature: number;
  defaultMaxTokens: number;
  streamingEnabled: boolean;
}

const STORAGE_KEY = "aigc-studio.preferences.v1";

const DEFAULT_PREFERENCES: PreferenceSnapshot = {
  density: "standard",
  navigationExpanded: true,
  theme: "system",
  soundEnabled: true,
  autoSaveConversations: true,
  defaultTemperature: 0.7,
  defaultMaxTokens: 4096,
  streamingEnabled: true,
};

function isDensity(value: unknown): value is DensityPreference {
  return value === "compact" || value === "standard" || value === "comfortable";
}

function isTheme(value: unknown): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

export function parsePreferences(value: string | null): PreferenceSnapshot {
  if (!value) {
    return { ...DEFAULT_PREFERENCES };
  }

  try {
    const candidate: unknown = JSON.parse(value);
    if (!candidate || typeof candidate !== "object") {
      return { ...DEFAULT_PREFERENCES };
    }

    const record = candidate as Record<string, unknown>;
    return {
      density: isDensity(record.density) ? record.density : DEFAULT_PREFERENCES.density,
      navigationExpanded:
        typeof record.navigationExpanded === "boolean"
          ? record.navigationExpanded
          : DEFAULT_PREFERENCES.navigationExpanded,
      theme: isTheme(record.theme) ? record.theme : DEFAULT_PREFERENCES.theme,
      soundEnabled:
        typeof record.soundEnabled === "boolean"
          ? record.soundEnabled
          : DEFAULT_PREFERENCES.soundEnabled,
      autoSaveConversations:
        typeof record.autoSaveConversations === "boolean"
          ? record.autoSaveConversations
          : DEFAULT_PREFERENCES.autoSaveConversations,
      defaultTemperature:
        typeof record.defaultTemperature === "number" &&
        record.defaultTemperature >= 0 &&
        record.defaultTemperature <= 2
          ? record.defaultTemperature
          : DEFAULT_PREFERENCES.defaultTemperature,
      defaultMaxTokens:
        typeof record.defaultMaxTokens === "number" && record.defaultMaxTokens > 0
          ? record.defaultMaxTokens
          : DEFAULT_PREFERENCES.defaultMaxTokens,
      streamingEnabled:
        typeof record.streamingEnabled === "boolean"
          ? record.streamingEnabled
          : DEFAULT_PREFERENCES.streamingEnabled,
    };
  } catch {
    return { ...DEFAULT_PREFERENCES };
  }
}

function loadPreferences(): PreferenceSnapshot {
  if (typeof window === "undefined") {
    return { ...DEFAULT_PREFERENCES };
  }

  try {
    return parsePreferences(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return { ...DEFAULT_PREFERENCES };
  }
}

export const usePreferencesStore = defineStore("preferences", () => {
  const initial = loadPreferences();
  const density = ref<DensityPreference>(initial.density);
  const navigationExpanded = ref(initial.navigationExpanded);
  const theme = ref<ThemePreference>(initial.theme);
  const soundEnabled = ref(initial.soundEnabled);
  const autoSaveConversations = ref(initial.autoSaveConversations);
  const defaultTemperature = ref(initial.defaultTemperature);
  const defaultMaxTokens = ref(initial.defaultMaxTokens);
  const streamingEnabled = ref(initial.streamingEnabled);
  const systemPrefersDark = ref(false);
  let initialized = false;

  const resolvedTheme = computed<"light" | "dark">(() => {
    if (theme.value === "system") {
      return systemPrefersDark.value ? "dark" : "light";
    }

    return theme.value;
  });

  function applyToDocument(): void {
    if (typeof document === "undefined") {
      return;
    }

    document.documentElement.dataset.theme = resolvedTheme.value;
    document.documentElement.dataset.density = density.value;
  }

  function persist(): void {
    if (typeof window === "undefined") {
      return;
    }

    const snapshot: PreferenceSnapshot = {
      density: density.value,
      navigationExpanded: navigationExpanded.value,
      theme: theme.value,
      soundEnabled: soundEnabled.value,
      autoSaveConversations: autoSaveConversations.value,
      defaultTemperature: defaultTemperature.value,
      defaultMaxTokens: defaultMaxTokens.value,
      streamingEnabled: streamingEnabled.value,
    };
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
    } catch {
      // Appearance preferences are non-critical when device storage is unavailable.
    }
  }

  function initialize(): void {
    if (initialized) {
      return;
    }

    initialized = true;
    if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
      const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      systemPrefersDark.value = mediaQuery.matches;
      mediaQuery.addEventListener("change", (event) => {
        systemPrefersDark.value = event.matches;
      });
    }

    applyToDocument();
  }

  function setDensity(value: DensityPreference): void {
    density.value = value;
  }

  function setTheme(value: ThemePreference): void {
    theme.value = value;
  }

  function toggleNavigation(): void {
    navigationExpanded.value = !navigationExpanded.value;
  }

  function toggleSound(): void {
    soundEnabled.value = !soundEnabled.value;
  }

  function toggleAutoSave(): void {
    autoSaveConversations.value = !autoSaveConversations.value;
  }

  function setDefaultTemperature(value: number): void {
    defaultTemperature.value = Math.max(0, Math.min(2, value));
  }

  function setDefaultMaxTokens(value: number): void {
    defaultMaxTokens.value = Math.max(1, Math.min(128000, value));
  }

  function toggleStreaming(): void {
    streamingEnabled.value = !streamingEnabled.value;
  }

  watch(
    [
      theme,
      density,
      navigationExpanded,
      systemPrefersDark,
      soundEnabled,
      autoSaveConversations,
      defaultTemperature,
      defaultMaxTokens,
      streamingEnabled,
    ],
    () => {
      applyToDocument();
      persist();
    },
  );

  return {
    autoSaveConversations,
    defaultMaxTokens,
    defaultTemperature,
    density,
    initialize,
    navigationExpanded,
    resolvedTheme,
    setDefaultMaxTokens,
    setDefaultTemperature,
    setDensity,
    setTheme,
    soundEnabled,
    streamingEnabled,
    theme,
    toggleAutoSave,
    toggleNavigation,
    toggleSound,
    toggleStreaming,
  };
});
