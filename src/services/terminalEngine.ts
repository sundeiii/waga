/**
 * Terminal engine. Owns xterm.js instances and backend session wiring
 * OUTSIDE React: terminal output goes straight from the IPC channel into
 * xterm without touching component state. React only sees tab metadata.
 */
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { api, Channel, errorMessage } from "./api";
import { useSessions, type TabState } from "@/stores/sessions";
import { useSettings } from "@/stores/settings";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { useMonitoring } from "@/stores/monitoring";
import type { AppSettings, LocalProfile, ServerView, SessionEvent } from "@/types";

export const THEMES: Record<string, ITheme> = {
  yoru: {
    background: "#0c0e14",
    foreground: "#e6e8ee",
    cursor: "#c9c0ff",
    cursorAccent: "#0c0e14",
    selectionBackground: "#a99cf640",
    black: "#1b1e27",
    red: "#f17c87",
    green: "#49c99a",
    yellow: "#f2b76b",
    blue: "#7aa2f7",
    magenta: "#b9a7ff",
    cyan: "#6fd3d8",
    white: "#c8ccd8",
    brightBlack: "#5a6074",
    brightRed: "#ff98a2",
    brightGreen: "#6fe0b5",
    brightYellow: "#ffcd8a",
    brightBlue: "#9dbcff",
    brightMagenta: "#d3c7ff",
    brightCyan: "#93e6ea",
    brightWhite: "#f4f5f7",
  },
  midnight: {
    background: "#05060a",
    foreground: "#d5d9e3",
    cursor: "#8b9cf6",
    selectionBackground: "#8b9cf640",
    black: "#15171e",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#e5c07b",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#abb2bf",
    brightBlack: "#4b5263",
    brightRed: "#ef8a92",
    brightGreen: "#b1d99a",
    brightYellow: "#f0d49b",
    brightBlue: "#86c2f5",
    brightMagenta: "#d79ceb",
    brightCyan: "#7cc8d2",
    brightWhite: "#ffffff",
  },
  paper: {
    background: "#f6f5f2",
    foreground: "#2a2c33",
    cursor: "#6c5ce7",
    selectionBackground: "#6c5ce733",
    black: "#2a2c33",
    red: "#c43e4f",
    green: "#2f8a5f",
    yellow: "#a86b12",
    blue: "#3366cc",
    magenta: "#7d4cdb",
    cyan: "#1b8a96",
    white: "#d9d7d0",
    brightBlack: "#6d707c",
    brightRed: "#d9576a",
    brightGreen: "#3aa673",
    brightYellow: "#c4821b",
    brightBlue: "#4d7fe0",
    brightMagenta: "#9466ee",
    brightCyan: "#22a3b1",
    brightWhite: "#ffffff",
  },
};

type Target = { kind: "ssh"; server: ServerView } | { kind: "local"; profile: LocalProfile };

interface Entry {
  tabId: string;
  target: Target;
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  el: HTMLDivElement;
  sessionId: string | null;
  state: TabState;
  everConnected: boolean;
  userClosed: boolean;
  reconnectAttempts: number;
  reconnectTimer?: ReturnType<typeof setTimeout>;
}

// Kept on globalThis so live terminals survive a module re-evaluation
// (dev hot reload) instead of leaving the tab bar with orphaned tabs.
const G = globalThis as unknown as { __yoruTerminals?: Map<string, Entry> };
const entries: Map<string, Entry> = (G.__yoruTerminals ??= new Map<string, Entry>());
let host: HTMLElement | null = null;
let fontsReady: Promise<unknown> | null = null;

const TERMINAL_FAILURES: TabState[] = ["authFailed", "timeout", "hostKeyRejected", "hostKeyMismatch", "error", "disconnected"];

const FALLBACK_FONTS = `"JetBrains Mono Variable", "Cascadia Code", Consolas, monospace`;

