import { useEffect, useMemo, useRef, useState } from "react";
import * as RD from "@radix-ui/react-dialog";
import { CornerDownLeft, Search, SquareCode } from "lucide-react";
import { Kbd } from "@/components/ui/misc";
import { CATEGORY_LABEL, insertSnippet, matchesSnippet, runSnippet, useSnippets } from "@/stores/snippets";
import { useSessions } from "@/stores/sessions";
import { cn } from "@/lib/utils";

/** Ctrl+Shift+S over the terminal: Enter inserts (never runs), Ctrl+Enter runs after confirmation. */
export function SnippetPalette() {
  const { paletteOpen, setPalette, snippets, loaded, load } = useSnippets();
  const activeTab = useSessions((s) => s.tabs.find((t) => t.id === s.activeTabId));
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (paletteOpen) {
      setQ("");
      setIdx(0);
      if (!loaded) void load();
    }
  }, [paletteOpen, loaded, load]);

  const results = useMemo(() => {
    const r = snippets.filter((s) => matchesSnippet(s, q));
    // favorites, then most used
    return r.sort((a, b) => Number(b.favorite) - Number(a.favorite) || b.useCount - a.useCount).slice(0, 50);
  }, [snippets, q]);

  useEffect(() => setIdx(0), [q]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-idx="${idx}"]`)?.scrollIntoView({ block: "nearest" });
  }, [idx]);

  const usable = activeTab?.state === "connected";
  const pick = async (run: boolean) => {
    const s = results[idx];
    if (!s || !activeTab || !usable) return;
    setPalette(false);
    await (run ? runSnippet(s, activeTab.id) : insertSnippet(s, activeTab.id));
  };

  return (
    <RD.Root open={paletteOpen} onOpenChange={setPalette}>
      <RD.Portal>
        <RD.Overlay className="fixed inset-0 z-40 bg-black/35 backdrop-blur-[3px]" />
        <RD.Content
          className="glass-elevated fixed left-1/2 top-[14vh] z-50 flex max-h-[60vh] w-[min(640px,calc(100vw-48px))] -translate-x-1/2 flex-col overflow-hidden rounded-2xl data-[state=open]:animate-[fade-in_140ms_ease-out]"
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setIdx((i) => Math.min(i + 1, results.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setIdx((i) => Math.max(i - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              void pick(e.ctrlKey);
            }
          }}
        >
          <RD.Title className="sr-only">Insert snippet</RD.Title>
          <RD.Description className="sr-only">Search snippets. Enter inserts without running; Ctrl+Enter runs after confirmation.</RD.Description>
          <div className="flex items-center gap-2.5 border-b border-line px-4">
            <Search className="size-4 text-faint" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder={usable ? `Insert into ${activeTab?.title}…` : "No connected terminal tab"}
              className="h-12 flex-1 bg-transparent text-[14px] outline-none placeholder:text-faint"
              aria-label="Search snippets"
            />
          </div>
          <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-1.5" role="listbox">
            {results.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-10 text-[13px] text-faint">
                <SquareCode className="size-5" />
                {snippets.length ? "No matches" : "No snippets yet. Add some on the Snippets page."}
              </div>
            ) : (
              results.map((s, i) => (
                <div
                  key={s.id}
                  data-idx={i}
                  role="option"
                  aria-selected={i === idx}
                  onMouseMove={() => setIdx(i)}
                  onClick={() => void pick(false)}
                  className={cn("cursor-default rounded-lg px-3 py-2", i === idx ? "bg-accent-soft" : "")}
                >
                  <div className="flex items-center gap-2 text-[13px]">
                    <span className="truncate font-medium">{s.title}</span>
                    <span className="shrink-0 text-[11px] text-faint">{CATEGORY_LABEL[s.category]}</span>
                  </div>
                  <div className="truncate font-mono text-[11.5px] text-muted">{s.command.split("\n")[0]}{s.command.includes("\n") ? " …" : ""}</div>
                </div>
              ))
            )}
          </div>
          <div className="flex items-center gap-4 border-t border-line px-4 py-2 text-[11.5px] text-faint">
            <span className="flex items-center gap-1.5">
              <Kbd>
                <CornerDownLeft className="inline size-3" />
              </Kbd>
              insert (doesn't run)
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>Ctrl ↵</Kbd> run after confirming
            </span>
            <span className="ml-auto flex items-center gap-1.5">
              <Kbd>Esc</Kbd> close
            </span>
          </div>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  );
}
