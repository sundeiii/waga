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

let appliedMaterial: string | null = null;

export function applyAppearance(s: AppSettings) {
  const root = document.documentElement;
  // Tint over the native Acrylic/Mica backdrop; solid when material is "none".
  root.style.setProperty("--app-tint", String(s.appearance.windowMaterial === "none" ? 1 : s.appearance.windowTint));
  if (appliedMaterial !== s.appearance.windowMaterial) {
    appliedMaterial = s.appearance.windowMaterial;
    void api.setWindowMaterial(s.appearance.windowMaterial).catch(() => undefined);
  }
  root.style.setProperty("--accent", s.appearance.accent);
  root.style.setProperty("--glass-alpha", String(s.appearance.transparency));
  root.style.setProperty("--glass-blur", `${s.appearance.blur}px`);
  const reduce = s.appearance.reducedMotion || !s.appearance.animations;
  root.classList.toggle("reduce-motion", reduce);
}

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
