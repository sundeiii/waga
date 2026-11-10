import { motion } from "motion/react";
import * as DM from "@radix-ui/react-dropdown-menu";
import { Copy, FolderTree, Gauge, MoreHorizontal, Pencil, Plug, PlugZap, Star, Trash2, KeyRound } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge, StatusDot, STATE_META } from "@/components/ui/misc";
import { terminalEngine } from "@/services/terminalEngine";
import { api, errorMessage } from "@/services/api";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { useSessions, type TabState } from "@/stores/sessions";
import { cn, displayEndpoint, relativeTime } from "@/lib/utils";
import type { ServerView } from "@/types";
import { PROFILE_KINDS } from "./ServerDialog";
import { Meter } from "@/components/monitoring/Charts";
import { diskPercent, formatUptime, memPercent, rootDisk, useMonitoring } from "@/stores/monitoring";

/** Best state across this server's tabs (connected > connecting > failed > idle). */
export function useServerState(serverId: string): TabState | "idle" {
  return useSessions((s) => {
    const tabs = s.tabs.filter((t) => t.serverId === serverId);
    if (!tabs.length) return "idle";
    const order: TabState[] = ["connected", "awaitingHostKey", "awaitingSecret", "authenticating", "connecting", "reconnecting", "hostKeyMismatch", "authFailed", "timeout", "error", "hostKeyRejected", "disconnected"];
    return order.find((st) => tabs.some((t) => t.state === st)) ?? "idle";
  });
}

export function useServerActions(server: ServerView) {
  const setPage = useUi((s) => s.setPage);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const askConfirm = useUi((s) => s.askConfirm);
  const toast = useUi((s) => s.toast);
  const { remove, duplicate, toggleFavorite, refresh } = useServers();

  return {
    connect: () => {
      setPage("terminal");
      void terminalEngine.openTab(server);
    },
    disconnect: () => {
      for (const t of useSessions.getState().tabs.filter((t) => t.serverId === server.id)) terminalEngine.disconnect(t.id);
    },
    edit: () => openServerDialog({ editing: server }),
    monitoring: () => {
      useMonitoring.getState().autoStart(server.id);
      setPage("monitoring");
    },
    duplicate: async () => {
      try {
        const d = await duplicate(server.id);
        toast(`Duplicated as ${d.name}. Credentials were not copied.`, "success");
      } catch (e) {
        toast(errorMessage(e), "danger");
      }
    },
    favorite: () => toggleFavorite(server.id),
    forgetCredentials: () =>
      askConfirm({
        title: `Forget credentials for ${server.name}?`,
        body: "Removes the saved password and key passphrase from Windows Credential Manager. You'll be asked again on the next connection.",
        confirmLabel: "Forget credentials",
        danger: true,
        onConfirm: async () => {
          await api.deleteServerCredentials(server.id);
          await refresh();
          toast("Credentials removed", "success");
        },
      }),
    remove: () =>
      askConfirm({
        title: `Remove ${server.name}?`,
        body: "Deletes the profile and its stored credentials. Open sessions stay open until closed. Trusted host keys are kept. This cannot be undone.",
        confirmLabel: "Remove server",
        danger: true,
        onConfirm: async () => {
          await remove(server.id);
          toast(`Removed ${server.name}`, "success");
        },
      }),
  };
}

const menuItem =
  "flex cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-muted outline-none data-[highlighted]:bg-white/[0.06] data-[highlighted]:text-fg data-[disabled]:opacity-40 [&_svg]:size-4";

export function ServerMenu({ server }: { server: ServerView }) {
  const a = useServerActions(server);
  const state = useServerState(server.id);
  return (
    <DM.Root>
      <DM.Trigger asChild>
        <Button variant="ghost" size="icon" aria-label={`Actions for ${server.name}`}>
          <MoreHorizontal />
        </Button>
      </DM.Trigger>
      <DM.Portal>
        <DM.Content align="end" sideOffset={6} className="glass-elevated z-50 w-56 rounded-xl p-1.5 data-[state=open]:animate-[fade-in_140ms_ease-out]">
          <DM.Item className={menuItem} onSelect={a.connect}>
            <Plug /> Connect
          </DM.Item>
          {state === "connected" && (
            <DM.Item className={menuItem} onSelect={a.disconnect}>
              <PlugZap /> Disconnect
            </DM.Item>
          )}
          <DM.Item className={menuItem} disabled={state !== "connected"} onSelect={() => useUi.getState().openSftp(server.id)}>
            <FolderTree /> Open SFTP
          </DM.Item>
          <DM.Item className={menuItem} disabled={state !== "connected"} onSelect={a.monitoring}>
            <Gauge /> View monitoring
          </DM.Item>
          <DM.Separator className="my-1 h-px bg-line" />
          <DM.Item className={menuItem} onSelect={a.edit}>
            <Pencil /> Edit configuration
          </DM.Item>
          <DM.Item className={menuItem} onSelect={a.duplicate}>
            <Copy /> Duplicate
          </DM.Item>
          <DM.Item className={menuItem} onSelect={a.favorite}>
            <Star /> {server.favorite ? "Unfavorite" : "Favorite"}
          </DM.Item>
          {(server.hasPassword || server.hasPassphrase) && (
            <DM.Item className={menuItem} onSelect={a.forgetCredentials}>
              <KeyRound /> Forget credentials
            </DM.Item>
          )}
          <DM.Separator className="my-1 h-px bg-line" />
          <DM.Item className={cn(menuItem, "text-danger data-[highlighted]:text-danger data-[highlighted]:bg-danger/10")} onSelect={a.remove}>
            <Trash2 /> Remove server
          </DM.Item>
        </DM.Content>
      </DM.Portal>
    </DM.Root>
  );
}