/** "#rrggbb" + alpha -> "#rrggbbaa" (other formats returned unchanged). */
export function withAlpha(color: string | undefined, alpha: number): string | undefined {
  if (!color || !/^#[0-9a-fA-F]{6}$/.test(color)) return color;
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  return `${color}${a.toString(16).padStart(2, "0")}`;
}

/** Yoru settings, overridden per local tab by its Windows Terminal profile. */
function termOptions(s: AppSettings, target?: Target) {
  let theme: ITheme = THEMES[s.terminal.theme] ?? THEMES.yoru;
  let fontFamily = `"${s.terminal.fontFamily}", ${FALLBACK_FONTS}`;
  let opacity = s.terminal.opacity;
  if (target?.kind === "local") {
    const p = target.profile;
    if (p.fontFace) fontFamily = `"${p.fontFace.replace(/"/g, "")}", ${FALLBACK_FONTS}`;
    if (p.theme) theme = { ...theme, ...(p.theme as ITheme) };
    if (p.opacity != null) opacity = p.opacity;
  }
  return {
    fontFamily,
    fontSize: s.terminal.fontSize,
    cursorStyle: s.terminal.cursorStyle,
    cursorBlink: s.terminal.cursorBlink,
    scrollback: s.terminal.scrollback,
    // Translucent background lets the window's Acrylic/Mica show through.
    theme: { ...theme, background: withAlpha(theme.background, opacity) },
  };
}

const targetId = (t: Target) => (t.kind === "ssh" ? t.server.id : "");
const targetTitle = (t: Target) => (t.kind === "ssh" ? t.server.name : t.profile.name);

/** Strip control characters before writing our own status lines into the terminal. */
function sanitize(s: string) {
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\u0000-\u001f\u007f-\u009f]/g, " ");
}

function note(e: Entry, text: string, tone: "dim" | "warn" | "err" = "dim") {
  const code = tone === "dim" ? "2" : tone === "warn" ? "33" : "31";
  e.term.write(`\r\n\x1b[${code}m── ${sanitize(text)} ──\x1b[0m\r\n`);
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    useUi.getState().toast("Clipboard is unavailable", "warning");
  }
}

async function pasteInto(term: Terminal) {
  try {
    const text = await navigator.clipboard.readText();
    if (text) term.paste(text);
  } catch {
    useUi.getState().toast("Clipboard is unavailable", "warning");
  }
}

function patch(tabId: string, p: Parameters<ReturnType<typeof useSessions.getState>["patchTab"]>[1]) {
  useSessions.getState().patchTab(tabId, p);
}

