import { create } from "zustand";
import type { HostKeyStatus, SessionState } from "@/types";

/** UI state of a terminal tab. "reconnecting" is frontend-only (auto-reconnect backoff). */
export type TabState = SessionState | "reconnecting";

export interface TabMeta {
  id: string;
  kind: "ssh" | "local";
  /** Empty string for local tabs. */
  serverId: string;
  title: string;
  state: TabState;
  message: string | null;
  /** Backend session currently bound to this tab. */
  sessionId: string | null;
}

export type Prompt =
  | {
      type: "hostKey";
      tabId: string;
      sessionId: string;
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      status: HostKeyStatus;
      previous: { algorithm: string; fingerprint: string; addedAt: string }[];
    }
  | {
      type: "secret";
      tabId: string;
      sessionId: string;
      kind: "password" | "passphrase";
      prompt: string;
      attempt: number;
    };

interface SessionsStore {
  tabs: TabMeta[];
  activeTabId: string | null;
  prompts: Prompt[];
  addTab: (t: TabMeta) => void;
  patchTab: (id: string, patch: Partial<TabMeta>) => void;
  removeTab: (id: string) => void;
  setActive: (id: string | null) => void;
  pushPrompt: (p: Prompt) => void;
  clearPrompts: (tabId: string) => void;
}

export const useSessions = create<SessionsStore>((set, get) => ({
  tabs: [],
  activeTabId: null,
  prompts: [],
  addTab: (t) => set({ tabs: [...get().tabs, t], activeTabId: t.id }),
  patchTab: (id, patch) => set({ tabs: get().tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)) }),
  removeTab: (id) => {
    const tabs = get().tabs.filter((t) => t.id !== id);
    const idx = get().tabs.findIndex((t) => t.id === id);
    const active = get().activeTabId === id ? (tabs[Math.max(0, idx - 1)]?.id ?? null) : get().activeTabId;
    set({ tabs, activeTabId: active, prompts: get().prompts.filter((p) => p.tabId !== id) });
  },
  setActive: (activeTabId) => set({ activeTabId }),
  pushPrompt: (p) => set({ prompts: [...get().prompts.filter((x) => x.tabId !== p.tabId), p] }),
  clearPrompts: (tabId) => set({ prompts: get().prompts.filter((p) => p.tabId !== tabId) }),
}));

export const isLive = (s: TabState) =>
  s === "connected" || s === "connecting" || s === "authenticating" || s === "awaitingHostKey" || s === "awaitingSecret";

export const connectedServerIds = (tabs: TabMeta[]) =>
  new Set(tabs.filter((t) => t.kind === "ssh" && t.state === "connected").map((t) => t.serverId));
