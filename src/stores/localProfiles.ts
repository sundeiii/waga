import { create } from "zustand";
import { api } from "@/services/api";
import type { LocalProfile } from "@/types";

interface LocalProfilesStore {
  profiles: LocalProfile[];
  loaded: boolean;
  load: () => Promise<void>;
  defaultProfile: () => LocalProfile | undefined;
}

export const useLocalProfiles = create<LocalProfilesStore>((set, get) => ({
  profiles: [],
  loaded: false,
  load: async () => {
    try {
      set({ profiles: await api.localProfiles(), loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
  defaultProfile: () => get().profiles.find((p) => p.isDefault) ?? get().profiles[0],
}));