export const terminalEngine = {
  /** Called by <TerminalHost> once its container is mounted. */
  attachHost(el: HTMLElement) {
    host = el;
    for (const e of entries.values()) if (!e.el.isConnected) el.appendChild(e.el);
  },

  openTab(server: ServerView) {
    return this.create({ kind: "ssh", server });
  },

  openLocal(profile: LocalProfile) {
    return this.create({ kind: "local", profile });
  },

  /** Never fails silently: any error is shown to the user. */
  async create(target: Target) {
    try {
      return await this.createInner(target);
    } catch (err) {
      console.error("Could not open terminal", err);
      useUi.getState().toast(`Could not open terminal: ${err instanceof Error ? err.message : String(err)}`, "danger");
    }
  },

  async createInner(target: Target) {
    const settings = useSettings.getState().settings;
    if (!settings) throw new Error("settings not loaded yet");
    // Self-heal if the module was re-instantiated (e.g. hot reload) after the host mounted.
    if (!host?.isConnected) host = document.querySelector<HTMLElement>("[data-terminal-host]");
    if (!host) throw new Error("terminal area not ready");
    fontsReady ??= document.fonts.load(`${settings.terminal.fontSize}px "JetBrains Mono Variable"`).catch(() => undefined);
    await fontsReady;
    if (target.kind === "local" && target.profile.fontFace) {
      // Locally installed font (e.g. a Nerd Font from Windows Terminal).
      await document.fonts.load(`${settings.terminal.fontSize}px "${target.profile.fontFace}"`).catch(() => undefined);
    }

    const tabId = crypto.randomUUID();
    const el = document.createElement("div");
    el.className = "xterm-host absolute inset-0";
    // Laid out but invisible until activated: xterm must be able to measure
    // character size when opening (display:none would make that fail).
    el.style.visibility = "hidden";
    host.appendChild(el);

    const term = new Terminal({
      ...termOptions(settings, target),
      allowTransparency: true,
      allowProposedApi: false,
      convertEol: false,
      macOptionIsMeta: false,
      rightClickSelectsWord: false,
      smoothScrollDuration: 0,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    // Links in remote output are untrusted: Ctrl+click copies, never auto-opens.
    term.loadAddon(
      new WebLinksAddon((ev, uri) => {
        if (ev.ctrlKey) {
          void copy(uri);
          useUi.getState().toast("Link copied to clipboard", "info");
        }
      }),
    );
    term.open(el);
    // Paint the padding around the grid in the theme's background so a
    // Windows Terminal scheme fills the whole pane (no mismatched frame).
    el.style.background = term.options.theme?.background ?? "";

    const entry: Entry = {
      tabId,
      target,
      term,
      fit,
      search,
      el,
      sessionId: null,
      state: "connecting",
      everConnected: false,
      userClosed: false,
      reconnectAttempts: 0,
    };
    entries.set(tabId, entry);

    // Only Ctrl+Shift+C / Ctrl+Shift+V are intercepted; Ctrl+C etc. go to the remote.
    term.attachCustomKeyEventHandler((ev) => {
      if (ev.type !== "keydown" || !ev.ctrlKey || !ev.shiftKey) return true;
      if (ev.code === "KeyC") {
        const sel = term.getSelection();
        if (sel) void copy(sel);
        return false;
      }
      if (ev.code === "KeyV") {
        void pasteInto(term);
        return false;
      }
      return true;
    });
    el.addEventListener("contextmenu", (ev) => {
      const s = useSettings.getState().settings;
      if (!s?.terminal.rightClickPaste) return;
      ev.preventDefault();
      if (term.hasSelection()) {
        void copy(term.getSelection());
        term.clearSelection();
      } else {
        void pasteInto(term);
      }
    });
    term.onSelectionChange(() => {
      if (useSettings.getState().settings?.terminal.copyOnSelect && term.hasSelection()) void copy(term.getSelection());
    });
    term.onData((data) => {
      // Local shells accept input from the start (ConPTY's startup cursor query
      // must be answered even if its output races the "connected" event).
      const ready = entry.state === "connected" || (entry.target.kind === "local" && entry.state === "connecting");
      if (entry.sessionId && ready) {
        api.sshWrite(entry.sessionId, data).catch(() => undefined);
      }
    });
    term.onResize(({ cols, rows }) => {
      if (entry.sessionId && entry.state === "connected") api.sshResize(entry.sessionId, cols, rows).catch(() => undefined);
    });

    useSessions.getState().addTab({
      id: tabId,
      kind: target.kind,
      serverId: targetId(target),
      title: targetTitle(target),
      state: "connecting",
      message: null,
      sessionId: null,
    });
    this.activate(tabId);
    this.start(tabId);
    return tabId;
  },

  start(tabId: string) {
    const e = entries.get(tabId);
    if (!e) return;
    clearTimeout(e.reconnectTimer);
    if (e.target.kind === "ssh") e.target = { kind: "ssh", server: useServers.getState().byId(e.target.server.id) ?? e.target.server };
    const sid = crypto.randomUUID();
    e.sessionId = sid;
    e.state = "connecting";
    e.userClosed = false;
    patch(tabId, { sessionId: sid, state: "connecting", message: null, title: targetTitle(e.target) });
    this.fit(tabId);

    const events = new Channel<SessionEvent>();
    events.onmessage = (ev) => this.onEvent(tabId, sid, ev);
    const output = new Channel<ArrayBuffer>();
    output.onmessage = (buf) => {
      if (e.sessionId !== sid) return;
      e.term.write(buf instanceof ArrayBuffer ? new Uint8Array(buf) : Uint8Array.from(buf as unknown as number[]));
    };

    const open =
      e.target.kind === "ssh"
        ? api.sshOpen(sid, e.target.server.id, e.term.cols, e.term.rows, events, output)
        : api.localOpen(sid, e.target.profile.id, e.term.cols, e.term.rows, events, output);
    open.catch((err) => {
      if (e.sessionId !== sid) return;
      e.state = "error";
      patch(tabId, { state: "error", message: errorMessage(err) });
      note(e, errorMessage(err), "err");
    });
  },

  onEvent(tabId: string, sid: string, ev: SessionEvent) {
    const e = entries.get(tabId);
    if (!e || e.sessionId !== sid) return; // stale session
    const sessions = useSessions.getState();
    switch (ev.type) {
      case "status": {
        e.state = ev.state;
        patch(tabId, { state: ev.state, message: ev.message });
        if (ev.state === "connected") {
          e.everConnected = true;
          e.reconnectAttempts = 0;
          sessions.clearPrompts(tabId);
          if (e.target.kind === "ssh") {
            void useServers.getState().refresh();
            // Monitoring piggybacks on this authenticated connection (no extra login).
            if (useSettings.getState().settings?.monitoring.enabled) useMonitoring.getState().autoStart(e.target.server.id);
          }
          // Sync size in case the pane changed while connecting.
          api.sshResize(sid, e.term.cols, e.term.rows).catch(() => undefined);
          if (useSessions.getState().activeTabId === tabId) e.term.focus();
        }
        if (TERMINAL_FAILURES.includes(ev.state)) {
          sessions.clearPrompts(tabId);
          if (ev.message) note(e, ev.message, ev.state === "disconnected" ? "dim" : ev.state === "hostKeyMismatch" ? "err" : "warn");
          this.maybeAutoReconnect(e, ev.state);
        }
        break;
      }
      case "hostKeyPrompt": {
        const { type: _t, ...rest } = ev;
        void _t;
        sessions.pushPrompt({ ...rest, type: "hostKey", tabId, sessionId: sid });
        break;
      }
      case "secretPrompt":
        sessions.pushPrompt({ type: "secret", tabId, sessionId: sid, kind: ev.kind, prompt: ev.prompt, attempt: ev.attempt });
        break;
    }
  },

  maybeAutoReconnect(e: Entry, state: TabState) {
    const recoverable = state === "disconnected" || state === "timeout" || state === "error";
    if (e.target.kind !== "ssh" || !e.target.server.autoReconnect || e.userClosed || !e.everConnected || !recoverable) return;
    if (e.reconnectAttempts >= 5) {
      note(e, "Auto-reconnect gave up after 5 attempts", "warn");
      return;
    }
    e.reconnectAttempts += 1;
    const delay = Math.min(30_000, 2000 * 2 ** (e.reconnectAttempts - 1));
    e.state = "reconnecting";
    patch(e.tabId, { state: "reconnecting", message: `Reconnecting in ${Math.round(delay / 1000)}s (attempt ${e.reconnectAttempts}/5)` });
    e.reconnectTimer = setTimeout(() => {
      if (entries.has(e.tabId) && e.state === "reconnecting") this.start(e.tabId);
    }, delay);
  },

  reconnect(tabId: string) {
    const e = entries.get(tabId);
    if (!e) return;
    if (e.sessionId && e.state !== "disconnected") void api.sshClose(e.sessionId).catch(() => undefined);
    e.reconnectAttempts = 0;
    note(e, e.target.kind === "local" ? "Restarting" : "Reconnecting");
    this.start(tabId);
  },

  disconnect(tabId: string) {
    const e = entries.get(tabId);
    if (!e?.sessionId) return;
    e.userClosed = true;
    clearTimeout(e.reconnectTimer);
    useSessions.getState().clearPrompts(tabId);
    void api.sshClose(e.sessionId).catch(() => undefined);
  },

  /** Always removes the tab, even if its terminal is missing (orphaned tab). */
  close(tabId: string) {
    const e = entries.get(tabId);
    if (e) {
      this.disconnect(tabId);
      e.term.dispose();
      e.el.remove();
      entries.delete(tabId);
    } else {
      const sid = useSessions.getState().tabs.find((t) => t.id === tabId)?.sessionId;
      if (sid) void api.sshClose(sid).catch(() => undefined);
    }
    useSessions.getState().removeTab(tabId);
    const next = useSessions.getState().activeTabId;
    if (next) this.activate(next);
  },

  activate(tabId: string) {
    useSessions.getState().setActive(tabId);
    for (const e of entries.values()) {
      // Never set visibility:visible here (it would override a hidden Terminal page).
      e.el.style.display = e.tabId === tabId ? "" : "none";
      e.el.style.visibility = "";
    }
    requestAnimationFrame(() => {
      this.fit(tabId);
      entries.get(tabId)?.term.focus();
    });
  },

  fit(tabId?: string | null) {
    const id = tabId ?? useSessions.getState().activeTabId;
    const e = id ? entries.get(id) : undefined;
    if (!e || !e.el.isConnected || e.el.clientWidth === 0) return;
    try {
      e.fit.fit();
    } catch {
      /* not yet laid out */
    }
  },

  focus(tabId?: string | null) {
    const id = tabId ?? useSessions.getState().activeTabId;
    if (id) entries.get(id)?.term.focus();
  },

  clear(tabId: string) {
    entries.get(tabId)?.term.clear();
  },

  search(tabId: string, query: string, dir: "next" | "prev") {
    const e = entries.get(tabId);
    if (!e || !query) return false;
    const opts = { caseSensitive: false, decorations: { matchOverviewRuler: "#a99cf6", activeMatchColorOverviewRuler: "#ffffff" } };
    return dir === "next" ? e.search.findNext(query, opts) : e.search.findPrevious(query, opts);
  },

  clearSearch(tabId: string) {
    entries.get(tabId)?.search.clearDecorations();
  },

  /** Send text as if typed/pasted (used by snippets later). Never adds a newline. */
  insert(tabId: string, text: string) {
    entries.get(tabId)?.term.paste(text);
  },

  applySettings(s: AppSettings) {
    for (const e of entries.values()) {
      Object.assign(e.term.options, termOptions(s, e.target));
      e.el.style.background = e.term.options.theme?.background ?? "";
    }
    this.fit();
  },

  connectionsForServer(serverId: string) {
    return [...entries.values()].filter((e) => targetId(e.target) === serverId);
  },
};

// Keep open terminals in sync with settings changes.
useSettings.subscribe((st, prev) => {
  if (st.settings && st.settings.terminal !== prev.settings?.terminal) terminalEngine.applySettings(st.settings);
});
