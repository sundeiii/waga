import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openFile } from "@tauri-apps/plugin-dialog";
import { CloudDownload, Loader2 } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Input, Select } from "@/components/ui/Input";
import { Badge } from "@/components/ui/misc";
import { errorMessage } from "@/services/api";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { cn } from "@/lib/utils";
import { displayName } from "@/lib/paths";
import type { AuthMethod } from "@/types";

type Provider = "digitalOcean" | "hetzner" | "linode" | "aws" | "azure";

interface CloudVm {
  provider: Provider;
  id: string;
  name: string;
  host: string | null;
  privateIp: string | null;
  region: string;
  os: string | null;
  windows: boolean;
  status: string;
  suggestedUser: string;
  tags: string[];
}

const PROVIDERS: {
  id: Provider;
  label: string;
  token: boolean;
  help: string;
}[] = [
  {
    id: "digitalOcean",
    label: "DigitalOcean",
    token: true,
    help: "API token with read scope: Control Panel → API → Generate New Token.",
  },
  {
    id: "hetzner",
    label: "Hetzner",
    token: true,
    help: "Read-only API token: Cloud Console → your project → Security → API tokens.",
  },
  {
    id: "linode",
    label: "Linode",
    token: true,
    help: "Personal access token with Linodes: Read Only.",
  },
  {
    id: "aws",
    label: "AWS",
    token: false,
    help: "Uses your signed-in AWS CLI (aws configure / aws sso login). yoru never sees those credentials.",
  },
  {
    id: "azure",
    label: "Azure",
    token: false,
    help: "Uses your signed-in Azure CLI (az login). yoru never sees those credentials.",
  },
];

