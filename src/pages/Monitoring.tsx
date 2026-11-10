import { useEffect, useMemo, useState } from "react";
import { Gauge, Pause, Play, Plug } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge, EmptyState, Panel } from "@/components/ui/misc";
import { Select } from "@/components/ui/Input";
import { ChartCard, Legend, Meter, NetworkChart, NoData, PercentChart } from "@/components/monitoring/Charts";
import { diskPercent, formatBytes, formatRate, formatUptime, HISTORY_LIMIT, memPercent, useMonitoring } from "@/stores/monitoring";
import { useServers } from "@/stores/servers";
import { useSessions } from "@/stores/sessions";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { formatTimestamp, cn } from "@/lib/utils";
import type { MonitorState } from "@/types";

const STATE_LABEL: Record<MonitorState, { label: string; tone: "success" | "warning" | "danger" | "neutral" }> = {
  running: { label: "Live", tone: "success" },
  noConnection: { label: "Waiting for session", tone: "warning" },
  error: { label: "Error", tone: "danger" },
  stopped: { label: "Stopped", tone: "neutral" },
};

const INTERVALS = [1, 2, 5, 10, 30, 60];
const WINDOWS_MIN_INTERVAL = 3; // mirrors WINDOWS_MIN_INTERVAL_SECS in Rust

