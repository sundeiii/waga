import { useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Cloud, Download, FolderOpen, RefreshCw, Upload } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { api, errorMessage } from "@/services/api";
import { useSettings } from "@/stores/settings";
import { refreshAfterMerge, useSync } from "@/stores/sync";
import { useUi } from "@/stores/ui";
import { relativeTime } from "@/lib/utils";

type PassDialog = { mode: "export"; path: string } | { mode: "import"; path: string } | { mode: "sync" };

const MIN = 10;

export function BackupSyncSection() {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const { hasPassphrase, syncing, last, error, loadStatus, run } = useSync();
  const toast = useUi((s) => s.toast);
  const [dlg, setDlg] = useState<PassDialog | null>(null);
  const [pass, setPass] = useState("");
  const [pass2, setPass2] = useState("");
  const [withSecrets, setWithSecrets] = useState(false);
  const [busy, setBusy] = useState(false);
  const [dlgError, setDlgError] = useState<string | null>(null);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  if (!settings) return null;
  const sync = settings.sync;
  const setSync = (patch: Partial<typeof sync>) => update((s) => ({ ...s, sync: { ...s.sync, ...patch } }));

  const openPass = (d: PassDialog) => {
    setDlg(d);
    setPass("");
    setPass2("");
    setWithSecrets(false);
    setDlgError(null);
  };

  const needsConfirm = dlg?.mode === "export" || dlg?.mode === "sync";
  const passOk = pass.length >= MIN && (!needsConfirm || pass === pass2);

  const submit = async () => {
    if (!dlg || !passOk) return;
    setBusy(true);
    setDlgError(null);
    try {
      if (dlg.mode === "export") {
        await api.exportBackup(dlg.path, pass, withSecrets);
        toast("Encrypted backup saved", "success");
      } else if (dlg.mode === "import") {
        const st = await api.importBackup(dlg.path, pass);
        await refreshAfterMerge(st);
        toast(`Restored: ${st.added} added, ${st.updated} updated${st.secrets ? `, ${st.secrets} credentials` : ""}`, "success");
      } else {
        await api.setSyncPassphrase(pass);
        await loadStatus();
        toast("Sync passphrase saved on this PC", "success");
      }
      setDlg(null);
    } catch (e) {
      setDlgError(errorMessage(e));
    } finally {
      setPass("");
      setPass2("");
      setBusy(false);
    }
  };

  const pickFolder = async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: "Choose a folder you already sync (OneDrive, Dropbox, NAS…)" });
    if (typeof dir === "string") setSync({ folder: dir });
  };

  const exportBackup = async () => {
    const path = await saveDialog({ title: "Save encrypted backup", defaultPath: `yoru-${new Date().toISOString().slice(0, 10)}.yorubackup`, filters: [{ name: "yoru backup", extensions: ["yorubackup"] }] });
    if (path) openPass({ mode: "export", path });
  };

  const importBackup = async () => {
    const path = await openDialog({ multiple: false, title: "Restore a yoru backup", filters: [{ name: "yoru backup", extensions: ["yorubackup"] }] });
    if (typeof path === "string") openPass({ mode: "import", path });
  };

  const syncNow = async () => {
    const r = await run();
    if (r && !r.errors.length) toast(`Synced with ${r.devices.length - 1} other PC${r.devices.length === 2 ? "" : "s"}`, "success");
  };

  return (
    <div className="space-y-4">
      {/* ---- sync ---- */}
      <div className="space-y-3">
        <label className="flex items-center justify-between text-[13px]">
          <span>
            <span className="flex items-center gap-1.5">
              <Cloud className="size-4 text-accent" /> Sync between PCs
            </span>
            <span className="block text-[12px] text-faint">Servers, snippets, forwards, CAs and trusted host keys. End-to-end encrypted.</span>
          </span>
          <Switch checked={sync.enabled} onCheckedChange={(v) => setSync({ enabled: v })} label="Sync between PCs" />
        </label>

        {sync.enabled && (
          <div className="space-y-3 rounded-xl border border-line bg-shade/15 p-3">
            <div className="flex items-center gap-2">
              <Input readOnly value={sync.folder} placeholder="No folder chosen" className="flex-1 font-mono text-[12px]" aria-label="Sync folder" />
              <Button size="sm" onClick={() => void pickFolder()}>
                <FolderOpen /> Choose…
              </Button>
            </div>
            <p className="text-[11.5px] text-faint">Pick a folder that OneDrive/Dropbox/your NAS already keeps in sync. yoru writes one encrypted file per PC into a “yoru-sync” subfolder. Use the same passphrase on every PC.</p>

            <div className="flex items-center justify-between text-[13px]">
              <span>
                Passphrase
                <span className="block text-[12px] text-faint">{hasPassphrase ? "Saved in Windows Credential Manager on this PC" : "Not set on this PC"}</span>
              </span>
              <div className="flex gap-2">
                {hasPassphrase && (
                  <Button size="sm" variant="ghost" onClick={async () => { await api.setSyncPassphrase(null); await loadStatus(); }}>
                    Forget
                  </Button>
                )}
                <Button size="sm" variant={hasPassphrase ? "secondary" : "primary"} onClick={() => openPass({ mode: "sync" })}>
                  {hasPassphrase ? "Change" : "Set passphrase"}
                </Button>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <label className="text-[12px] text-muted">
                This PC's name
                <Input value={sync.deviceName} onChange={(e) => setSync({ deviceName: e.target.value })} className="mt-1" />
              </label>
              <label className="text-[12px] text-muted">
                Every (minutes)
                <Input type="number" min={1} max={120} value={sync.intervalMins} onChange={(e) => setSync({ intervalMins: Math.min(120, Math.max(1, Number(e.target.value) || 5)) })} className="mt-1 font-mono" />
              </label>
            </div>

            <label className="flex items-center justify-between text-[13px]">
              <span>
                Include saved passwords
                <span className="block text-[12px] text-faint">Encrypted with your passphrase; stored in Credential Manager on each PC.</span>
              </span>
              <Switch checked={sync.includeSecrets} onCheckedChange={(v) => setSync({ includeSecrets: v })} label="Sync saved passwords" />
            </label>

            <div className="flex items-center justify-between border-t border-line pt-3">
              <div className="min-w-0 text-[12px] text-faint">
                {last ? (
                  <>
                    Last sync {relativeTime(last.at)} · {last.devices.length} PC{last.devices.length === 1 ? "" : "s"}: {last.devices.map((d) => d.deviceName + (d.isSelf ? " (this)" : "")).join(", ")}
                  </>
                ) : (
                  "Not synced yet"
                )}
              </div>
              <Button size="sm" variant="primary" disabled={syncing || !sync.folder || !hasPassphrase} onClick={() => void syncNow()}>
                <RefreshCw className={syncing ? "animate-spin" : undefined} /> {syncing ? "Syncing…" : "Sync now"}
              </Button>
            </div>
            {error && <p className="whitespace-pre-wrap text-[12px] text-danger">{error}</p>}
          </div>
        )}
      </div>

      {/* ---- backup ---- */}
      <div className="flex items-center justify-between border-t border-line pt-3 text-[13px]">
        <span>
          Encrypted backup
          <span className="block text-[12px] text-faint">One file you can keep anywhere. Restoring merges; nothing on this PC is removed.</span>
        </span>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => void importBackup()}>
            <Upload /> Restore…
          </Button>
          <Button size="sm" variant="secondary" onClick={() => void exportBackup()}>
            <Download /> Back up…
          </Button>
        </div>
      </div>

      <Dialog
        open={!!dlg}
        onOpenChange={(o) => !o && !busy && setDlg(null)}
        title={dlg?.mode === "export" ? "Encrypt backup" : dlg?.mode === "import" ? "Unlock backup" : "Sync passphrase"}
        description={
          dlg?.mode === "export"
            ? "Choose a passphrase. Without it the backup cannot be opened by anyone — including you."
            : dlg?.mode === "import"
              ? "Enter the passphrase this backup was made with."
              : "Use the same passphrase on every PC you sync. It never leaves this PC unencrypted."
        }
        footer={
          <>
            {dlgError && <p className="mr-auto max-w-[60%] text-[12px] text-danger">{dlgError}</p>}
            <Button variant="ghost" disabled={busy} onClick={() => setDlg(null)}>
              Cancel
            </Button>
            <Button variant="primary" disabled={!passOk || busy} onClick={() => void submit()}>
              {busy ? "Working…" : dlg?.mode === "import" ? "Restore" : "Save"}
            </Button>
          </>
        }
      >
        <form
          className="space-y-3 pb-3"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <Input autoFocus type="password" value={pass} onChange={(e) => setPass(e.target.value)} placeholder={`Passphrase (at least ${MIN} characters)`} aria-label="Passphrase" />
          {needsConfirm && <Input type="password" value={pass2} onChange={(e) => setPass2(e.target.value)} placeholder="Repeat passphrase" aria-label="Repeat passphrase" aria-invalid={!!pass2 && pass !== pass2} />}
          {dlg?.mode === "export" && (
            <label className="flex items-center gap-2 text-[12.5px] text-muted">
              <input type="checkbox" checked={withSecrets} onChange={(e) => setWithSecrets(e.target.checked)} className="size-4 accent-[var(--accent)]" />
              Include saved passwords and key passphrases
            </label>
          )}
          <button type="submit" hidden />
        </form>
      </Dialog>
    </div>
  );
}
