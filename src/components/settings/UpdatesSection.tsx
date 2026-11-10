import { ArrowUpCircle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import { Meter } from "@/components/ui/Meter";
import { useUpdates } from "@/stores/updates";
import { useSettings } from "@/stores/settings";
import { useSessions } from "@/stores/sessions";
import { formatBytes } from "@/stores/monitoring";
import { relativeTime } from "@/lib/utils";

export function UpdatesSection() {
  const { current, available, checking, installing, progress, error, lastChecked, check, install } = useUpdates();
  const checkUpdates = useSettings((s) => s.settings?.system.checkUpdates ?? true);
  const update = useSettings((s) => s.update);
  const live = useSessions((s) => s.tabs.filter((t) => t.state === "connected").length);
  const pct = progress?.total ? (progress.downloaded / progress.total) * 100 : null;

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-[13px]">
            yoru <span className="font-mono">{current ?? "…"}</span>
          </div>
          <div className="text-[12px] text-faint">{lastChecked ? `Last checked ${relativeTime(new Date(lastChecked).toISOString())}` : "Not checked yet"}</div>
        </div>
        <Button size="sm" variant="secondary" disabled={checking || installing} onClick={() => void check()}>
          <RefreshCw className={checking ? "animate-spin" : undefined} /> {checking ? "Checking…" : "Check for updates"}
        </Button>
      </div>

      <label className="flex items-center justify-between text-[13px]">
        <span>
          Check automatically
          <span className="block text-[12px] text-faint">Once a day at startup, from github.com/sundeiii/waga</span>
        </span>
        <Switch checked={checkUpdates} onCheckedChange={(v) => update((s) => ({ ...s, system: { ...s.system, checkUpdates: v } }))} label="Check for updates automatically" />
      </label>

      {available && (
        <div className="rounded-xl border border-accent/30 bg-accent-soft p-3.5">
          <div className="flex items-center gap-2 text-[13.5px] font-medium">
            <ArrowUpCircle className="size-4 text-accent" /> yoru {available.version} is available
            <span className="font-normal text-faint">(you have {available.currentVersion})</span>
          </div>
          {available.notes && <pre className="selectable mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-sans text-[12.5px] leading-relaxed text-muted">{available.notes}</pre>}
          {installing ? (
            <div className="mt-3">
              <Meter percent={pct} label="Download progress" />
              <div className="mt-1 text-[11.5px] text-faint">
                {progress?.total ? `${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)}` : "Downloading…"} · verifying signature, then installing and restarting
              </div>
            </div>
          ) : (
            <div className="mt-3 flex items-center justify-between gap-3">
              <span className="text-[12px] text-faint">{live ? `${live} open session${live > 1 ? "s" : ""} will be closed by the restart.` : "Your servers, keys and settings are kept."}</span>
              <Button variant="primary" size="sm" onClick={() => void install()}>
                Install &amp; restart
              </Button>
            </div>
          )}
        </div>
      )}
      {error && <p className="text-[12px] text-danger">{error}</p>}
      <p className="text-[11.5px] text-faint">Updates are verified against yoru's signing key before installing; an unsigned or tampered installer is refused.</p>
    </div>
  );
}
