import { useMemo, useState } from "react";
import { LayoutGrid, List, Plus, SearchX, Server, Star } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge, EmptyState, Panel, StatusDot } from "@/components/ui/misc";
import { Select } from "@/components/ui/Input";
import { ServerCard, ServerMenu, useServerActions, useServerState } from "@/components/servers/ServerCard";
import { useServers } from "@/stores/servers";
import { useSessions } from "@/stores/sessions";
import { useUi } from "@/stores/ui";
import { cn, displayEndpoint, relativeTime } from "@/lib/utils";
import type { ServerView } from "@/types";

type Sort = "name" | "status" | "lastConnected";

function Row({ s }: { s: ServerView }) {
  const state = useServerState(s.id);
  const a = useServerActions(s);
  return (
    <div className="grid grid-cols-[20px_1.2fr_1.4fr_1fr_120px_auto] items-center gap-3 border-b border-line px-4 py-2.5 text-[13px] last:border-0 hover:bg-ink/[0.025]">
      <StatusDot state={state} />
      <div className="flex min-w-0 items-center gap-2">
        <span className="truncate font-medium">{s.name}</span>
        {s.favorite && <Star className="size-3 shrink-0 fill-warning text-warning" />}
      </div>
      <span className="selectable truncate font-mono text-[12px] text-muted">{displayEndpoint(s.username, s.host, s.port)}</span>
      <div className="flex min-w-0 gap-1 overflow-hidden">
        {s.groupName && <Badge tone="accent">{s.groupName}</Badge>}
        {s.tags.slice(0, 2).map((t) => (
          <Badge key={t}>#{t}</Badge>
        ))}
      </div>
      <span className="text-[12px] text-faint">{relativeTime(s.lastConnectedAt)}</span>
      <div className="flex items-center gap-1">
        <Button size="sm" variant="secondary" onClick={a.connect}>
          Connect
        </Button>
        <ServerMenu server={s} />
      </div>
    </div>
  );
}

export function Servers() {
  const { servers, loaded } = useServers();
  const tabs = useSessions((s) => s.tabs);
  const search = useUi((s) => s.search);
  const setSearch = useUi((s) => s.setSearch);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [sort, setSort] = useState<Sort>("name");
  const [tag, setTag] = useState<string | null>(null);
  const [group, setGroup] = useState<string>("");
  const [favOnly, setFavOnly] = useState(false);

  const allTags = useMemo(() => [...new Set(servers.flatMap((s) => s.tags))].sort(), [servers]);
  const groups = useMemo(() => [...new Set(servers.map((s) => s.groupName).filter(Boolean) as string[])].sort(), [servers]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    const connected = new Set(tabs.filter((t) => t.state === "connected").map((t) => t.serverId));
    const list = servers.filter(
      (s) =>
        (!q || [s.name, s.host, s.username, s.groupName ?? "", s.notes, ...s.tags].some((f) => f.toLowerCase().includes(q))) &&
        (!tag || s.tags.includes(tag)) &&
        (!group || s.groupName === group) &&
        (!favOnly || s.favorite),
    );
    return list.sort((a, b) => {
      if (sort === "status") return Number(connected.has(b.id)) - Number(connected.has(a.id)) || a.name.localeCompare(b.name);
      if (sort === "lastConnected") return (b.lastConnectedAt ?? "").localeCompare(a.lastConnectedAt ?? "");
      return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
    });
  }, [servers, search, tag, group, favOnly, sort, tabs]);

  const grouped = useMemo(() => {
    const m = new Map<string, ServerView[]>();
    for (const s of filtered) {
      const k = s.groupName ?? "Ungrouped";
      m.set(k, [...(m.get(k) ?? []), s]);
    }
    return [...m.entries()].sort(([a], [b]) => (a === "Ungrouped" ? 1 : b === "Ungrouped" ? -1 : a.localeCompare(b)));
  }, [filtered]);

  if (loaded && servers.length === 0) {
    return (
      <Panel className="mx-auto mt-6 max-w-2xl">
        <EmptyState
          icon={<Server />}
          title="No servers yet"
          body="Save a profile for each machine you manage: Linux, Windows OpenSSH, Proxmox, local VMs."
          action={
            <Button variant="primary" onClick={() => openServerDialog()}>
              <Plus /> Add server
            </Button>
          }
        />
      </Panel>
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-2">
        <button
          onClick={() => setFavOnly((v) => !v)}
          className={cn("flex h-8 items-center gap-1.5 rounded-[10px] border px-3 text-[12.5px] transition-colors", favOnly ? "border-warning/40 bg-warning/10 text-warning" : "border-line text-muted hover:text-fg")}
        >
          <Star className={cn("size-3.5", favOnly && "fill-warning")} /> Favorites
        </button>
        {groups.length > 0 && (
          <div className="w-44">
            <Select value={group} onChange={(e) => setGroup(e.target.value)} aria-label="Filter by group" className="h-8 text-[12.5px]">
              <option value="">All groups</option>
              {groups.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </Select>
          </div>
        )}
        <div className="flex flex-wrap gap-1">
          {allTags.map((t) => (
            <button
              key={t}
              onClick={() => setTag(tag === t ? null : t)}
              className={cn("h-7 rounded-lg border px-2 text-[12px] transition-colors", tag === t ? "border-accent/50 bg-accent-soft text-fg" : "border-line text-muted hover:text-fg")}
            >
              #{t}
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-2">
          <div className="w-44">
            <Select value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="Sort servers" className="h-8 text-[12.5px]">
              <option value="name">Sort by name</option>
              <option value="status">Sort by status</option>
              <option value="lastConnected">Sort by last connection</option>
            </Select>
          </div>
          <div className="flex rounded-[10px] border border-line p-0.5">
            <Button variant="ghost" size="icon" className={cn("size-7", view === "grid" && "bg-ink/10 text-fg")} onClick={() => setView("grid")} aria-label="Grid view">
              <LayoutGrid />
            </Button>
            <Button variant="ghost" size="icon" className={cn("size-7", view === "list" && "bg-ink/10 text-fg")} onClick={() => setView("list")} aria-label="List view">
              <List />
            </Button>
          </div>
        </div>
      </div>

      {filtered.length === 0 ? (
        <Panel>
          <EmptyState
            icon={<SearchX />}
            title="No matching servers"
            body="Try a different search or clear the filters."
            action={
              <Button
                onClick={() => {
                  setSearch("");
                  setTag(null);
                  setGroup("");
                  setFavOnly(false);
                }}
              >
                Clear filters
              </Button>
            }
          />
        </Panel>
      ) : (
        grouped.map(([name, list]) => (
          <section key={name}>
            <h2 className="mb-2.5 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-[0.08em] text-faint">
              {name} <span className="font-normal normal-case tracking-normal">· {list.length}</span>
            </h2>
            {view === "grid" ? (
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2 2xl:grid-cols-3">
                {list.map((s, i) => (
                  <ServerCard key={s.id} server={s} index={i} />
                ))}
              </div>
            ) : (
              <Panel className="overflow-hidden">
                {list.map((s) => (
                  <Row key={s.id} s={s} />
                ))}
              </Panel>
            )}
          </section>
        ))
      )}
    </div>
  );
}
