import { motion } from "motion/react";
import {
  Activity,
  ChevronsLeft,
  ChevronsRight,
  FolderTree,
  Gauge,
  LayoutDashboard,
  Server,
  Settings,
  SquareCode,
  SquareTerminal,
} from "lucide-react";
import type { Page } from "@/types";
import { useUi } from "@/stores/ui";
import { useSettings } from "@/stores/settings";
import { useSessions } from "@/stores/sessions";
import { cn } from "@/lib/utils";
import { YoruWordmark } from "./Logo";

export const NAV: { id: Page; label: string; icon: typeof Server; soon?: boolean }[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard },
  { id: "servers", label: "Servers", icon: Server },
  { id: "terminal", label: "Terminal", icon: SquareTerminal },
  { id: "sftp", label: "SFTP", icon: FolderTree },
  { id: "monitoring", label: "Monitoring", icon: Gauge },
  { id: "snippets", label: "Snippets", icon: SquareCode },
  { id: "activity", label: "Activity", icon: Activity },
];

export function Sidebar() {
  const page = useUi((s) => s.page);
  const setPage = useUi((s) => s.setPage);
  const collapsed = useSettings((s) => s.settings?.appearance.sidebarCollapsed ?? false);
  const update = useSettings((s) => s.update);
  const tabs = useSessions((s) => s.tabs);
  const connected = tabs.filter((t) => t.state === "connected").length;
  const pending = tabs.filter((t) => ["connecting", "authenticating", "awaitingHostKey", "awaitingSecret", "reconnecting"].includes(t.state)).length;

  const toggle = () => update((s) => ({ ...s, appearance: { ...s.appearance, sidebarCollapsed: !collapsed } }));

  const item = (id: Page, label: string, Icon: typeof Server, soon?: boolean, badge?: number) => {
    const active = page === id;
    return (
      <button
        key={id}
        onClick={() => setPage(id)}
        title={collapsed ? label : undefined}
        aria-current={active ? "page" : undefined}
        className={cn(
          "group relative flex h-9 w-full items-center rounded-xl text-[13.5px] font-medium transition-colors duration-150",
          collapsed ? "justify-center px-0" : "gap-3 px-2.5",
          active ? "text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg",
        )}
      >
        {active && (
          <motion.span
            layoutId="nav-active"
            className="absolute inset-0 rounded-xl border border-line bg-white/[0.07] shadow-[0_0_0_1px_rgb(255_255_255/0.02)_inset]"
            transition={{ type: "spring", stiffness: 500, damping: 40 }}
          />
        )}
        <Icon className={cn("relative size-[18px] shrink-0", active && "text-accent")} />
        {!collapsed && <span className="relative flex-1 truncate text-left">{label}</span>}
        {!collapsed && soon && <span className="relative text-[10px] font-normal text-faint">soon</span>}
        {!!badge && (
          <span
            className={cn(
              "relative grid h-[18px] min-w-[18px] place-items-center rounded-full bg-success/20 px-1 text-[10.5px] font-semibold text-success",
              collapsed && "absolute -right-0.5 -top-0.5",
            )}
          >
            {badge}
          </span>
        )}
      </button>
    );
  };

  return (
    <motion.aside
      animate={{ width: collapsed ? 64 : 224 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      className="glass relative z-10 m-2 mr-0 flex shrink-0 flex-col overflow-hidden rounded-2xl"
    >
      <div className={cn("flex h-14 items-center", collapsed ? "justify-center" : "px-4")}>
        <YoruWordmark collapsed={collapsed} />
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 px-2 pt-2" aria-label="Main">
        {NAV.map((n) => item(n.id, n.label, n.icon, n.soon, n.id === "terminal" ? connected : undefined))}
      </nav>

      <div className="flex flex-col gap-1 border-t border-line px-2 py-2">
        {!collapsed ? (
          <div className="mb-1 rounded-xl bg-black/20 px-3 py-2.5 text-[12px]">
            <div className="flex items-center gap-2">
              <span className={cn("size-1.5 rounded-full", connected ? "bg-success shadow-[0_0_6px_var(--color-success)]" : "bg-faint")} />
              <span className="text-muted">
                {connected} active {connected === 1 ? "session" : "sessions"}
              </span>
            </div>
            {pending > 0 && <div className="mt-1 pl-3.5 text-faint">{pending} connecting…</div>}
            <div className="mt-1 pl-3.5 text-faint">Local only · no cloud sync</div>
          </div>
        ) : null}
        {item("settings", "Settings", Settings)}
        <button
          onClick={toggle}
          className="flex h-8 items-center justify-center gap-2 rounded-xl text-faint transition-colors hover:bg-white/[0.04] hover:text-fg"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
        >
          {collapsed ? <ChevronsRight className="size-4" /> : <ChevronsLeft className="size-4" />}
        </button>
      </div>
    </motion.aside>
  );
}
