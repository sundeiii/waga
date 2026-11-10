import { ArrowDown, ArrowUp, Folder, X } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Panel } from "@/components/ui/misc";
import { Meter } from "@/components/ui/Meter";
import { api } from "@/services/api";
import { formatBytes } from "@/stores/monitoring";
import { useTransfers } from "@/stores/transfers";
import { cn } from "@/lib/utils";
import { displayName } from "@/lib/paths";

export function TransfersPanel() {
  const { byId, order, clearFinished } = useTransfers();
  const items = order.map((id) => byId[id]).filter(Boolean);
  if (!items.length) return null;
  const running = items.filter((t) => t.state === "running").length;

  return (
    <Panel className="flex max-h-48 shrink-0 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-faint">Transfers</span>
        <span className="text-[11.5px] text-muted">{running ? `${running} running` : "idle"}</span>
        <Button variant="ghost" size="sm" className="ml-auto h-6 text-[11.5px]" onClick={clearFinished} disabled={items.length === running}>
          Clear finished
        </Button>
      </div>
      <div className="overflow-y-auto">
        {items.map((t) => {
          const pct = t.total ? (t.bytes / t.total) * 100 : null;
          const secs = Math.max(0.001, (t.updatedAt - t.startedAt) / 1000);
          const rate = t.state === "running" && secs > 0.3 ? t.bytes / secs : null;
          return (
            <div key={t.id} className="grid grid-cols-[18px_1fr_190px_28px] items-center gap-3 border-b border-line px-3 py-2 text-[12px] last:border-0">
              {t.direction === "upload" ? <ArrowUp className="size-4 text-accent" aria-label="Upload" /> : <ArrowDown className="size-4 text-accent" aria-label="Download" />}
              <div className="min-w-0">
                <div className="flex items-center gap-1.5 truncate">
                  {t.isDir && <Folder className="size-3.5 shrink-0 text-accent" aria-label="Folder" />}
                  <span className="truncate">{displayName(t.name)}</span>
                  {t.isDir && (
                    <span className="shrink-0 text-[11px] tabular-nums text-faint">
                      {t.filesDone}/{t.filesTotal} files{t.skipped ? ` · ${t.skipped} links skipped` : ""}
                    </span>
                  )}
                </div>
                <div className="truncate font-mono text-[10.5px] text-faint" title={t.dest}>
                  → {t.dest}
                </div>
              </div>
              <div>
                {t.state === "running" ? (
                  <>
                    <Meter percent={pct} />
                    <div className="mt-1 flex justify-between font-mono text-[10.5px] tabular-nums text-muted">
                      <span>
                        {formatBytes(t.bytes)}
                        {t.total != null && ` / ${formatBytes(t.total)}`}
                      </span>
                      <span>{rate ? `${formatBytes(rate)}/s` : ""}</span>
                    </div>
                  </>
                ) : (
                  <span className={cn("text-[11.5px]", t.state === "done" ? "text-success" : t.state === "cancelled" ? "text-muted" : "text-danger")} title={t.error ?? undefined}>
                    {t.state === "done" ? `Done · ${formatBytes(t.bytes)}` : t.state === "cancelled" ? "Cancelled" : `Failed: ${t.error ?? "unknown error"}`}
                  </span>
                )}
              </div>
              {t.state === "running" ? (
                <Button variant="ghost" size="icon" className="size-7" onClick={() => void api.sftpCancel(t.id)} aria-label={`Cancel ${t.name}`} title="Cancel">
                  <X />
                </Button>
              ) : (
                <span />
              )}
            </div>
          );
        })}
      </div>
    </Panel>
  );
}