export function CloudImportDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const servers = useServers((s) => s.servers);
  const refresh = useServers((s) => s.refresh);
  const toast = useUi((s) => s.toast);
  const [provider, setProvider] = useState<Provider>("digitalOcean");
  const [token, setToken] = useState("");
  const [remember, setRemember] = useState(true);
  const [hasToken, setHasToken] = useState(false);
  const [region, setRegion] = useState("");
  const [vms, setVms] = useState<CloudVm[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [users, setUsers] = useState<Record<string, string>>({});
  const [auth, setAuth] = useState<AuthMethod>("agent");
  const [keyPath, setKeyPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const meta = PROVIDERS.find((p) => p.id === provider)!;

  useEffect(() => {
    if (!open) return;
    setVms(null);
    setError(null);
    setToken("");
    setSelected(new Set());
    if (meta.token)
      void invoke<boolean>("cloud_has_token", { provider })
        .then(setHasToken)
        .catch(() => setHasToken(false));
    else setHasToken(false);
  }, [open, provider, meta.token]);

  const exists = useMemo(() => new Set(servers.map((s) => `${s.host.toLowerCase()}|${s.username}`)), [servers]);
  const userFor = (vm: CloudVm) => users[vm.id] ?? vm.suggestedUser;
  const isSaved = (vm: CloudVm) => !!vm.host && exists.has(`${vm.host.toLowerCase()}|${userFor(vm)}`);

  const fetchVms = async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<CloudVm[]>("cloud_list", {
        provider,
        token: token || null,
        remember,
        region: region || null,
      });
      setVms(list);
      setSelected(new Set(list.filter((v) => v.host && !exists.has(`${v.host.toLowerCase()}|${v.suggestedUser}`)).map((v) => v.id)));
      setUsers({});
      setToken("");
      if (remember && meta.token) setHasToken(true);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const doImport = async () => {
    if (!vms) return;
    const items = vms
      .filter((v) => selected.has(v.id) && v.host)
      .map((v) => ({
        name: v.name,
        host: v.host!,
        username: userFor(v),
        windows: v.windows,
        os: v.os,
        region: v.region,
        tags: v.tags,
      }));
    setLoading(true);
    try {
      const r = await invoke<{ created: number; skipped: string[] }>("cloud_import", {
        provider,
        items,
        authMethod: auth,
        keyPath: auth === "key" ? keyPath : null,
      });
      await refresh();
      toast(`Imported ${r.created} server${r.created === 1 ? "" : "s"}${r.skipped.length ? `, skipped ${r.skipped.length}` : ""}`, r.created ? "success" : "warning");
      if (r.skipped.length) setError(r.skipped.join("\n"));
      else onClose();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      className="w-[min(860px,calc(100vw-48px))]"
      icon={
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          <CloudDownload className="size-5" />
        </div>
      }
      title="Import servers from the cloud"
      description="Lists your VMs and creates server profiles for the ones you pick. Host keys are still verified on first connect."
      footer={
        vms && (
          <>
            <span className="mr-auto text-[12px] text-faint">{selected.size} selected</span>
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" disabled={!selected.size || loading || (auth === "key" && !keyPath)} onClick={() => void doImport()}>
              Import {selected.size || ""}
            </Button>
          </>
        )
      }
    >
      <div className="space-y-3 pb-4">
        <div className="flex flex-wrap gap-1" role="tablist" aria-label="Provider">
          {PROVIDERS.map((p) => (
            <button
              key={p.id}
              role="tab"
              aria-selected={provider === p.id}
              onClick={() => setProvider(p.id)}
              className={cn("h-8 rounded-[10px] border px-3 text-[12.5px] transition-colors", provider === p.id ? "border-accent/50 bg-accent-soft text-fg" : "border-line text-muted hover:text-fg")}
            >
              {p.label}
            </button>
          ))}
        </div>
        <p className="text-[12px] text-faint">{meta.help}</p>

        <div className="flex flex-wrap items-center gap-2">
          {meta.token && (
            <>
              <Input
                type="password"
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder={hasToken ? "Saved token will be used — or paste a new one" : "API token"}
                className="min-w-64 flex-1 font-mono"
                aria-label="API token"
              />
              <label className="flex items-center gap-1.5 text-[12.5px] text-muted">
                <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} className="size-4 accent-[var(--accent)]" />
                Remember (Credential Manager)
              </label>
              {hasToken && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={async () => {
                    await invoke("cloud_forget_token", { provider });
                    setHasToken(false);
                  }}
                >
                  Forget token
                </Button>
              )}
            </>
          )}
          {provider === "aws" && (
            <Input value={region} onChange={(e) => setRegion(e.target.value.trim())} placeholder="Region (optional, e.g. eu-central-1)" className="w-64 font-mono" aria-label="AWS region" />
          )}
          <Button variant="primary" disabled={loading || (meta.token && !token && !hasToken)} onClick={() => void fetchVms()}>
            {loading && !vms ? <Loader2 className="animate-spin" /> : null} List servers
          </Button>
        </div>
        {error && <p className="whitespace-pre-wrap text-[12px] text-danger">{error}</p>}

        {vms && (
          <>
            {vms.length === 0 ? (
              <p className="py-6 text-center text-[13px] text-faint">No VMs found{provider === "aws" ? " in this region" : ""}.</p>
            ) : (
              <div className="max-h-[42vh] overflow-y-auto rounded-xl border border-line">
                <table className="w-full text-[12.5px]">
                  <thead className="sticky top-0 bg-surface/95 text-left text-[10.5px] uppercase tracking-wider text-faint backdrop-blur">
                    <tr>
                      <th className="w-8 px-3 py-2">
                        <input
                          type="checkbox"
                          aria-label="Select all"
                          className="size-4 accent-[var(--accent)]"
                          checked={vms.filter((v) => v.host).every((v) => selected.has(v.id))}
                          onChange={(e) => setSelected(new Set(e.target.checked ? vms.filter((v) => v.host).map((v) => v.id) : []))}
                        />
                      </th>
                      <th className="px-2 py-2 font-medium">Name</th>
                      <th className="px-2 py-2 font-medium">Address</th>
                      <th className="px-2 py-2 font-medium">Region · OS</th>
                      <th className="w-36 px-2 py-2 font-medium">User</th>
                    </tr>
                  </thead>
                  <tbody>
                    {vms.map((v) => (
                      <tr key={v.id} className="border-t border-line">
                        <td className="px-3 py-1.5">
                          <input
                            type="checkbox"
                            disabled={!v.host}
                            aria-label={`Select ${v.name}`}
                            className="size-4 accent-[var(--accent)]"
                            checked={selected.has(v.id)}
                            onChange={(e) => {
                              const n = new Set(selected);
                              if (e.target.checked) n.add(v.id);
                              else n.delete(v.id);
                              setSelected(n);
                            }}
                          />
                        </td>
                        <td className="px-2 py-1.5">
                          <div className="flex items-center gap-1.5">
                            <span className="truncate">{displayName(v.name)}</span>
                            {isSaved(v) && <Badge>saved</Badge>}
                            {v.status && !/running|active/i.test(v.status) && <Badge tone="warning">{v.status}</Badge>}
                          </div>
                        </td>
                        <td className="px-2 py-1.5 font-mono text-[11.5px] text-muted">{v.host ?? <span className="text-danger">no address</span>}</td>
                        <td className="truncate px-2 py-1.5 text-[11.5px] text-faint">
                          {v.region}
                          {v.os ? ` · ${v.os}` : ""}
                        </td>
                        <td className="px-2 py-1">
                          <Input value={userFor(v)} onChange={(e) => setUsers({ ...users, [v.id]: e.target.value })} className="h-7 font-mono text-[12px]" aria-label={`Username for ${v.name}`} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <div className="flex flex-wrap items-center gap-2 text-[12.5px] text-muted">
              Sign in with
              <div className="w-40">
                <Select value={auth} onChange={(e) => setAuth(e.target.value as AuthMethod)} className="h-8 text-[12.5px]" aria-label="Authentication for imported servers">
                  <option value="agent">SSH agent</option>
                  <option value="key">Private key</option>
                  <option value="password">Password (asked on connect)</option>
                </Select>
              </div>
              {auth === "key" && (
                <>
                  <Input value={keyPath} readOnly placeholder="Choose key file" className="h-8 flex-1 font-mono text-[11.5px]" aria-label="Private key file" />
                  <Button
                    size="sm"
                    onClick={async () => {
                      const f = await openFile({
                        multiple: false,
                        title: "Choose private key",
                      });
                      if (typeof f === "string") setKeyPath(f);
                    }}
                  >
                    Browse…
                  </Button>
                </>
              )}
            </div>
          </>
        )}
      </div>
    </Dialog>
  );
}