export function MonitoringPage() {
  const servers = useServers((s) => s.servers);
  const tabs = useSessions((s) => s.tabs);
  const byServer = useMonitoring((s) => s.byServer);
  const { start, pause } = useMonitoring();
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const setPage = useUi((s) => s.setPage);

  const connected = useMemo(() => new Set(tabs.filter((t) => t.state === "connected").map((t) => t.serverId)), [tabs]);
  // Servers worth listing: connected now, or with collected history.
  const candidates = servers.filter((s) => connected.has(s.id) || byServer[s.id]?.samples.length);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    if (!selected || !candidates.some((c) => c.id === selected)) setSelected(candidates[0]?.id ?? null);
  }, [candidates, selected]);

  if (!settings) return null;

  if (!settings.monitoring.enabled) {
    return (
      <Panel className="mx-auto mt-6 max-w-2xl">
        <EmptyState
          icon={<Gauge />}
          title="Monitoring is turned off"
          body="When enabled, Yoru reads CPU, memory, disk and network over your already-open SSH session. No agent, no extra logins."
          action={
            <Button variant="primary" onClick={() => update((s) => ({ ...s, monitoring: { ...s.monitoring, enabled: true } }))}>
              Enable monitoring
            </Button>
          }
        />
      </Panel>
    );
  }

  if (candidates.length === 0) {
    return (
      <Panel className="mx-auto mt-6 max-w-2xl">
        <EmptyState
          icon={<Gauge />}
          title="No server to monitor"
          body="Monitoring uses an open SSH session, so nothing is collected until you connect. Open a terminal to a server and metrics start automatically."
          action={
            <Button variant="primary" onClick={() => setPage("servers")}>
              <Plug /> Connect to a server
            </Button>
          }
        />
      </Panel>
    );
  }

  const server = servers.find((s) => s.id === selected);
  const mon = selected ? byServer[selected] : undefined;
  const samples = mon?.samples ?? [];
  const last = samples.at(-1);
  const isConnected = !!selected && connected.has(selected);
  const running = mon?.state === "running" || mon?.state === "noConnection";
  const st = STATE_LABEL[mon?.state ?? "stopped"];

  const cpuData = samples.map((s) => ({ ts: s.ts, v: s.cpuPercent }));
  const memData = samples.map((s) => ({ ts: s.ts, v: memPercent(s) }));
  const netData = samples.map((s) => ({ ts: s.ts, rx: s.netRxBps, tx: s.netTxBps }));
  const hasCpu = cpuData.some((d) => d.v != null);
  const hasMem = memData.some((d) => d.v != null);
  const hasNet = netData.some((d) => d.rx != null || d.tx != null);

  return (
    <div className="space-y-4 pb-2">
      {/* server picker + controls */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex flex-wrap gap-1">
          {candidates.map((s) => {
            const m = byServer[s.id];
            return (
              <button
                key={s.id}
                onClick={() => setSelected(s.id)}
                className={cn(
                  "flex h-8 items-center gap-2 rounded-[10px] border px-3 text-[12.5px] transition-colors",
                  s.id === selected ? "border-accent/50 bg-accent-soft text-fg" : "border-line text-muted hover:text-fg",
                )}
              >
                <span className={cn("size-1.5 rounded-full", m?.state === "running" ? "bg-success" : m?.state === "error" ? "bg-danger" : "bg-faint")} />
                {s.name}
              </button>
            );
          })}
        </div>
        <div className="ml-auto flex items-center gap-2">
          <span className="text-[12px] text-faint">Every</span>
          <div className="w-24">
            <Select
              aria-label="Polling interval"
              className="h-8 text-[12.5px]"
              value={settings.monitoring.intervalSecs}
              onChange={(e) => update((s) => ({ ...s, monitoring: { ...s.monitoring, intervalSecs: Number(e.target.value) } }))}
            >
              {[...new Set([...INTERVALS, settings.monitoring.intervalSecs])].sort((a, b) => a - b).map((i) => (
                <option key={i} value={i}>
                  {i}s
                </option>
              ))}
            </Select>
          </div>
          {selected &&
            (running ? (
              <Button size="sm" onClick={() => pause(selected)}>
                <Pause /> Pause
              </Button>
            ) : (
              <Button size="sm" variant="primary" disabled={!isConnected} onClick={() => start(selected)} title={isConnected ? undefined : "Connect a terminal session first"}>
                <Play /> Resume
              </Button>
            ))}
        </div>
      </div>

      {/* header */}
      <Panel className="flex flex-wrap items-center gap-x-8 gap-y-2 px-5 py-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-[16px] font-semibold tracking-tight">{server?.name}</h2>
            <Badge tone={st.tone}>{st.label}</Badge>
          </div>
          <div className="mt-0.5 truncate text-[12.5px] text-muted">
            {last?.os ?? server?.osDetected ?? "OS not detected yet"}
            {last?.hostname && <span className="text-faint"> · {last.hostname}</span>}
          </div>
        </div>
        <Fact label="Uptime" value={formatUptime(last?.uptimeSecs)} />
        <Fact label="Load avg" value={last?.load ? last.load.map((l) => l.toFixed(2)).join("  ") : "n/a"} hint={last?.load ? "1 / 5 / 15 min" : "Not reported on Windows"} />
        <Fact label="CPU cores" value={last?.cpuCores != null ? String(last.cpuCores) : "n/a"} />
        <Fact label="Last sample" value={mon?.lastSuccess ? new Date(mon.lastSuccess).toLocaleTimeString() : "none yet"} hint={mon?.lastSuccess ? formatTimestamp(mon.lastSuccess) : undefined} />
        {server?.profileKind === "windows" && settings.monitoring.intervalSecs < WINDOWS_MIN_INTERVAL && (
          <p className="w-full text-[12px] text-faint">Windows hosts are sampled every {WINDOWS_MIN_INTERVAL}s at most: each sample starts PowerShell on the server.</p>
        )}
        {mon?.message && mon.state !== "running" &&<p className={cn("w-full text-[12.5px]", mon.state === "error" ? "text-danger" : "text-warning")}>{mon.message}</p>}
      </Panel>

      {/* charts */}
      <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
        <ChartCard title="CPU" value={last?.cpuPercent != null ? last.cpuPercent.toFixed(1) : "—"} unit="%">
          {hasCpu ? <PercentChart data={cpuData} name="CPU" /> : <NoData>{samples.length ? "CPU % needs two samples; collecting…" : "No samples yet"}</NoData>}
        </ChartCard>
        <ChartCard
          title="Memory"
          value={memPercent(last) != null ? memPercent(last)!.toFixed(1) : "—"}
          unit="%"
          footer={last?.memTotalBytes != null ? `${formatBytes(last.memUsedBytes)} used of ${formatBytes(last.memTotalBytes)}` : undefined}
        >
          {hasMem ? <PercentChart data={memData} name="Memory" /> : <NoData>Memory usage unavailable</NoData>}
        </ChartCard>
        <ChartCard title="Network" value={<Legend items={[{ label: "↓", value: formatRate(last?.netRxBps) }, { label: "↑", value: formatRate(last?.netTxBps) }]} />} footer="All non-loopback interfaces, bytes per second">
          {hasNet ? <NetworkChart data={netData} /> : <NoData>{samples.length ? "Rate needs two samples; collecting…" : "Network counters unavailable"}</NoData>}
        </ChartCard>
        <Panel className="flex flex-col p-4">
          <h3 className="text-[12px] font-medium uppercase tracking-[0.08em] text-faint">Disks</h3>
          <div className="mt-3 flex-1 space-y-3 overflow-y-auto">
            {last?.disks.length ? (
              last.disks.map((d) => {
                const p = diskPercent(d);
                return (
                  <div key={d.mount}>
                    <div className="flex items-baseline justify-between text-[12.5px]">
                      <span className="selectable truncate font-mono">{d.mount}</span>
                      <span className="font-mono tabular-nums text-muted">
                        {formatBytes(d.usedBytes)} / {formatBytes(d.totalBytes)} · <span className="text-fg">{p?.toFixed(0)}%</span>
                      </span>
                    </div>
                    <Meter percent={p} className="mt-1.5" />
                  </div>
                );
              })
            ) : (
              <NoData>{samples.length ? "No disks reported" : "No samples yet"}</NoData>
            )}
          </div>
        </Panel>
      </div>

      <p className="text-[11.5px] text-faint">
        History covers only samples collected while monitoring was active in this window ({samples.length} of max {HISTORY_LIMIT}, about {Math.round((HISTORY_LIMIT * settings.monitoring.intervalSecs) / 60)} min). Nothing is collected while paused or disconnected, and nothing is stored on disk.
      </p>
    </div>
  );
}

function Fact({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div title={hint}>
      <div className="text-[10.5px] uppercase tracking-wider text-faint">{label}</div>
      <div className="mt-0.5 font-mono text-[13px] tabular-nums">{value}</div>
    </div>
  );
}
