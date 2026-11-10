import { create } from "zustand";
import { api, errorMessage } from "@/services/api";
import type { AppSettings } from "@/types";

interface SettingsStore {
  settings: AppSettings | null;
  error: string | null;
  saving: boolean;
  load: () => Promise<void>;
  /** Optimistically apply and persist (debounced). */
  update: (fn: (s: AppSettings) => AppSettings) => void;
  reset: () => Promise<void>;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;

let appliedWindow: string | null = null;
const systemLight = window.matchMedia?.("(prefers-color-scheme: light)");

/** "dark" | "light" after resolving "system" against Windows' app mode. */
export function resolvedTheme(s: AppSettings | null): "dark" | "light" {
  const t = s?.appearance.theme ?? "dark";
  if (t === "system") return systemLight?.matches ? "light" : "dark";
  return t;
}

export function applyAppearance(s: AppSettings) {
  const root = document.documentElement;
  const theme = resolvedTheme(s);
  if (root.dataset.theme !== theme) {
    root.dataset.theme = theme;
    window.dispatchEvent(new Event("yoru-theme-change"));
  }
  // Tint over the native Acrylic/Mica backdrop; solid when material is "none".
  root.style.setProperty("--app-tint", String(s.appearance.windowMaterial === "none" ? 1 : s.appearance.windowTint));
  const key = `${s.appearance.windowMaterial}|${s.appearance.theme}|${theme}`;
  if (appliedWindow !== key) {
    appliedWindow = key;
    void api.setWindowAppearance(s.appearance.windowMaterial, s.appearance.theme).catch(() => undefined);
  }
  root.style.setProperty("--accent", s.appearance.accent);
  root.style.setProperty("--glass-alpha", String(s.appearance.transparency));
  root.style.setProperty("--glass-blur", `${s.appearance.blur}px`);
  const reduce = s.appearance.reducedMotion || !s.appearance.animations;
  root.classList.toggle("reduce-motion", reduce);
}

// Follow Windows' light/dark switch live when the theme is "system".
systemLight?.addEventListener("change", () => {
  const s = useSettings.getState().settings;
  if (s?.appearance.theme === "system") applyAppearance(s);
});

export const useSettings = create<SettingsStore>((set, get) => ({
  settings: null,
  error: null,
  saving: false,
  load: async () => {
    try {
      const s = await api.getSettings();
      applyAppearance(s);
      set({ settings: s, error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },
  update: (fn) => {
    const cur = get().settings;
    if (!cur) return;
    const next = fn(structuredClone(cur));
    applyAppearance(next);
    set({ settings: next });
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      set({ saving: true });
      try {
        await api.updateSettings(next);
        set({ error: null });
      } catch (e) {
        set({ error: errorMessage(e) });
      } finally {
        set({ saving: false });
      }
    }, 350);
  },
  reset: async () => {
    const s = await api.resetSettings();
    applyAppearance(s);
    set({ settings: s });
  },
}));
