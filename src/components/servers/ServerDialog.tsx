import { useEffect, useMemo, useState } from "react";
import { open as openFile } from "@tauri-apps/plugin-dialog";
import { FileKey, KeyRound, Lock, ShieldCheck } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Field, Input, Select, Textarea } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { useUi } from "@/stores/ui";
import { useServers } from "@/stores/servers";
import { useSettings } from "@/stores/settings";
import { errorMessage } from "@/services/api";
import { terminalEngine } from "@/services/terminalEngine";
import { parseTags, validateHost, validatePort, validateUsername } from "@/lib/validation";
import { cn } from "@/lib/utils";
import type { AuthMethod, ProfileKind, ServerInput } from "@/types";

export const PROFILE_KINDS: { value: ProfileKind; label: string }[] = [
  { value: "linux", label: "Linux server" },
  { value: "debian", label: "Debian / Ubuntu" },
  { value: "windows", label: "Windows (OpenSSH)" },
  { value: "localVm", label: "Local virtual machine" },
  { value: "proxmox", label: "Proxmox host" },
  { value: "custom", label: "Custom SSH environment" },
];

const AUTH: { value: AuthMethod; label: string; icon: typeof Lock }[] = [
  { value: "password", label: "Password", icon: Lock },
  { value: "key", label: "Private key", icon: FileKey },
  { value: "agent", label: "SSH agent", icon: KeyRound },
];

function blank(defaultPort: number, defaultUser: string): ServerInput {
  return {
    name: "",
    host: "",
    port: defaultPort,
    username: defaultUser,
    authMethod: "password",
    keyPath: null,
    profileKind: "linux",
    groupName: null,
    tags: [],
    notes: "",
    favorite: false,
    autoReconnect: false,
    keepalive: true,
  };
}

