import { useMemo, useRef, useState, type DragEvent } from "react";
import { ArrowUp, ChevronRight, File, FileSymlink, Folder, FolderPlus, HardDrive, Loader2, Pencil, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Panel } from "@/components/ui/misc";
import { formatBytes } from "@/stores/monitoring";
import { cn } from "@/lib/utils";
import { displayName, localCrumbs, remoteCrumbs } from "@/lib/paths";
import type { FileEntry, Listing } from "@/types";

export const DND_TYPE = "application/x-yoru-files";
export interface DragPayload {
  side: "local" | "remote";
  paths: string[];
}

function fmtDate(secs: number | null) {
  if (secs == null) return "—";
  return new Date(secs * 1000).toLocaleString(undefined, { year: "numeric", month: "short", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

export function FilePane({
  side,
  title,
  listing,
  loading,
  error,
  showHidden,
  selected,
  onSelect,
  onNavigate,
  onRefresh,
  onMkdir,
  onRename,
  onDelete,
  onDropFiles,
  disabled,
}: {
  side: "local" | "remote";
  title: string;
  listing: Listing | null;
  loading: boolean;
  error: string | null;
  showHidden: boolean;
  selected: Set<string>;
  onSelect: (s: Set<string>) => void;
  onNavigate: (path: string) => void;
  onRefresh: () => void;
  onMkdir: () => void;
  onRename: (e: FileEntry) => void;
  onDelete: (e: FileEntry[]) => void;
  onDropFiles: (p: DragPayload) => void;
  disabled?: boolean;
}) {
  const [dragOver, setDragOver] = useState(false);
  const anchor = useRef<number | null>(null);
  const entries = useMemo(() => (listing?.entries ?? []).filter((e) => showHidden || !e.hidden), [listing, showHidden]);
  const crumbs = listing ? (side === "remote" ? remoteCrumbs(listing.path) : localCrumbs(listing.path)) : [];
  const selectedEntries = entries.filter((e) => selected.has(e.path));

  const click = (ev: React.MouseEvent, i: number, e: FileEntry) => {
    const next = new Set(ev.ctrlKey ? selected : []);
    if (ev.shiftKey && anchor.current != null) {
      const [a, b] = [anchor.current, i].sort((x, y) => x - y);
      for (let k = a; k <= b; k++) next.add(entries[k].path);
    } else if (ev.ctrlKey && selected.has(e.path)) {
      next.delete(e.path);
    } else {
      next.add(e.path);
      anchor.current = i;
    }
    onSelect(next);
  };

  const dragStart = (ev: DragEvent, e: FileEntry) => {
    const paths = selected.has(e.path) ? selectedEntries.map((x) => x.path) : [e.path];
    ev.dataTransfer.setData(DND_TYPE, JSON.stringify({ side, paths } satisfies DragPayload));
    ev.dataTransfer.effectAllowed = "copy";
  };

  const drop = (ev: DragEvent) => {
    ev.preventDefault();
    setDragOver(false);
    const raw = ev.dataTransfer.getData(DND_TYPE);
    if (!raw) return;
    const p = JSON.parse(raw) as DragPayload;
    if (p.side !== side) onDropFiles(p);
  };

  return (
    <Panel
      className={cn("flex min-h-0 min-w-0 flex-col overflow-hidden transition-[box-shadow,border-color]", dragOver && "border-accent/60 shadow-[0_0_0_1px_var(--accent),0_0_40px_-12px_var(--accent)]")}
      onDragOver={(ev) => {
        if (ev.dataTransfer.types.includes(DND_TYPE) && !disabled) {
          ev.preventDefault();
          ev.dataTransfer.dropEffect = "copy";
          setDragOver(true);
        }
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={drop}
    >
      {/* header */}
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-faint">{title}</span>
        <div className="ml-auto flex items-center gap-0.5">
          <Button variant="ghost" size="icon" className="size-7" disabled={!listing?.parent && listing?.parent !== ""} onClick={() => listing?.parent != null && onNavigate(listing.parent)} aria-label="Up one level" title="Up (Backspace)">
            <ArrowUp />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" onClick={onRefresh} aria-label="Refresh" title="Refresh">
            <RefreshCw className={cn(loading && "animate-spin")} />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" disabled={disabled || !listing?.path} onClick={onMkdir} aria-label="New folder" title="New folder">
            <FolderPlus />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" disabled={disabled || selectedEntries.length !== 1} onClick={() => onRename(selectedEntries[0])} aria-label="Rename" title="Rename (F2)">
            <Pencil />
          </Button>
          <Button variant="ghost" size="icon" className="size-7 hover:text-danger" disabled={disabled || selectedEntries.length === 0 || !listing?.path} onClick={() => onDelete(selectedEntries)} aria-label="Delete" title="Delete (Del)">
            <Trash2 />
          </Button>
        </div>
      </div>
      {/* breadcrumbs */}
      <nav className="flex min-h-9 items-center gap-0.5 overflow-x-auto border-b border-line px-2 text-[12.5px]" aria-label={`${title} path`}>
        {crumbs.map((c, i) => (
          <span key={c.path + i} className="flex shrink-0 items-center">
            {i > 0 && <ChevronRight className="size-3.5 text-faint" />}
            <button onClick={() => onNavigate(c.path)} className={cn("rounded-md px-1.5 py-0.5 font-mono hover:bg-ink/[0.06]", i === crumbs.length - 1 ? "text-fg" : "text-muted")}>
              {displayName(c.label)}
            </button>
          </span>
        ))}
      </nav>
      {/* table */}
      <div
        className="min-h-0 flex-1 overflow-y-auto outline-none"
        tabIndex={0}
        onKeyDown={(ev) => {
          if (ev.key === "Backspace" && listing?.parent != null) onNavigate(listing.parent);
          if (ev.key === "Delete" && selectedEntries.length && !disabled && listing?.path) onDelete(selectedEntries);
          if (ev.key === "F2" && selectedEntries.length === 1 && !disabled) onRename(selectedEntries[0]);
          if (ev.key === "Enter" && selectedEntries.length === 1 && selectedEntries[0].isDir) onNavigate(selectedEntries[0].path);
          if (ev.key === "a" && ev.ctrlKey) {
            ev.preventDefault();
            onSelect(new Set(entries.map((e) => e.path)));
          }
        }}
      >
        {error ? (
          <p className="p-4 text-[12.5px] text-danger">{error}</p>
        ) : !listing && loading ? (
          <div className="grid h-full place-items-center text-faint">
            <Loader2 className="size-5 animate-spin" />
          </div>
        ) : entries.length === 0 ? (
          <p className="p-4 text-[12.5px] text-faint">{listing ? "This folder is empty" : ""}</p>
        ) : (
          <table className="w-full table-fixed text-[12.5px]">
            <thead className="sticky top-0 z-[1] bg-surface/95 text-left text-[10.5px] uppercase tracking-wider text-faint backdrop-blur">
              <tr>
                <th className="px-3 py-1.5 font-medium">Name</th>
                <th className="w-24 px-2 py-1.5 text-right font-medium">Size</th>
                <th className="w-40 px-3 py-1.5 font-medium">Modified</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e, i) => {
                const Icon = !listing?.path ? HardDrive : e.kind === "symlink" ? FileSymlink : e.isDir ? Folder : File;
                const sel = selected.has(e.path);
                return (
                  <tr
                    key={e.path}
                    draggable={!disabled && !!listing?.path}
                    onDragStart={(ev) => dragStart(ev, e)}
                    onClick={(ev) => click(ev, i, e)}
                    onDoubleClick={() => e.isDir && onNavigate(e.path)}
                    aria-selected={sel}
                    className={cn("cursor-default select-none", sel ? "bg-accent-soft text-fg" : "hover:bg-ink/[0.035]", e.hidden && !sel && "text-muted")}
                    title={e.permissions ? `${e.permissions}` : undefined}
                  >
                    <td className="truncate px-3 py-1.5">
                      <span className="flex min-w-0 items-center gap-2">
                        <Icon className={cn("size-4 shrink-0", e.isDir ? "text-accent" : "text-faint")} />
                        <span className="truncate">{displayName(e.name)}</span>
                      </span>
                    </td>
                    <td className="px-2 py-1.5 text-right font-mono text-[11.5px] tabular-nums text-muted">{e.size != null ? formatBytes(e.size) : ""}</td>
                    <td className="truncate px-3 py-1.5 text-[11.5px] text-faint">{fmtDate(e.modified)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      <div className="flex items-center gap-2 border-t border-line px-3 py-1.5 text-[11px] text-faint">
        <span>
          {entries.length} items{selectedEntries.length ? ` · ${selectedEntries.length} selected` : ""}
        </span>
        {listing?.truncated && <span className="text-warning">Showing first {listing.entries.length} of {listing.total}</span>}
      </div>
    </Panel>
  );
}
