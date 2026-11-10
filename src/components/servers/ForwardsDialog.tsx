import { useEffect, useState } from "react";
import { ArrowRightLeft, Copy, Play, Square, Trash2 } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { Badge, StatusDot } from "@/components/ui/misc";
import { api, errorMessage } from "@/services/api";
import { useForwards } from "@/stores/forwards";
import { useServers } from "@/stores/servers";
import { connectedServerIds, useSessions } from "@/stores/sessions";
import { useUi } from "@/stores/ui";
import { formatBytes } from "@/stores/monitoring";
import { validateHost } from "@/lib/validation";
import type { SavedForward } from "@/types";

const blank = { label: "", localPort: "", remoteHost: "localhost", remotePort: "", autoStart: false };

export function ForwardsDialog() {
  const serverId = useForwards((s) => s.dialogServerId);
  const close = useForwards((s) => s.closeDialog);
  const active = useForwards((s) => s.active);
  const server = useServers((s) => s.servers.find((x) => x.id === serverId));
  const connected = useSessions((s) => (serverId ? connectedServerIds(s.tabs).has(serverId) : false));
  const toast = useUi((s) => s.toast);
  const [saved, setSaved] = useState<SavedForward[]>([]);
  const [form, setForm] = useState(blank);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    if (serverId) setSaved(await api.listSavedForwards(serverId).catch(() => []));
  };
  useEffect(() => {
    setForm(blank);
    setError(null);
    void load();
    void useForwards.getState().refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverId]);

  if (!serverId || !server) return null;
  const running = Object.values(active).filter((f) => f.serverId === serverId);
  const runningFor = (id: string) => running.find((r) => r.savedId === id);

  const add = async () => {
    const lp = form.localPort === "" ? 0 : Number(form.localPort);
    const rp = Number(form.remotePort);
    const hostErr = validateHost(form.remoteHost);
    if (hostErr) return setError(`Destination: ${hostErr}`);
    if (!Number.isInteger(rp) || rp < 1 || rp > 65535) return setError("Destination port must be 1–65535");
    if (!Number.isInteger(lp) || lp < 0 || lp > 65535) return setError("Local port must be 0–65535 (0 = any free port)");
    try {
      await api.saveForward(serverId, { label: form.label, localPort: lp, remoteHost: form.remoteHost.trim(), remotePort: rp, autoStart: form.autoStart });
      setForm(blank);
      setError(null);
      await load();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const start = async (f: SavedForward) => {
    try {
      const st = await api.startForward(serverId, f.id);
      useForwards.setState((s) => ({ active: { ...s.active, [st.id]: st } }));
      toast(`Listening on 127.0.0.1:${st.localPort} → ${f.remoteHost}:${f.remotePort}`, "success");
    } catch (e) {
      toast(errorMessage(e), "danger");
    }
  };

  const stop = async (id: string) => {
    await api.stopForward(id).catch(() => undefined);
  };

  const copyAddr = async (port: number) => {
    await navigator.clipboard.writeText(`127.0.0.1:${port}`).catch(() => undefined);
    toast("Address copied", "success");
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && close()}
      className="w-[min(720px,calc(100vw-48px))]"
      icon={
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          <ArrowRightLeft className="size-5" />
        </div>
      }
      title={`Port forwarding · ${server.name}`}
      description={
        <>
          Local forwards (like <code className="font-mono">ssh -L</code>) through this server's open SSH session. Ports listen on <b className="text-fg">127.0.0.1 only</b>, so nothing on your network can use them.
          {!connected && <span className="mt-1 block text-warning">Not connected — connect to start forwards.</span>}
        </>
      }
    >
      <div className="space-y-4 pb-4">
        {saved.length > 0 && (
          <div className="divide-y divide-line overflow-hidden rounded-xl border border-line">
            {saved.map((f) => {
              const r = runningFor(f.id);
              return (
                <div key={f.id} className="flex items-center gap-3 px-3 py-2.5 text-[13px]">
                  <StatusDot state={r ? "connected" : "idle"} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono">
                        127.0.0.1:{r ? r.localPort : f.localPort || "auto"} → {f.remoteHost}:{f.remotePort}
                      </span>
                      {f.label && <span className="truncate text-muted">{f.label}</span>}
                      {f.autoStart && <Badge tone="accent">auto</Badge>}
                    </div>
                    {r && (
                      <div className="text-[11.5px] text-faint">
                        {r.activeConnections} open · {r.totalConnections} total · {formatBytes(r.bytes)}
                      </div>
                    )}
                  </div>
                  {r && (
                    <Button variant="ghost" size="icon" className="size-7" onClick={() => void copyAddr(r.localPort)} aria-label="Copy local address">
                      <Copy />
                    </Button>
                  )}
                  {r ? (
                    <Button size="sm" variant="secondary" onClick={() => void stop(r.id)}>
                      <Square /> Stop
                    </Button>
                  ) : (
                    <Button size="sm" variant="primary" disabled={!connected} onClick={() => void start(f)}>
                      <Play /> Start
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 hover:text-danger"
                    aria-label="Delete forward"
                    onClick={async () => {
                      await api.deleteSavedForward(f.id);
                      await load();
                    }}
                  >
                    <Trash2 />
                  </Button>
                </div>
              );
            })}
          </div>
        )}

        <div className="rounded-xl border border-line bg-black/15 p-3">
          <div className="mb-2 text-[12px] font-medium text-muted">Add forward</div>
          <div className="grid grid-cols-[110px_auto_1fr_100px] items-center gap-2">
            <Input aria-label="Local port" placeholder="local port" value={form.localPort} inputMode="numeric" onChange={(e) => setForm({ ...form, localPort: e.target.value.replace(/\D/g, "") })} className="font-mono" />
            <span className="text-faint">→</span>
            <Input aria-label="Destination host (as seen from the server)" placeholder="host as seen from server" value={form.remoteHost} onChange={(e) => setForm({ ...form, remoteHost: e.target.value })} className="font-mono" />
            <Input aria-label="Destination port" placeholder="port" value={form.remotePort} inputMode="numeric" onChange={(e) => setForm({ ...form, remotePort: e.target.value.replace(/\D/g, "") })} className="font-mono" />
          </div>
          <div className="mt-2 flex items-center gap-3">
            <Input aria-label="Label" placeholder="Label (optional), e.g. Postgres" value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} className="flex-1" />
            <label className="flex items-center gap-2 text-[12.5px] text-muted">
              <Switch checked={form.autoStart} onCheckedChange={(v) => setForm({ ...form, autoStart: v })} label="Start automatically on connect" />
              Auto-start
            </label>
            <Button variant="primary" size="sm" onClick={() => void add()}>
              Add
            </Button>
          </div>
          {error && <p className="mt-2 text-[12px] text-danger">{error}</p>}
          <p className="mt-2 text-[11.5px] text-faint">Leave the local port empty to pick any free port. "localhost" means the server itself.</p>
        </div>
      </div>
    </Dialog>
  );
}
