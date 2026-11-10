import { useEffect, useRef } from "react";
import * as DM from "@radix-ui/react-dropdown-menu";
import { Palette, Plus, Search, Settings2, Zap } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Kbd } from "@/components/ui/misc";
import { useUi } from "@/stores/ui";
import { useSessions } from "@/stores/sessions";
import { useSettings } from "@/stores/settings";
import { NAV } from "./Sidebar";
import { ACCENTS } from "@/pages/Settings";

const TITLES: Record<string, string> = Object.fromEntries([...NAV.map((n) => [n.id, n.label]), ["settings", "Settings"]]);

export function TopBar() {
  const page = useUi((s) => s.page);
  const search = useUi((s) => s.search);
  const setSearch = useUi((s) => s.setSearch);
  const setPage = useUi((s) => s.setPage);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const setQuickConnect = useUi((s) => s.setQuickConnect);
  const tabs = useSessions((s) => s.tabs);
  const update = useSettings((s) => s.update);
  const accent = useSettings((s) => s.settings?.appearance.accent);
  const connected = tabs.filter((t) => t.state === "connected").length;
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <header className="relative z-10 flex h-14 shrink-0 items-center gap-3 px-5">
      <h1 className="min-w-28 text-[17px] font-semibold tracking-[-0.015em]">{TITLES[page]}</h1>

      <div className="relative mx-auto w-full max-w-md">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
        <input
          ref={inputRef}
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            if (page !== "servers" && e.target.value) setPage("servers");
          }}
          onKeyDown={(e) => e.key === "Escape" && (setSearch(""), inputRef.current?.blur())}
          placeholder="Search servers, hosts, tags…"
          aria-label="Search servers"
          spellCheck={false}
          className="h-9 w-full rounded-xl border border-line bg-black/25 pl-9 pr-14 text-[13px] text-fg placeholder:text-faint transition-colors hover:border-line-strong outline-none focus:border-accent/60 focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_16%,transparent)]"
        />
        <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2">
          <Kbd>Ctrl K</Kbd>
        </span>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={() => setPage("terminal")}
          className="flex h-9 items-center gap-2 rounded-xl border border-line bg-white/[0.03] px-3 text-[12.5px] text-muted transition-colors hover:text-fg"
          title="Active SSH sessions"
        >
          <span className={connected ? "size-1.5 rounded-full bg-success shadow-[0_0_6px_var(--color-success)]" : "size-1.5 rounded-full bg-faint"} />
          {connected} live
        </button>
        <Button variant="secondary" onClick={() => setQuickConnect(true)} title="Quick Connect (Ctrl+Shift+Q)">
          <Zap /> Quick Connect
        </Button>
        <Button variant="primary" onClick={() => openServerDialog()} title="New Server (Ctrl+N)">
          <Plus /> New Server
        </Button>
        <DM.Root>
          <DM.Trigger asChild>
            <Button variant="ghost" size="icon" aria-label="Theme and settings">
              <Settings2 />
            </Button>
          </DM.Trigger>
          <DM.Portal>
            <DM.Content
              align="end"
              sideOffset={8}
              className="glass-elevated z-50 w-56 rounded-xl p-1.5 text-[13px] data-[state=open]:animate-[fade-in_150ms_ease-out]"
            >
              <DM.Label className="flex items-center gap-2 px-2 py-1.5 text-[11px] uppercase tracking-wider text-faint">
                <Palette className="size-3.5" /> Accent
              </DM.Label>
              <div className="flex flex-wrap gap-1.5 px-2 pb-2">
                {ACCENTS.map((a) => (
                  <button
                    key={a.value}
                    title={a.name}
                    aria-label={`Accent ${a.name}`}
                    onClick={() => update((s) => ({ ...s, appearance: { ...s.appearance, accent: a.value } }))}
                    className="size-6 rounded-full border-2 transition-transform hover:scale-110"
                    style={{ background: a.value, borderColor: accent?.toLowerCase() === a.value.toLowerCase() ? "#fff" : "transparent" }}
                  />
                ))}
              </div>
              <DM.Separator className="my-1 h-px bg-line" />
              <DM.Item
                onSelect={() => setPage("settings")}
                className="flex cursor-default items-center gap-2 rounded-lg px-2 py-1.5 text-muted outline-none data-[highlighted]:bg-white/[0.06] data-[highlighted]:text-fg"
              >
                <Settings2 className="size-4" /> All settings
              </DM.Item>
            </DM.Content>
          </DM.Portal>
        </DM.Root>
      </div>
    </header>
  );
}
