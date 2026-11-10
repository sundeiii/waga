import { create } from "zustand";
import { api, errorMessage } from "@/services/api";
import { terminalEngine } from "@/services/terminalEngine";
import { useSessions } from "@/stores/sessions";
import { useUi } from "@/stores/ui";
import type { Snippet, SnippetInput } from "@/types";

interface SnippetsStore {
  snippets: Snippet[];
  loaded: boolean;
  /** Snippet editor dialog. */
  editor: { open: boolean; editing?: Snippet; prefill?: Partial<SnippetInput> };
  paletteOpen: boolean;
  load: () => Promise<void>;
  save: (input: SnippetInput, id?: string) => Promise<Snippet>;
  remove: (id: string) => Promise<void>;
  toggleFavorite: (s: Snippet) => Promise<void>;
  openEditor: (opts?: { editing?: Snippet; prefill?: Partial<SnippetInput> }) => void;
  closeEditor: () => void;
  setPalette: (open: boolean) => void;
}

export const useSnippets = create<SnippetsStore>((set, get) => ({
  snippets: [],
  loaded: false,
  editor: { open: false },
  paletteOpen: false,
  load: async () => {
    try {
      set({ snippets: await api.listSnippets(), loaded: true });
    } catch (e) {
      set({ loaded: true });
      useUi.getState().toast(errorMessage(e), "danger");
    }
  },
  save: async (input, id) => {
    const s = id ? await api.updateSnippet(id, input) : await api.createSnippet(input);
    await get().load();
    return s;
  },
  remove: async (id) => {
    await api.deleteSnippet(id);
    await get().load();
  },
  toggleFavorite: async (s) => {
    set({ snippets: get().snippets.map((x) => (x.id === s.id ? { ...x, favorite: !x.favorite } : x)) });
    await api.setSnippetFavorite(s.id, !s.favorite).catch(() => undefined);
  },
  openEditor: (opts) => set({ editor: { open: true, ...opts } }),
  closeEditor: () => set({ editor: { open: false } }),
  setPalette: (paletteOpen) => set({ paletteOpen }),
}));

export { CATEGORY_LABEL, matchesSnippet } from "@/lib/snippets";

/** Connected terminal tabs a snippet can target (active tab first). */
export function snippetTargets() {
  const { tabs, activeTabId } = useSessions.getState();
  const live = tabs.filter((t) => t.state === "connected");
  return [...live.filter((t) => t.id === activeTabId), ...live.filter((t) => t.id !== activeTabId)];
}

/**
 * Insert a snippet WITHOUT executing it. If the text is multi-line and the
 * shell lacks bracketed paste (pasting would run each line), ask first.
 */
export async function insertSnippet(s: Snippet, tabId: string): Promise<boolean> {
  const ui = useUi.getState();
  const tab = useSessions.getState().tabs.find((t) => t.id === tabId);
  if (!tab || tab.state !== "connected") {
    ui.toast("That terminal isn't connected", "warning");
    return false;
  }
  if (!terminalEngine.pasteIsInert(tabId, s.command)) {
    const ok = await ui.confirmAsync({
      title: "This may run immediately",
      body: `“${s.title}” has several lines and ${tab.title} doesn't use bracketed paste, so each line may run as it's inserted. Insert anyway?`,
      code: s.command,
      confirmLabel: "Insert anyway",
      danger: true,
    });
    if (!ok) return false;
  }
  terminalEngine.insert(tabId, s.command);
  void api.markSnippetUsed(s.id).catch(() => undefined);
  return true;
}

/** Execute a snippet after an explicit confirmation showing the exact command and target. */
export async function runSnippet(s: Snippet, tabId: string): Promise<boolean> {
  const ui = useUi.getState();
  const tab = useSessions.getState().tabs.find((t) => t.id === tabId);
  if (!tab || tab.state !== "connected") {
    ui.toast("That terminal isn't connected", "warning");
    return false;
  }
  const lines = s.command.split(/\r?\n/).length;
  const ok = await ui.confirmAsync({
    title: `Run “${s.title}” on ${tab.title}?`,
    body: `${lines > 1 ? `${lines} lines will be sent. ` : ""}This executes immediately${tab.kind === "local" ? " on this PC" : " on the server"}, exactly as shown:`,
    code: s.command,
    confirmLabel: "Run",
    danger: true,
  });
  if (!ok) return false;
  const done = terminalEngine.run(tabId, s.command);
  if (done) void api.markSnippetUsed(s.id).catch(() => undefined);
  return done;
}
