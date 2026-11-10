import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "@/services/api";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import type { UpdateInfo } from "@/types";

const LAST_CHECK_KEY = "yoru.lastUpdateCheck"; // timestamp only, nothing sensitive
const DAY = 24 * 60 * 60 * 1000;

interface UpdatesStore {
  current: string | null;
  available: UpdateInfo | null;
  checking: boolean;
  installing: boolean;
  progress: { downloaded: number; total: number | null } | null;
  error: string | null;
  lastChecked: number | null;
  check: (opts?: { silent?: boolean }) => Promise<void>;
  install: () => Promise<void>;
}

export const useUpdates = create<UpdatesStore>((set, get) => ({
  current: null,
  available: null,
  checking: false,
  installing: false,
  progress: null,
  error: null,
  lastChecked: Number(localStorage.getItem(LAST_CHECK_KEY)) || null,
  check: async ({ silent } = {}) => {
    if (get().checking || get().installing) return;
    set({ checking: true, error: null });
    try {
      const info = await api.checkForUpdate();
      const now = Date.now();
      localStorage.setItem(LAST_CHECK_KEY, String(now));
      set({ available: info, lastChecked: now });
      if (info && silent) useUi.getState().toast(`yoru ${info.version} is available — Settings → About & updates`, "info");
      if (!info && !silent) useUi.getState().toast("You're on the latest version", "success");
    } catch (e) {
      set({ error: errorMessage(e) });
      if (!silent) useUi.getState().toast(errorMessage(e), "warning");
    } finally {
      set({ checking: false });
    }
  },
  install: async () => {
    set({ installing: true, progress: { downloaded: 0, total: null }, error: null });
    try {
      await api.installUpdate(); // app restarts on success
    } catch (e) {
      set({ installing: false, progress: null, error: errorMessage(e) });
    }
  },
}));

let wired = false;
/** Load the version, listen for download progress, and auto-check (at most daily). */
export function initUpdates() {
  if (wired) return;
  wired = true;
  void api.appVersion().then((current) => useUpdates.setState({ current }));
  void listen<{ downloaded: number; total: number | null }>("update://progress", ({ payload }) => useUpdates.setState({ progress: payload }));
  const tryAuto = () => {
    const s = useSettings.getState().settings;
    if (!s) return false;
    const last = useUpdates.getState().lastChecked ?? 0;
    if (s.system.checkUpdates && Date.now() - last > DAY) {
      // Small delay so startup stays snappy.
      setTimeout(() => void useUpdates.getState().check({ silent: true }), 8000);
    }
    return true;
  };
  if (!tryAuto()) {
    const unsub = useSettings.subscribe((st) => {
      if (st.settings) {
        unsub();
        tryAuto();
      }
    });
  }
}
