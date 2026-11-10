import { motion } from "motion/react";
import { Activity, CircleDot, CloudOff, Plus, Server, SquareTerminal, Zap } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { EmptyState, Panel, SectionTitle } from "@/components/ui/misc";
import { ServerCard } from "@/components/servers/ServerCard";
import { useServers } from "@/stores/servers";
import { useSessions } from "@/stores/sessions";
import { useUi } from "@/stores/ui";
import { useMonitoring } from "@/stores/monitoring";
import { cn } from "@/lib/utils";

function Stat({ icon: Icon, label, value, hint, tone = "accent", i }: { icon: typeof Server; label: string; value: number | string; hint: string; tone?: "accent" | "success" | "warning" | "muted"; i: number }) {
  const tones = { accent: "text-accent bg-accent-soft", success: "text-success bg-success/12", warning: "text-warning bg-warning/12", muted: "text-muted bg-white/[0.06]" };
  return (
    <motion.div initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.3, delay: i * 0.04, ease: [0.22, 1, 0.36, 1] }}>
      <Panel className="flex h-full items-center gap-3.5 p-4">
        <div className={cn("grid size-10 shrink-0 place-items-center rounded-xl", tones[tone])}>
          <Icon className="size-[18px]" />
        </div>
        <div className="min-w-0">
          <div className="text-[22px] font-semibold leading-none tracking-tight tabular-nums">{value}</div>
          <div className="mt-1.5 truncate text-[12px] text-muted">{label}</div>
          <div className="truncate text-[11px] text-faint">{hint}</div>
        </div>
      </Panel>
    </motion.div>
  );
}

export function Overview() {
  const { servers, loaded } = useServers();
  const tabs = useSessions((s) => s.tabs);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const setQuickConnect = useUi((s) => s.setQuickConnect);
  const setPage = useUi((s) => s.setPage);

  const connectedIds = new Set(tabs.filter((t) => t.kind === "ssh" && t.state === "connected").map((t) => t.serverId));
  const unreachable = new Set(tabs.filter((t) => t.kind === "ssh" && ["timeout", "error"].includes(t.state) && !connectedIds.has(t.serverId)).map((t) => t.serverId));
  const monitored = useMonitoring((s) => Object.values(s.byServer).filter((m) => m.state === "running").length);
  const activeSessions =tabs.filter((t) => t.state === "connected").length;
  const sorted = [...servers].sort((a, b) => Number(b.favorite) - Number(a.favorite) || (b.lastConnectedAt ?? "").localeCompare(a.lastConnectedAt ?? ""));

  if (loaded && servers.length === 0) {
    return (
      <Panel className="mx-auto mt-6 max-w-2xl">
        <EmptyState
          icon={<Server />}
          title="Welcome to yoru"
          body="Add your first server to open real SSH sessions. Profiles stay on this machine; secrets live in Windows Credential Manager."
          action={
            <div className="flex gap-2">
              <Button variant="primary" onClick={() => openServerDialog()}>
                <Plus /> Add a server
              </Button>
              <Button onClick={() => setQuickConnect(true)}>
                <Zap /> Quick connect
              </Button>
            </div>
          }
        />
      </Panel>
    );
  }

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Stat i={0} icon={Server} label="Configured servers" value={servers.length} hint={`${servers.filter((s) => s.favorite).length} favorites`} />
        <Stat i={1} icon={CircleDot} label="Connected" value={connectedIds.size} hint="Live SSH right now" tone="success" />
        <Stat i={2} icon={Activity} label="Monitored" value={monitored} hint="Live metrics right now" tone={monitored ? "success" : "muted"} />
        <Stat i={3} icon={CloudOff} label="Unreachable" value={unreachable.size} hint="Failed this session" tone={unreachable.size ? "warning" : "muted"} />
        <Stat i={4} icon={SquareTerminal} label="Active sessions" value={activeSessions} hint={`${tabs.length} open tabs`} />
      </div>

      <div>
        <SectionTitle
          right={
            <Button variant="ghost" size="sm" onClick={() => setPage("servers")}>
              View all
            </Button>
          }
        >
          Servers
        </SectionTitle>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 2xl:grid-cols-3">
          {sorted.slice(0, 9).map((s, i) => (
            <ServerCard key={s.id} server={s} index={i} />
          ))}
        </div>
      </div>
    </div>
  );
}
