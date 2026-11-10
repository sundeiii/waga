import { useEffect, useRef, useState } from "react";
import { KeyRound, ShieldAlert, ShieldQuestion } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { api, errorMessage } from "@/services/api";
import { terminalEngine } from "@/services/terminalEngine";
import { useSessions, type Prompt } from "@/stores/sessions";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { formatTimestamp } from "@/lib/utils";

/** Renders the oldest pending security prompt across all tabs. */
export function PromptDialogs() {
  const prompt = useSessions((s) => s.prompts[0]);
  useEffect(() => {
    if (prompt) terminalEngine.activate(prompt.tabId);
  }, [prompt]);
  if (!prompt) return null;
  return prompt.type === "hostKey" ? <HostKeyDialog key={prompt.sessionId} p={prompt} /> : <SecretDialog key={prompt.sessionId + prompt.attempt} p={prompt} />;
}

function HostKeyDialog({ p }: { p: Extract<Prompt, { type: "hostKey" }> }) {
  const clear = useSessions((s) => s.clearPrompts);
  const toast = useUi((s) => s.toast);
  const server = useServers((s) => s.servers.find((x) => x.id === useSessions.getState().tabs.find((t) => t.id === p.tabId)?.serverId));
  const [ack, setAck] = useState(false);
  const changed = p.status === "changed";
  const endpoint = `${p.host.includes(":") ? `[${p.host}]` : p.host}:${p.port}`;

  const answer = async (accept: boolean) => {
    try {
      await api.sshHostKeyDecision(p.sessionId, accept, changed && ack);
    } catch (e) {
      toast(errorMessage(e), "danger");
    }
    clear(p.tabId);
  };

  return (
    <Dialog
      open
      onOpenChange={() => undefined}
      dismissable={false}
      icon={
        <div className={changed ? "grid size-10 shrink-0 place-items-center rounded-xl bg-danger/15 text-danger" : "grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"}>
          {changed ? <ShieldAlert className="size-5" /> : <ShieldQuestion className="size-5" />}
        </div>
      }
      title={changed ? "WARNING: the host key has changed" : "Verify host key"}
      description={
        changed ? (
          <>
            The key presented by <b className="text-fg">{endpoint}</b> does not match the one you trusted before. This could mean someone is intercepting the connection (man-in-the-middle), or the server was legitimately reinstalled. <b className="text-danger">The connection is blocked.</b>
          </>
        ) : (
          <>
            First connection to <b className="text-fg">{server?.name ?? endpoint}</b> ({endpoint}). Compare this fingerprint with the server's real key (e.g. <code className="font-mono">ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub</code> on the server) before trusting it.
          </>
        )
      }
      className={changed ? "border-danger/40" : undefined}
      footer={
        <>
          <Button variant={changed ? "primary" : "ghost"} onClick={() => answer(false)} autoFocus={changed}>
            {changed ? "Keep blocked (recommended)" : "Cancel"}
          </Button>
          {changed ? (
            <Button variant="danger" disabled={!ack} onClick={() => answer(true)}>
              Replace trusted key & connect
            </Button>
          ) : (
            <Button variant="primary" onClick={() => answer(true)}>
              Trust & connect
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-3 pb-3">
        <div className="rounded-xl border border-line bg-black/30 p-3.5">
          <div className="text-[11px] uppercase tracking-wider text-faint">{changed ? "Presented key (new)" : "Fingerprint"}</div>
          <div className="selectable mt-1 break-all font-mono text-[13px] text-fg">{p.fingerprint}</div>
          <div className="mt-2 text-[12px] text-muted">
            Algorithm <span className="font-mono text-fg">{p.algorithm}</span>
          </div>
        </div>
        {changed && p.previous.length > 0 && (
          <div className="rounded-xl border border-line bg-black/20 p-3.5">
            <div className="text-[11px] uppercase tracking-wider text-faint">Previously trusted</div>
            {p.previous.map((k) => (
              <div key={k.fingerprint} className="mt-1.5">
                <div className="selectable break-all font-mono text-[12.5px] text-muted">{k.fingerprint}</div>
                <div className="text-[11.5px] text-faint">
                  {k.algorithm} · trusted {formatTimestamp(k.addedAt)}
                </div>
              </div>
            ))}
          </div>
        )}
        {changed && (
          <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-danger/30 bg-danger/[0.06] p-3 text-[12.5px] leading-relaxed">
            <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} className="mt-0.5 size-4 accent-[var(--color-danger)]" />
            <span>I have verified the new fingerprint with the server administrator (or on the server console) and understand the risk of replacing a trusted key.</span>
          </label>
        )}
      </div>
    </Dialog>
  );
}

function SecretDialog({ p }: { p: Extract<Prompt, { type: "secret" }> }) {
  const clear = useSessions((s) => s.clearPrompts);
  const [value, setValue] = useState("");
  const [remember, setRemember] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

  const send = async (secret: string | null) => {
    try {
      await api.sshProvideSecret(p.sessionId, secret, remember);
    } finally {
      setValue("");
      clear(p.tabId);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && void send(null)}
      icon={
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          <KeyRound className="size-5" />
        </div>
      }
      title={p.kind === "password" ? "Authentication required" : "Unlock private key"}
      description={p.prompt}
      footer={
        <>
          <Button variant="ghost" onClick={() => send(null)}>
            Cancel
          </Button>
          <Button variant="primary" disabled={!value} onClick={() => send(value)}>
            {p.kind === "password" ? "Sign in" : "Unlock"}
          </Button>
        </>
      }
    >
      <form
        className="space-y-3 pb-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (value) void send(value);
        }}
      >
        <Input ref={ref} autoFocus type="password" value={value} onChange={(e) => setValue(e.target.value)} placeholder={p.kind === "password" ? "Password" : "Passphrase"} autoComplete="off" aria-label={p.kind} />
        <label className="flex items-center gap-2 text-[12.5px] text-muted">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} className="size-4 accent-[var(--accent)]" />
          Remember in Windows Credential Manager (saved only if it works)
        </label>
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
