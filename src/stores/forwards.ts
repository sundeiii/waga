import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "@/services/api";
import { useUi } from "@/stores/ui";
import type { ForwardStatus } from "@/types";

interface ForwardsStore {
  /** Running forwards by id. */
  active: Record<string, ForwardStatus>;
  /** Server whose port-forwarding dialog is open. */
  dialogServerId: string | null;
  openDialog: (serverId: string) => void;
  closeDialog: () => void;
  refresh: () => Promise<void>;
}

export const useForwards = create<ForwardsStore>((set) => ({
  active: {},
  dialogServerId: null,
  openDialog: (dialogServerId) => set({ dialogServerId }),
  closeDialog: () => set({ dialogServerId: null }),
  refresh: async () => {
    const list = await api.listActiveForwards().catch(() => []);
    set({ active: Object.fromEntries(list.map((f) => [f.id, f])) });
  },
}));

let wired = false;
export function initForwardEvents() {
  if (wired) return;
  wired = true;
  void listen<ForwardStatus>("forward://update", ({ payload }) => {
    useForwards.setState((s) => {
      const active = { ...s.active };
      if (payload.state === "active") active[payload.id] = payload;
      else {
        delete active[payload.id];
        if (payload.message && payload.message !== "Stopped") {
          useUi.getState().toast(`Forward 127.0.0.1:${payload.localPort} stopped: ${payload.message}`, "warning");
        }
      }
      return { active };
    });
  });
}

/** Start saved forwards marked "auto-start" once a server's session connects. */
export async function autoStartForwards(serverId: string) {
  try {
    const saved = await api.listSavedForwards(serverId);
    const running = Object.values(useForwards.getState().active);
    for (const f of saved.filter((x) => x.autoStart && !running.some((r) => r.savedId === x.id))) {
      try {
        const st = await api.startForward(serverId, f.id);
        useForwards.setState((s) => ({ active: { ...s.active, [st.id]: st } }));
      } catch (e) {
        useUi.getState().toast(`Forward ${f.label || `${f.remoteHost}:${f.remotePort}`}: ${errorMessage(e)}`, "warning");
      }
    }
  } catch {
    /* no saved forwards */
  }
}
