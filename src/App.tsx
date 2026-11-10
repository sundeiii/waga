import { useEffect } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Background } from "@/components/layout/Background";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import { ConfirmDialog, Toaster } from "@/components/layout/Overlays";
import { ServerDialog } from "@/components/servers/ServerDialog";
import { QuickConnectDialog } from "@/components/servers/QuickConnectDialog";
import { PromptDialogs } from "@/components/terminal/PromptDialogs";
import { Overview } from "@/pages/Overview";
import { Servers } from "@/pages/Servers";
import { TerminalPage } from "@/pages/Terminal";
import { ActivityPage } from "@/pages/Activity";
import { SettingsPage } from "@/pages/Settings";
import { SnippetsPage } from "@/pages/Snippets";
import { SnippetEditor } from "@/components/snippets/SnippetEditor";
import { SnippetPalette } from "@/components/snippets/SnippetPalette";
import { useSnippets } from "@/stores/snippets";
import { MonitoringPage } from "@/pages/Monitoring";
import { SFTPPage } from "@/pages/SFTP";
import { initTransferEvents } from "@/stores/transfers";
import { initMonitoringEvents } from "@/stores/monitoring";
import { useLocalProfiles } from "@/stores/localProfiles";
import { terminalEngine } from "@/services/terminalEngine";
import { api } from "@/services/api";
import { listen } from "@tauri-apps/api/event";
import { useSettings } from "@/stores/settings";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { YoruMark } from "@/components/layout/Logo";
import { ErrorBoundary } from "@/components/layout/ErrorBoundary";

function useGlobalShortcuts() {
  const setPage = useUi((s) => s.setPage);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const setQuickConnect = useUi((s) => s.setQuickConnect);
  const update = useSettings((s) => s.update);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Never steal keys from the terminal except Ctrl+Shift combos handled there.
      const inTerminal = (e.target as HTMLElement)?.closest?.(".xterm");
      if (!e.ctrlKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (e.shiftKey && k === "t") {
        e.preventDefault();
        e.stopPropagation();
        const p = useLocalProfiles.getState().defaultProfile();
        if (p) {
          setPage("terminal");
          void terminalEngine.openLocal(p);
        }
      } else if (e.shiftKey && k === "s") {
        e.preventDefault();
        e.stopPropagation();
        useSnippets.getState().setPalette(true);
      } else if (e.shiftKey && k === "q") {
        e.preventDefault();
        e.stopPropagation();
        setQuickConnect(true);
      } else if (!e.shiftKey && !inTerminal && k === "n") {
        e.preventDefault();
        openServerDialog();
      } else if (!e.shiftKey && !inTerminal && k === "b") {
        e.preventDefault();
        update((s) => ({ ...s, appearance: { ...s.appearance, sidebarCollapsed: !s.appearance.sidebarCollapsed } }));
      } else if (e.shiftKey && /^digit[1-8]$/.test(e.code.toLowerCase())) {
        e.preventDefault();
        const pages = ["overview", "servers", "terminal", "sftp", "monitoring", "snippets", "activity", "settings"] as const;
        setPage(pages[Number(e.code.slice(5)) - 1]);
      } else if (!e.shiftKey && e.key === ",") {
        e.preventDefault();
        setPage("settings");
      }
    };
    // Capture phase: xterm stops propagation of keys it handles, so app-level
    // Ctrl+Shift shortcuts must be seen before it. Plain Ctrl keys still go to the shell.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [setPage, openServerDialog, setQuickConnect, update]);
}

export default function App() {
  const settings = useSettings((s) => s.settings);
  const settingsError = useSettings((s) => s.error);
  const loadSettings = useSettings((s) => s.load);
  const refreshServers = useServers((s) => s.refresh);
  const page = useUi((s) => s.page);
  const sftpTarget = useUi((s) => s.sftpTarget);
  useGlobalShortcuts();

  useEffect(() => {
    // A (re)loaded window has no terminals: end sessions left by a previous load.
    // (Flag lives on window so dev hot-reloads don't close live sessions.)
    const w = window as unknown as { __yoruBooted?: boolean };
    if (!w.__yoruBooted) {
      w.__yoruBooted = true;
      void api.sessionsCloseAll().catch(() => undefined);
    }
    initMonitoringEvents();
    initTransferEvents();
    // Tray menu → "New local terminal".
    const unlisten = listen("tray://new-local", () => {
      const p = useLocalProfiles.getState().defaultProfile();
      if (p) {
        useUi.getState().setPage("terminal");
        void terminalEngine.openLocal(p);
      }
    });
    void useLocalProfiles.getState().load();
    void loadSettings();
    void refreshServers();
    return () => {
      void unlisten.then((f) => f());
    };
  }, [loadSettings, refreshServers]);

  if (!settings) {
    return (
      <div className="grid h-full place-items-center">
        <div className="flex flex-col items-center gap-3 text-muted">
          <YoruMark className="size-10 animate-pulse" />
          {settingsError && <p className="max-w-md text-center text-[13px] text-danger">{settingsError}</p>}
        </div>
      </div>
    );
  }

  const reduce = settings.appearance.reducedMotion || !settings.appearance.animations;

  return (
    <MotionConfig reducedMotion={reduce ? "always" : "user"}>
      <Background />
      <div className="relative z-10 flex h-full">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <div className="relative min-h-0 flex-1">
            {/* Terminal stays mounted so sessions and scrollback survive navigation. */}
            <ErrorBoundary>
              <TerminalPage visible={page === "terminal"} />
            </ErrorBoundary>
            <ErrorBoundary resetKey={page}>
            <AnimatePresence mode="wait" initial={false}>
              {page !== "terminal" && (
                <motion.div
                  key={page}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -4 }}
                  transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
                  className="absolute inset-0 overflow-y-auto px-5 pb-6"
                >
                  {page === "overview" && <Overview />}
                  {page === "servers" && <Servers />}
                  {page === "sftp" && <SFTPPage preferredServer={sftpTarget} />}
                  {page === "monitoring" && <MonitoringPage />}
                  {page === "snippets" && <SnippetsPage />}
                  {page === "activity" && <ActivityPage visible />}
                  {page === "settings" && <SettingsPage visible />}
                </motion.div>
              )}
            </AnimatePresence>
            </ErrorBoundary>
          </div>
        </main>
      </div>
      <ServerDialog />
      <QuickConnectDialog />
      <PromptDialogs />
      <SnippetEditor />
      <SnippetPalette />
      <ConfirmDialog />
      <Toaster />
    </MotionConfig>
  );
}