function Metric({ label, percent, text }: { label: string; percent?: number | null; text?: string | null }) {
  const shown = text ?? (percent != null ? `${percent.toFixed(0)}%` : null);
  return (
    <div className="min-w-0">
      <div className="text-[10.5px] uppercase tracking-wider text-faint">{label}</div>
      {percent !== undefined ? <Meter percent={percent} className="mt-1 h-1" /> : <div className="mt-1 h-1" />}
      <div className={cn("mt-1 truncate font-mono text-[11.5px] tabular-nums", shown ? "text-fg" : "text-faint")}>{shown ?? "n/a"}</div>
    </div>
  );
}

/** Latest sample, only while monitoring is actually live for this server. */
function useLiveSample(serverId: string) {
  return useMonitoring((s) => {
    const m = s.byServer[serverId];
    return m && m.state === "running" ? m.samples.at(-1) : undefined;
  });
}

export function ServerCard({ server, index = 0 }: { server: ServerView; index?: number }) {
  const state = useServerState(server.id);
  const a = useServerActions(server);
  const sample = useLiveSample(server.id);
  const setPage = useUi((s) => s.setPage);
  const kind =PROFILE_KINDS.find((k) => k.value === server.profileKind)?.label;
  const live = state === "connected";

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.28, delay: Math.min(index, 8) * 0.03, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "glass group relative flex flex-col rounded-2xl p-4 transition-[border-color,box-shadow,transform] duration-200",
        "hover:-translate-y-0.5 hover:border-line-strong hover:shadow-[0_0_0_1px_rgb(255_255_255/0.03)_inset,0_18px_48px_-16px_rgb(0_0_0/0.7),0_0_40px_-20px_var(--accent)]",
      )}
    >
      <div className="flex items-start gap-3">
        <div className={cn("grid size-10 shrink-0 place-items-center rounded-xl border border-line bg-black/25 font-mono text-[13px] font-semibold", live ? "text-success" : "text-accent")}>
          {server.name.slice(0, 2).toUpperCase()}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-[14.5px] font-semibold tracking-tight">{server.name}</h3>
            {server.favorite && <Star className="size-3.5 shrink-0 fill-warning text-warning" aria-label="Favorite" />}
          </div>
          <div className="selectable truncate font-mono text-[12px] text-muted">{displayEndpoint(server.username, server.host, server.port)}</div>
        </div>
        <ServerMenu server={server} />
      </div>

      <div className="mt-3 flex items-center gap-2 text-[12px]">
        <StatusDot state={state} />
        <span className={cn(state === "connected" ? "text-success" : state === "idle" ? "text-faint" : STATE_META[state].color.includes("danger") ? "text-danger" : "text-warning")}>
          {STATE_META[state].label}
        </span>
        <span className="text-faint">·</span>
        <span className="truncate text-faint">{server.osDetected ?? kind}</span>
      </div>

      {/* Real monitoring data only; "n/a" when not connected or not monitored. */}
      <button
        className="mt-4 grid grid-cols-4 gap-3 rounded-lg text-left"
        title={sample ? "Open monitoring" : "Metrics appear while a session is open and monitoring is on"}
        onClick={() => sample && setPage("monitoring")}
      >
        <Metric label="CPU" percent={sample?.cpuPercent ?? null} />
        <Metric label="RAM" percent={memPercent(sample)} />
        <Metric label="Disk" percent={diskPercent(rootDisk(sample))} />
        <Metric label="Uptime" text={sample ? formatUptime(sample.uptimeSecs) : null} />
      </button>

      <div className="mt-4 flex min-h-5 flex-wrap items-center gap-1.5">
        {server.groupName && <Badge tone="accent">{server.groupName}</Badge>}
        {server.tags.slice(0, 4).map((t) => (
          <Badge key={t}>#{t}</Badge>
        ))}
        {server.tags.length > 4 && <Badge>+{server.tags.length - 4}</Badge>}
      </div>

      <div className="mt-4 flex items-center justify-between border-t border-line pt-3">
        <span className="text-[11.5px] text-faint">Last connected · {relativeTime(server.lastConnectedAt)}</span>
        {live ? (
          <Button size="sm" variant="secondary" onClick={a.connect}>
            <Plug /> New tab
          </Button>
        ) : (
          <Button size="sm" variant="primary" onClick={a.connect}>
            <Plug /> Connect
          </Button>
        )}
      </div>
    </motion.div>
  );
}
