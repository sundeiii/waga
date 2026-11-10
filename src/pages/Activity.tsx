import { useEffect, useMemo, useState } from "react";
import { Activity as ActivityIcon, RefreshCw, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge, EmptyState, FilterChip, Panel } from "@/components/ui/misc";
import { api, errorMessage } from "@/services/api";
import { useUi } from "@/stores/ui";
import { formatTimestamp } from "@/lib/utils";
import type { ActivityEntry } from "@/types";

const KIND: Record<string, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" }> = {
  connectAttempt: { label: "Attempt", tone: "neutral" },
  connected: { label: "Connected", tone: "success" },
  disconnected: { label: "Disconnected", tone: "neutral" },
  authFailed: { label: "Auth failed", tone: "danger" },
  connectionError: { label: "Error", tone: "danger" },
  hostKeyTrusted: { label: "Host key trusted", tone: "accent" },
  hostKeyChanged: { label: "Host key CHANGED", tone: "danger" },
  hostKeyRejected: { label: "Host key rejected", tone: "warning" },
  serverCreated: { label: "Created", tone: "accent" },
  serverUpdated: { label: "Updated", tone: "neutral" },
  serverDeleted: { label: "Deleted", tone: "warning" },
  credentialsDeleted: { label: "Credentials removed", tone: "warning" },
  monitoringFailed: { label: "Monitoring failed", tone: "warning" },
};

export function ActivityPage({ visible }: { visible: boolean }) {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const [q, setQ] = useState("");
  const [kind, setKind] = useState<string | null>(null);
  const askConfirm = useUi((s) => s.askConfirm);
  const toast = useUi((s) => s.toast);

  const load = () =>
    api
      .listActivity(1000)
      .then(setEntries)
      .catch((e) => toast(errorMessage(e), "danger"));

  useEffect(() => {
    if (visible) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    return entries.filter((e) => (!kind || e.kind === kind) && (!s || `${e.serverName ?? ""} ${e.message}`.toLowerCase().includes(s)));
  }, [entries, q, kind]);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative w-72">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-faint" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Filter activity"
            aria-label="Filter activity"
            className="h-8 w-full rounded-[10px] border border-line bg-shade/25 pl-8 pr-2 text-[12.5px] outline-none transition-[border-color,box-shadow] placeholder:text-faint focus:border-accent/60 focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_16%,transparent)]"
          />
        </div>
        {["connected", "authFailed", "connectionError", "hostKeyChanged", "monitoringFailed"].map((k) => (
          <FilterChip key={k} active={kind === k} tone={KIND[k].tone} onClick={() => setKind(kind === k ? null : k)}>
            {KIND[k].label}
          </FilterChip>
        ))}
        <div className="ml-auto flex gap-2">
          <Button variant="ghost" size="sm" onClick={load}>
            <RefreshCw /> Refresh
          </Button>
          <Button
            variant="danger"
            size="sm"
            disabled={!entries.length}
            onClick={() =>
              askConfirm({
                title: "Clear activity history?",
                body: "Removes all local connection history. Server profiles, credentials and trusted host keys are not affected.",
                confirmLabel: "Clear history",
                danger: true,
                onConfirm: async () => {
                  await api.clearActivity();
                  await load();
                },
              })
            }
          >
            <Trash2 /> Clear history
          </Button>
        </div>
      </div>

      <Panel className="overflow-hidden">
        {filtered.length === 0 ? (
          <EmptyState icon={<ActivityIcon />} title="No activity" body="Connections, authentication results, host key decisions and profile changes appear here. Passwords and terminal contents are never recorded." />
        ) : (
          <div className="divide-y divide-line">
            {filtered.map((e) => {
              const k = KIND[e.kind] ?? { label: e.kind, tone: "neutral" as const };
              return (
                <div key={e.id} className="grid grid-cols-[170px_150px_140px_1fr] items-center gap-3 px-4 py-2.5 text-[12.5px] hover:bg-ink/[0.02]">
                  <span className="font-mono text-[11.5px] text-faint">{formatTimestamp(e.ts)}</span>
                  <span>
                    <Badge tone={k.tone}>{k.label}</Badge>
                  </span>
                  <span className="truncate text-muted">{e.serverName ?? "—"}</span>
                  <span className="selectable truncate">{e.message}</span>
                </div>
              );
            })}
          </div>
        )}
      </Panel>
    </div>
  );
}
