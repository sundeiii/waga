import { useEffect, useMemo, useState } from "react";
import * as DM from "@radix-ui/react-dropdown-menu";
import { Copy, CornerDownLeft, MoreHorizontal, Pencil, Play, Plus, Search, SquareCode, Star, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge, EmptyState, Panel } from "@/components/ui/misc";
import { Select } from "@/components/ui/Input";
import { CATEGORY_LABEL, insertSnippet, matchesSnippet, runSnippet, useSnippets } from "@/stores/snippets";
import { useSessions } from "@/stores/sessions";
import { useUi } from "@/stores/ui";
import { cn, relativeTime } from "@/lib/utils";
import { SNIPPET_CATEGORIES, type Snippet, type SnippetCategory } from "@/types";

const menuItem =
  "flex cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-muted outline-none data-[highlighted]:bg-white/[0.06] data-[highlighted]:text-fg [&_svg]:size-4";

export function SnippetsPage() {
  const { snippets, loaded, load, openEditor, remove, toggleFavorite } = useSnippets();
  const tabs = useSessions((s) => s.tabs);
  const activeTabId = useSessions((s) => s.activeTabId);
  const askConfirm = useUi((s) => s.askConfirm);
  const toast = useUi((s) => s.toast);
  const setPage = useUi((s) => s.setPage);
  const [q, setQ] = useState("");
  const [cat, setCat] = useState<SnippetCategory | "all" | "favorites">("all");

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const live = tabs.filter((t) => t.state === "connected");
  const [target, setTarget] = useState<string | null>(null);
  const targetId = target && live.some((t) => t.id === target) ? target : (live.find((t) => t.id === activeTabId)?.id ?? live[0]?.id ?? null);

  const counts = useMemo(() => {
    const m: Record<string, number> = { all: snippets.length, favorites: snippets.filter((s) => s.favorite).length };
    for (const s of snippets) m[s.category] = (m[s.category] ?? 0) + 1;
    return m;
  }, [snippets]);

  const filtered = snippets.filter((s) => (cat === "all" || (cat === "favorites" ? s.favorite : s.category === cat)) && matchesSnippet(s, q));

  const doInsert = async (s: Snippet) => {
    if (!targetId) return;
    if (await insertSnippet(s, targetId)) {
      setPage("terminal");
      toast(`Inserted “${s.title}”. Review it, then press Enter to run.`, "info");
    }
  };
  const doRun = async (s: Snippet) => {
    if (!targetId) return;
    if (await runSnippet(s, targetId)) setPage("terminal");
  };
  const copy = async (s: Snippet) => {
    try {
      await navigator.clipboard.writeText(s.command);
      toast("Command copied", "success");
    } catch {
      toast("Clipboard is unavailable", "warning");
    }
  };

  const side = (id: typeof cat, label: string) => (
    <button
      key={id}
      onClick={() => setCat(id)}
      className={cn("flex h-8 w-full items-center justify-between rounded-lg px-2.5 text-[13px] transition-colors", cat === id ? "bg-white/[0.07] text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg")}
    >
      <span className="truncate">{label}</span>
      <span className="text-[11px] tabular-nums text-faint">{counts[id] ?? 0}</span>
    </button>
  );

  return (
    <div className="grid h-full grid-cols-[220px_1fr] gap-4 pb-2">
      <Panel className="flex flex-col gap-0.5 self-start p-2">
        {side("all", "All snippets")}
        {side("favorites", "Favorites")}
        <div className="my-1.5 h-px bg-line" />
        {SNIPPET_CATEGORIES.map((c) => side(c, CATEGORY_LABEL[c]))}
      </Panel>

      <div className="flex min-w-0 flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative w-72">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-faint" />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search title, command, tags"
              aria-label="Search snippets"
              className="h-8 w-full rounded-[10px] border border-line bg-black/25 pl-8 pr-2 text-[12.5px] outline-none transition-[border-color,box-shadow] placeholder:text-faint focus:border-accent/60 focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_16%,transparent)]"
            />
          </div>
          <div className="ml-auto flex items-center gap-2">
            <span className="text-[12px] text-faint">Target</span>
            <div className="w-56">
              <Select value={targetId ?? ""} onChange={(e) => setTarget(e.target.value)} disabled={!live.length} aria-label="Target terminal" className="h-8 text-[12.5px]">
                {live.length === 0 && <option value="">No connected terminal</option>}
                {live.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.title}
                    {t.kind === "local" ? " (this PC)" : ""}
                  </option>
                ))}
              </Select>
            </div>
            <Button variant="primary" size="sm" onClick={() => openEditor({ prefill: cat !== "all" && cat !== "favorites" ? { category: cat } : undefined })}>
              <Plus /> New snippet
            </Button>
          </div>
        </div>

        {loaded && filtered.length === 0 ? (
          <Panel>
            <EmptyState
              icon={<SquareCode />}
              title={snippets.length ? "No matching snippets" : "No snippets yet"}
              body={snippets.length ? "Try another search or category." : "Save commands you use often, then insert them into any terminal."}
              action={
                <Button variant="primary" onClick={() => openEditor()}>
                  <Plus /> New snippet
                </Button>
              }
            />
          </Panel>
        ) : (
          <div className="grid grid-cols-1 gap-3 overflow-y-auto pb-2 xl:grid-cols-2">
            {filtered.map((s) => (
              <Panel key={s.id} className="group flex flex-col p-4 transition-[border-color] hover:border-line-strong">
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h3 className="truncate text-[14px] font-semibold tracking-tight">{s.title}</h3>
                      <button onClick={() => void toggleFavorite(s)} aria-label={s.favorite ? "Unfavorite" : "Favorite"} className="shrink-0 text-faint hover:text-warning">
                        <Star className={cn("size-3.5", s.favorite && "fill-warning text-warning")} />
                      </button>
                    </div>
                    {s.description && <p className="mt-0.5 truncate text-[12.5px] text-muted">{s.description}</p>}
                  </div>
                  <DM.Root>
                    <DM.Trigger asChild>
                      <Button variant="ghost" size="icon" className="size-7" aria-label={`More actions for ${s.title}`}>
                        <MoreHorizontal />
                      </Button>
                    </DM.Trigger>
                    <DM.Portal>
                      <DM.Content align="end" sideOffset={6} className="glass-elevated z-50 w-48 rounded-xl p-1.5 data-[state=open]:animate-[fade-in_140ms_ease-out]">
                        <DM.Item className={menuItem} onSelect={() => void copy(s)}>
                          <Copy /> Copy command
                        </DM.Item>
                        <DM.Item className={menuItem} onSelect={() => openEditor({ editing: s })}>
                          <Pencil /> Edit
                        </DM.Item>
                        <DM.Item className={menuItem} onSelect={() => openEditor({ prefill: { ...s, title: `${s.title} (copy)`, favorite: false } })}>
                          <Plus /> Duplicate
                        </DM.Item>
                        <DM.Separator className="my-1 h-px bg-line" />
                        <DM.Item
                          className={cn(menuItem, "text-danger data-[highlighted]:bg-danger/10 data-[highlighted]:text-danger")}
                          onSelect={() =>
                            askConfirm({ title: `Delete “${s.title}”?`, body: "This snippet will be removed.", confirmLabel: "Delete", danger: true, onConfirm: () => remove(s.id) })
                          }
                        >
                          <Trash2 /> Delete
                        </DM.Item>
                      </DM.Content>
                    </DM.Portal>
                  </DM.Root>
                </div>
                <pre className="selectable mt-3 max-h-28 overflow-auto whitespace-pre-wrap break-all rounded-lg border border-line bg-black/30 px-3 py-2 font-mono text-[12px] leading-relaxed text-fg">{s.command}</pre>
                <div className="mt-3 flex items-center gap-1.5">
                  <Badge tone="accent">{CATEGORY_LABEL[s.category]}</Badge>
                  {s.tags.slice(0, 3).map((t) => (
                    <Badge key={t}>#{t}</Badge>
                  ))}
                  <span className="ml-auto text-[11px] text-faint">{s.useCount ? `Used ${s.useCount}× · ${relativeTime(s.lastUsedAt)}` : ""}</span>
                </div>
                <div className="mt-3 flex items-center justify-end gap-2 border-t border-line pt-3">
                  <Button size="sm" variant="ghost" disabled={!targetId} onClick={() => void doRun(s)} title={targetId ? "Run after confirmation" : "Connect a terminal first"}>
                    <Play /> Run…
                  </Button>
                  <Button size="sm" variant="primary" disabled={!targetId} onClick={() => void doInsert(s)} title={targetId ? "Insert into the terminal without running" : "Connect a terminal first"}>
                    <CornerDownLeft /> Insert
                  </Button>
                </div>
              </Panel>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
