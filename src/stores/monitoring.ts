import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "@/services/api";
import type { MetricsSample, MonitorState, MonitorStatus } from "@/types";

/** Samples kept per server (in memory only, this window session). */
export const HISTORY_LIMIT = 900; // 15 min at 1 s

export interface ServerMonitor {
  samples: MetricsSample[];
  state: MonitorState;
  message: string | null;
  lastSuccess: string | null;
  /** Explicitly paused by the user: don't auto-restart on connect. */
  paused: boolean;
}

interface MonitoringStore {
  byServer: Record<string, ServerMonitor>;
  start: (serverId: string) => Promise<void>;
  pause: (serverId: string) => Promise<void>;
  /** Called when a terminal session connects. */
  autoStart: (serverId: string) => void;
  latest: (serverId: string) => MetricsSample | undefined;
}

const empty = (): ServerMonitor => ({ samples: [], state: "stopped", message: null, lastSuccess: null, paused: false });

export const useMonitoring = create<MonitoringStore>((set, get) => {
  const patch = (id: string, fn: (m: ServerMonitor) => Partial<ServerMonitor>) =>
    set((s) => {
      const cur = s.byServer[id] ?? empty();
      return { byServer: { ...s.byServer, [id]: { ...cur, ...fn(cur) } } };
    });

  return {
    byServer: {},
    start: async (id) => {
      patch(id, () => ({ paused: false, state: "running", message: null }));
      try {
        await api.monitorStart(id);
      } catch (e) {
        patch(id, () => ({ state: "error", message: errorMessage(e) }));
      }
    },
    pause: async (id) => {
      patch(id, () => ({ paused: true }));
      await api.monitorStop(id).catch(() => undefined);
      patch(id, () => ({ state: "stopped", message: "Paused" }));
    },
    autoStart: (id) => {
      const m = get().byServer[id];
      if (m?.paused) return;
      void get().start(id);
    },
    latest: (id) => get().byServer[id]?.samples.at(-1),
  };
});

let wired = false;
/** Subscribe once to backend monitor events. */
export function initMonitoringEvents() {
  if (wired) return;
  wired = true;
  const set = useMonitoring.setState;
  void listen<MetricsSample>("monitor://sample", ({ payload }) => {
    set((s) => {
      const cur = s.byServer[payload.serverId] ?? empty();
      const samples = cur.samples.length >= HISTORY_LIMIT ? [...cur.samples.slice(1), payload] : [...cur.samples, payload];
      return { byServer: { ...s.byServer, [payload.serverId]: { ...cur, samples, lastSuccess: payload.ts } } };
    });
  });
  void listen<MonitorStatus>("monitor://status", ({ payload }) => {
    set((s) => {
      const cur = s.byServer[payload.serverId] ?? empty();
      return { byServer: { ...s.byServer, [payload.serverId]: { ...cur, state: payload.state, message: payload.message } } };
    });
  });
}

// ---- formatting helpers ----

export function formatBytes(n: number | null | undefined, digits = 1): string {
  if (n == null) return "n/a";
  const u = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : digits)} ${u[i]}`;
}

export function formatRate(bps: number | null | undefined): string {
  return bps == null ? "n/a" : `${formatBytes(bps)}/s`;
}

export function formatUptime(secs: number | null | undefined): string {
  if (secs == null) return "n/a";
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

export function memPercent(s: MetricsSample | undefined): number | null {
  if (!s || s.memTotalBytes == null || s.memUsedBytes == null || s.memTotalBytes === 0) return null;
  return (s.memUsedBytes / s.memTotalBytes) * 100;
}

export function rootDisk(s: MetricsSample | undefined) {
  if (!s || !s.disks.length) return null;
  return s.disks.find((d) => d.mount === "/" || d.mount.toUpperCase() === "C:") ?? s.disks[0];
}

export function diskPercent(d: { totalBytes: number; usedBytes: number } | null): number | null {
  if (!d || d.totalBytes === 0) return null;
  return (d.usedBytes / d.totalBytes) * 100;
}
