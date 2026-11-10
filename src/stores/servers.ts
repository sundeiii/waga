import { create } from "zustand";
import { api, errorMessage } from "@/services/api";
import type { SecretUpdate, ServerInput, ServerView } from "@/types";

interface ServersStore {
  servers: ServerView[];
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: (input: ServerInput, secrets: SecretUpdate) => Promise<ServerView>;
  update: (id: string, input: ServerInput, secrets: SecretUpdate) => Promise<ServerView>;
  remove: (id: string) => Promise<void>;
  duplicate: (id: string) => Promise<ServerView>;
  toggleFavorite: (id: string) => Promise<void>;
  byId: (id: string) => ServerView | undefined;
}

export const useServers = create<ServersStore>((set, get) => ({
  servers: [],
  loaded: false,
  error: null,
  refresh: async () => {
    try {
      set({ servers: await api.listServers(), loaded: true, error: null });
    } catch (e) {
      set({ error: errorMessage(e), loaded: true });
    }
  },
  create: async (input, secrets) => {
    const s = await api.createServer(input, secrets);
    await get().refresh();
    return s;
  },
  update: async (id, input, secrets) => {
    const s = await api.updateServer(id, input, secrets);
    await get().refresh();
    return s;
  },
  remove: async (id) => {
    await api.deleteServer(id);
    await get().refresh();
  },
  duplicate: async (id) => {
    const s = await api.duplicateServer(id);
    await get().refresh();
    return s;
  },
  toggleFavorite: async (id) => {
    const s = get().byId(id);
    if (!s) return;
    set({ servers: get().servers.map((x) => (x.id === id ? { ...x, favorite: !x.favorite } : x)) });
    try {
      await api.setFavorite(id, !s.favorite);
    } finally {
      await get().refresh();
    }
  },
  byId: (id) => get().servers.find((s) => s.id === id),
}));