export function ServerDialog() {
  const { open, editing, prefill, connectAfterSave } = useUi((s) => s.serverDialog);
  const close = useUi((s) => s.closeServerDialog);
  const toast = useUi((s) => s.toast);
  const setPage = useUi((s) => s.setPage);
  const { create, update } = useServers();
  const conn = useSettings((s) => s.settings?.connections);

  const [form, setForm] = useState<ServerInput>(() => blank(22, ""));
  const [tagsText, setTagsText] = useState("");
  const [password, setPassword] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [clearPassword, setClearPassword] = useState(false);
  const [clearPassphrase, setClearPassphrase] = useState(false);
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [saving, setSaving] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const base = editing ? { ...editing } : { ...blank(conn?.defaultPort ?? 22, conn?.defaultUsername ?? ""), ...prefill };
    setForm(base);
    setTagsText(base.tags.join(", "));
    setPassword("");
    setPassphrase("");
    setClearPassword(false);
    setClearPassphrase(false);
    setTouched({});
    setServerError(null);
  }, [open, editing, prefill, conn?.defaultPort, conn?.defaultUsername]);

  const set = <K extends keyof ServerInput>(k: K, v: ServerInput[K]) => setForm((f) => ({ ...f, [k]: v }));

  const errors = useMemo(
    () => ({
      name: form.name.trim() ? null : "Display name is required",
      host: validateHost(form.host),
      port: validatePort(form.port),
      username: validateUsername(form.username),
      keyPath: form.authMethod === "key" && !form.keyPath?.trim() ? "Choose a private key file" : null,
    }),
    [form],
  );
  const valid = Object.values(errors).every((e) => !e);
  const err = (k: keyof typeof errors) => (touched[k] || touched.__submit ? errors[k] : null);

  const pickKey = async () => {
    const selected = await openFile({ multiple: false, directory: false, title: "Choose private key" });
    if (typeof selected === "string") set("keyPath", selected);
  };

  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    setTouched((t) => ({ ...t, __submit: true }));
    if (!valid) return;
    setSaving(true);
    setServerError(null);
    const input: ServerInput = { ...form, tags: parseTags(tagsText), port: Number(form.port) };
    const secrets = {
      password: form.authMethod === "password" && password ? password : undefined,
      passphrase: form.authMethod === "key" && passphrase ? passphrase : undefined,
      clearPassword,
      clearPassphrase,
    };
    try {
      const saved = editing ? await update(editing.id, input, secrets) : await create(input, secrets);
      setPassword("");
      setPassphrase("");
      close();
      toast(editing ? `Saved ${saved.name}` : `Added ${saved.name}`, "success");
      if (connectAfterSave) {
        setPage("terminal");
        void terminalEngine.openTab(saved);
      }
    } catch (ex) {
      setServerError(errorMessage(ex));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && close()}
      title={editing ? `Edit ${editing.name}` : "New server"}
      description="Profiles are stored locally. Passwords and passphrases go to Windows Credential Manager, never to the database."
      className="w-[min(680px,calc(100vw-48px))]"
      footer={
        <>
          {serverError && <p className="mr-auto max-w-[60%] text-[12.5px] text-danger">{serverError}</p>}
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => void submit()} disabled={saving}>
            {saving ? "Saving…" : connectAfterSave ? "Save & connect" : "Save server"}
          </Button>
        </>
      }
    >
      <form onSubmit={submit} className="grid grid-cols-6 gap-x-4 gap-y-4 pb-4" noValidate>
        <Field label="Display name" error={err("name")} className="col-span-3" htmlFor="sv-name">
          <Input id="sv-name" autoFocus value={form.name} placeholder="web-01" onChange={(e) => set("name", e.target.value)} onBlur={() => setTouched((t) => ({ ...t, name: true }))} aria-invalid={!!err("name")} />
        </Field>
        <Field label="Profile" className="col-span-3" htmlFor="sv-kind">
          <Select id="sv-kind" value={form.profileKind} onChange={(e) => set("profileKind", e.target.value as ProfileKind)}>
            {PROFILE_KINDS.map((k) => (
              <option key={k.value} value={k.value}>
                {k.label}
              </option>
            ))}
          </Select>
        </Field>

        <Field label="Hostname / IP" error={err("host")} hint="IPv4, IPv6 or DNS name" className="col-span-4" htmlFor="sv-host">
          <Input id="sv-host" value={form.host} placeholder="10.0.0.12 or host.example.com" onChange={(e) => set("host", e.target.value)} onBlur={() => setTouched((t) => ({ ...t, host: true }))} aria-invalid={!!err("host")} className="font-mono text-[13px]" />
        </Field>
        <Field label="Port" error={err("port")} className="col-span-2" htmlFor="sv-port">
          <Input id="sv-port" type="number" min={1} max={65535} value={form.port} onChange={(e) => set("port", Number(e.target.value))} onBlur={() => setTouched((t) => ({ ...t, port: true }))} aria-invalid={!!err("port")} className="font-mono" />
        </Field>

        <Field label="Username" error={err("username")} className="col-span-6" htmlFor="sv-user">
          <Input id="sv-user" value={form.username} placeholder="root" onChange={(e) => set("username", e.target.value)} onBlur={() => setTouched((t) => ({ ...t, username: true }))} aria-invalid={!!err("username")} className="font-mono text-[13px]" />
        </Field>

        <div className="col-span-6">
          <div className="mb-1.5 text-[12px] font-medium tracking-wide text-muted">Authentication</div>
          <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label="Authentication method">
            {AUTH.map((a) => (
              <button
                type="button"
                key={a.value}
                role="radio"
                aria-checked={form.authMethod === a.value}
                onClick={() => set("authMethod", a.value)}
                className={cn(
                  "flex h-10 items-center justify-center gap-2 rounded-xl border text-[13px] transition-colors",
                  form.authMethod === a.value ? "border-accent/50 bg-accent-soft text-fg" : "border-line text-muted hover:border-line-strong hover:text-fg",
                )}
              >
                <a.icon className="size-4" /> {a.label}
              </button>
            ))}
          </div>
        </div>

        {form.authMethod === "password" && (
          <Field
            label="Password"
            className="col-span-6"
            htmlFor="sv-pw"
            hint={
              editing?.hasPassword && !clearPassword ? (
                <span>
                  A password is saved. Leave empty to keep it ·{" "}
                  <button type="button" className="text-accent hover:underline" onClick={() => setClearPassword(true)}>
                    forget it
                  </button>
                </span>
              ) : (
                "Optional. Leave empty to be asked on each connection."
              )
            }
          >
            <Input id="sv-pw" type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder={editing?.hasPassword && !clearPassword ? "••••••••  (saved)" : ""} autoComplete="new-password" />
          </Field>
        )}

        {form.authMethod === "key" && (
          <>
            <Field label="Private key file" error={err("keyPath")} className="col-span-6" htmlFor="sv-key" hint="OpenSSH, PEM or PKCS#8. Encrypted keys are supported.">
              <div className="flex gap-2">
                <Input id="sv-key" value={form.keyPath ?? ""} onChange={(e) => set("keyPath", e.target.value)} placeholder="C:\Users\you\.ssh\id_ed25519" className="font-mono text-[12.5px]" aria-invalid={!!err("keyPath")} />
                <Button onClick={pickKey}>Browse…</Button>
              </div>
            </Field>
            <Field
              label="Key passphrase"
              className="col-span-6"
              htmlFor="sv-pp"
              hint={
                editing?.hasPassphrase && !clearPassphrase ? (
                  <span>
                    A passphrase is saved ·{" "}
                    <button type="button" className="text-accent hover:underline" onClick={() => setClearPassphrase(true)}>
                      forget it
                    </button>
                  </span>
                ) : (
                  "Optional. If the key is encrypted and none is saved, you'll be asked."
                )
              }
            >
              <Input id="sv-pp" type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} autoComplete="new-password" />
            </Field>
          </>
        )}

        {form.authMethod === "agent" && (
          <p className="col-span-6 -mt-1 rounded-xl border border-line bg-black/20 px-3 py-2.5 text-[12.5px] leading-relaxed text-muted">
            Uses keys from the Windows <span className="text-fg">OpenSSH Authentication Agent</span> service, or Pageant if it's running. Add keys with <code className="font-mono text-fg">ssh-add</code>.
          </p>
        )}

        <Field label="Group" className="col-span-3" htmlFor="sv-group">
          <Input id="sv-group" value={form.groupName ?? ""} placeholder="production" onChange={(e) => set("groupName", e.target.value || null)} />
        </Field>
        <Field label="Tags" hint="Comma separated" className="col-span-3" htmlFor="sv-tags">
          <Input id="sv-tags" value={tagsText} placeholder="web, nginx" onChange={(e) => setTagsText(e.target.value)} />
        </Field>

        <Field label="Jump host" className="col-span-6" hint="ProxyJump support is planned; not implemented yet.">
          <Input disabled placeholder="Not yet implemented" />
        </Field>

        <Field label="Notes" className="col-span-6" htmlFor="sv-notes">
          <Textarea id="sv-notes" value={form.notes} onChange={(e) => set("notes", e.target.value)} rows={2} />
        </Field>

        <div className="col-span-6 grid grid-cols-2 gap-3">
          <label className="flex items-center justify-between rounded-xl border border-line bg-black/15 px-3 py-2.5 text-[13px]">
            <span>
              Keepalive
              <span className="block text-[11.5px] text-faint">Detect dead connections</span>
            </span>
            <Switch checked={form.keepalive} onCheckedChange={(v) => set("keepalive", v)} label="Keepalive" />
          </label>
          <label className="flex items-center justify-between rounded-xl border border-line bg-black/15 px-3 py-2.5 text-[13px]">
            <span>
              Auto-reconnect
              <span className="block text-[11.5px] text-faint">After an unexpected drop</span>
            </span>
            <Switch checked={form.autoReconnect} onCheckedChange={(v) => set("autoReconnect", v)} label="Auto-reconnect" />
          </label>
        </div>

        <p className="col-span-6 flex items-center gap-2 text-[12px] text-faint">
          <ShieldCheck className="size-3.5 text-success" /> Host keys are verified on first connect; changed keys are blocked.
        </p>
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
