import { create } from "zustand";
import { api, errorMessage } from "@/services/api";
import { useSettings } from "@/stores/settings";
import { useServers } from "@/stores/servers";
import { useSnippets } from "@/stores/snippets";
import type { MergeStats, SyncReport } from "@/types";

interface SyncStore {
  hasPassphrase: boolean;
  syncing: boolean;
  last: SyncReport | null;
  error: string | null;
  loadStatus: () => Promise<void>;
  run: (opts?: { quiet?: boolean }) => Promise<SyncReport | null>;
}

/** Reload lists after other PCs' (or a backup's) changes were merged in. */
export async function refreshAfterMerge(stats: MergeStats) {
  if (stats.added + stats.updated + stats.deleted === 0) return;
  await useServers.getState().refresh();
  if (useSnippets.getState().loaded) await useSnippets.getState().load();
}

export const useSync = create<SyncStore>((set, get) => ({
  hasPassphrase: false,
  syncing: false,
  last: null,
  error: null,
  loadStatus: async () => {
    const s = await api.syncStatus().catch(() => null);
    if (s) set({ hasPassphrase: s.hasPassphrase });
  },
  run: async () => {
    if (get().syncing) return null;
    set({ syncing: true, error: null });
    try {
      const r = await api.syncNow();
      set({ last: r, error: r.errors.length ? r.errors.join("\n") : null });
      await refreshAfterMerge(r.stats);
      return r;
    } catch (e) {
      set({ error: errorMessage(e) });
      return null;
    } finally {
      set({ syncing: false });
    }
  },
}));

let timer: ReturnType<typeof setInterval> | undefined;
let soon: ReturnType<typeof setTimeout> | undefined;

function canSync() {
  const s = useSettings.getState().settings?.sync;
  return !!s?.enabled && !!s.folder && useSync.getState().hasPassphrase;
}

/** Sync a few seconds after a local change (debounced). */
export function syncSoon() {
  if (!canSync()) return;
  clearTimeout(soon);
  soon = setTimeout(() => void useSync.getState().run({ quiet: true }), 4000);
}

/** Start/stop the periodic sync to match settings. */
export function rescheduleSync() {
  clearInterval(timer);
  const s = useSettings.getState().settings?.sync;
  if (!canSync() || !s) return;
  timer = setInterval(() => void useSync.getState().run({ quiet: true }), s.intervalMins * 60_000);
}

let wired = false;
export function initSync() {
  if (wired) return;
  wired = true;
  void useSync
    .getState()
    .loadStatus()
    .then(() => {
      const start = () => {
        rescheduleSync();
        if (canSync()) setTimeout(() => void useSync.getState().run({ quiet: true }), 5000);
      };
      if (useSettings.getState().settings) start();
      else {
        const unsub = useSettings.subscribe((st) => {
          if (st.settings) {
            unsub();
            start();
          }
        });
      }
    });
  // Re-plan when sync settings change.
  useSettings.subscribe((st, prev) => {
    if (st.settings?.sync !== prev.settings?.sync) rescheduleSync();
  });
  // Push local edits soon after they happen.
  useServers.subscribe((st, prev) => {
    if (st.servers !== prev.servers && prev.loaded) syncSoon();
  });
  useSnippets.subscribe((st, prev) => {
    if (st.snippets !== prev.snippets && prev.loaded) syncSoon();
  });
}
