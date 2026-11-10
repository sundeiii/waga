import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import type { TransferEvent } from "@/types";

interface TransfersStore {
  byId: Record<string, TransferEvent & { startedAt: number; updatedAt: number }>;
  order: string[];
  clearFinished: () => void;
}

export const useTransfers = create<TransfersStore>((set, get) => ({
  byId: {},
  order: [],
  clearFinished: () => {
    const byId = { ...get().byId };
    for (const id of Object.keys(byId)) if (byId[id].state !== "running") delete byId[id];
    set({ byId, order: get().order.filter((id) => byId[id]) });
  },
}));

type Listener = (e: TransferEvent) => void;
const listeners = new Set<Listener>();
/** Subscribe to transfer completions (used to refresh panes). */
export function onTransferUpdate(fn: Listener) {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

let wired = false;
export function initTransferEvents() {
  if (wired) return;
  wired = true;
  void listen<TransferEvent>("sftp://transfer", ({ payload }) => {
    const now = Date.now();
    useTransfers.setState((s) => {
      const prev = s.byId[payload.id];
      return {
        byId: { ...s.byId, [payload.id]: { ...payload, startedAt: prev?.startedAt ?? now, updatedAt: now } },
        order: prev ? s.order : [payload.id, ...s.order],
      };
    });
    for (const l of listeners) l(payload);
  });
}
