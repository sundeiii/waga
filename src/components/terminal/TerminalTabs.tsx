import { useEffect, useRef, useState } from "react";
import { motion } from "motion/react";
import { ArrowDown, ArrowRightLeft, ArrowUp, Eraser, Minus, Monitor, Plus, PlugZap, RotateCw, Search, SquareCode, X } from "lucide-react";
import { useSnippets } from "@/stores/snippets";
import { useForwards } from "@/stores/forwards";
import { Button } from "@/components/ui/Button";
import { StatusDot, STATE_META } from "@/components/ui/misc";
import { terminalEngine } from "@/services/terminalEngine";
import { isLive, useSessions } from "@/stores/sessions";
import { useSettings } from "@/stores/settings";
import { cn } from "@/lib/utils";

export function TerminalTabs() {
  const tabs = useSessions((s) => s.tabs);
  const active = useSessions((s) => s.activeTabId);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto" role="tablist" aria-label="Terminal sessions">
      {tabs.map((t) => (
        <div
          key={t.id}
          role="tab"
          aria-selected={t.id === active}
          tabIndex={0}
          onClick={() => terminalEngine.activate(t.id)}
          onKeyDown={(e) => e.key === "Enter" && terminalEngine.activate(t.id)}
          onAuxClick={(e) => e.button === 1 && terminalEngine.close(t.id)}
          title={`${t.title} — ${STATE_META[t.state].label}${t.message ? `: ${t.message}` : ""}`}
          className={cn(
            // Inset focus ring: an outer outline gets clipped by the scrolling tab strip.
            "group relative flex h-8 max-w-52 shrink-0 cursor-default items-center gap-2 rounded-[10px] pl-3 pr-1.5 text-[12.5px] outline-none transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent/60",
            t.id === active ? "text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg",
          )}
        >
          {t.id === active && (
            <motion.span layoutId="tab-active" className="absolute inset-0 rounded-[10px] border border-line bg-white/[0.07]" transition={{ type: "spring", stiffness: 500, damping: 40 }} />
          )}
          {t.kind === "local" ? <Monitor className="relative size-3.5 shrink-0 text-faint" aria-label="Local shell" /> : <StatusDot state={t.state} className="relative" />}
          <span className="relative truncate">{t.title}</span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              terminalEngine.close(t.id);
            }}
            className="relative grid size-5 place-items-center rounded-md text-faint opacity-60 transition hover:bg-white/10 hover:text-fg group-hover:opacity-100"
            aria-label={`Close ${t.title}`}
          >
            <X className="size-3.5" />
          </button>
        </div>
      ))}
    </div>
  );
}

export function TerminalToolbar() {
  const activeId = useSessions((s) => s.activeTabId);
  const tab = useSessions((s) => s.tabs.find((t) => t.id === s.activeTabId));
  const fontSize = useSettings((s) => s.settings?.terminal.fontSize ?? 14);
  const update = useSettings((s) => s.update);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [miss, setMiss] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.code === "KeyF") {
        e.preventDefault();
        setSearchOpen(true);
        requestAnimationFrame(() => inputRef.current?.select());
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!tab || !activeId) return null;

  const font = (d: number) => update((s) => ({ ...s, terminal: { ...s.terminal, fontSize: Math.min(32, Math.max(8, s.terminal.fontSize + d)) } }));
  const find = (dir: "next" | "prev") => setMiss(!terminalEngine.search(activeId, query, dir));
  const closeSearch = () => {
    setSearchOpen(false);
    terminalEngine.clearSearch(activeId);
    terminalEngine.focus();
  };

  return (
    <div className="flex shrink-0 items-center gap-1">
      {searchOpen ? (
        <div className="flex items-center gap-1 rounded-[10px] border border-line bg-black/30 pl-2.5">
          <Search className="size-3.5 text-faint" />
          <input
            ref={inputRef}
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setMiss(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") find(e.shiftKey ? "prev" : "next");
              if (e.key === "Escape") closeSearch();
            }}
            placeholder="Find in output"
            aria-label="Find in terminal output"
            className={cn("h-7 w-40 bg-transparent text-[12.5px] outline-none placeholder:text-faint", miss && "text-danger")}
          />
          <Button variant="ghost" size="icon" className="size-7" onClick={() => find("prev")} aria-label="Previous match">
            <ArrowUp />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" onClick={() => find("next")} aria-label="Next match">
            <ArrowDown />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" onClick={closeSearch} aria-label="Close search">
            <X />
          </Button>
        </div>
      ) : (
        <Button variant="ghost" size="icon" onClick={() => setSearchOpen(true)} title="Find (Ctrl+Shift+F)" aria-label="Find in output">
          <Search />
        </Button>
      )}
      {tab.kind === "ssh" && (
        <Button variant="ghost" size="icon" onClick={() => useForwards.getState().openDialog(tab.serverId)} title="Port forwarding" aria-label="Port forwarding">
          <ArrowRightLeft />
        </Button>
      )}
      <Button variant="ghost" size="icon" onClick={() => useSnippets.getState().setPalette(true)} disabled={tab.state !== "connected"} title="Insert snippet (Ctrl+Shift+S)" aria-label="Insert snippet">
        <SquareCode />
      </Button>
      <Button variant="ghost" size="icon" onClick={() => font(-1)} title="Smaller font" aria-label="Decrease font size">
        <Minus />
      </Button>
      <span className="w-7 text-center font-mono text-[11.5px] text-faint">{fontSize}</span>
      <Button variant="ghost" size="icon" onClick={() => font(1)} title="Larger font" aria-label="Increase font size">
        <Plus />
      </Button>
      <Button variant="ghost" size="icon" onClick={() => terminalEngine.clear(activeId)} title="Clear scrollback" aria-label="Clear terminal">
        <Eraser />
      </Button>
      {isLive(tab.state) && tab.state !== "connected" ? null : tab.state === "connected" ? (
        <Button variant="ghost" size="sm" onClick={() => terminalEngine.disconnect(activeId)}>
          <PlugZap /> {tab.kind === "local" ? "End process" : "Disconnect"}
        </Button>
      ) : (
        <Button variant="primary" size="sm" onClick={() => terminalEngine.reconnect(activeId)}>
          <RotateCw /> {tab.kind === "local" ? "Restart" : "Reconnect"}
        </Button>
      )}
    </div>
  );
}
