import { Monitor, Plus, SquareTerminal } from "lucide-react";
import { useLocalProfiles } from "@/stores/localProfiles";
import * as DM from "@radix-ui/react-dropdown-menu";
import { Button } from "@/components/ui/Button";
import { EmptyState, Kbd, Panel, STATE_META } from "@/components/ui/misc";
import { TerminalHost } from "@/components/terminal/TerminalHost";
import { TerminalTabs, TerminalToolbar } from "@/components/terminal/TerminalTabs";
import { terminalEngine } from "@/services/terminalEngine";
import { useSessions } from "@/stores/sessions";
import { useServers } from "@/stores/servers";
import { useUi } from "@/stores/ui";
import { cn } from "@/lib/utils";

const menuLabel = "px-2.5 pb-1 pt-1.5 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-faint";

function NewTabMenu() {
  const servers = useServers((s) => s.servers);
  const profiles = useLocalProfiles((s) => s.profiles);
  const openServerDialog = useUi((s) => s.openServerDialog);
  return (
    <DM.Root>
      <DM.Trigger asChild>
        <Button variant="ghost" size="icon" aria-label="New session" title="New tab (Ctrl+Shift+T opens your default local shell)">
          <Plus />
        </Button>
      </DM.Trigger>
      <DM.Portal>
        <DM.Content align="start" sideOffset={6} className="glass-elevated z-50 max-h-[420px] w-72 overflow-y-auto rounded-xl p-1.5 data-[state=open]:animate-[fade-in_140ms_ease-out]">
          {profiles.length > 0 && <DM.Label className={menuLabel}>This PC</DM.Label>}
          {profiles.map((p) => (
            <DM.Item
              key={p.id}
              onSelect={() => void terminalEngine.openLocal(p)}
              className="flex cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 outline-none data-[highlighted]:bg-white/[0.06]"
            >
              <Monitor className="size-4 shrink-0 text-faint" />
              <span className="min-w-0 flex-1">
                <span className="block text-[13px]">
                  {p.name}
                  {p.isDefault && <span className="ml-1.5 text-[10.5px] text-faint">default</span>}
                </span>
                <span className="block truncate font-mono text-[11px] text-faint">{p.program}</span>
              </span>
            </DM.Item>
          ))}
          {profiles.length > 0 && servers.length > 0 && <DM.Separator className="my-1 h-px bg-line" />}
          {servers.length > 0 && <DM.Label className={menuLabel}>Servers</DM.Label>}
          {servers.map((s) => (
            <DM.Item
              key={s.id}
              onSelect={() => void terminalEngine.openTab(s)}
              className="flex cursor-default flex-col rounded-lg px-2.5 py-1.5 outline-none data-[highlighted]:bg-white/[0.06]"
            >
              <span className="text-[13px]">{s.name}</span>
              <span className="font-mono text-[11px] text-faint">
                {s.username}@{s.host}
              </span>
            </DM.Item>
          ))}
          {servers.length > 0 && <DM.Separator className="my-1 h-px bg-line" />}
          <DM.Item onSelect={() => openServerDialog({ connectAfterSave: true })} className="flex cursor-default items-center gap-2 rounded-lg px-2.5 py-1.5 text-[13px] text-muted outline-none data-[highlighted]:bg-white/[0.06] data-[highlighted]:text-fg">
            <Plus className="size-4" /> New server…
          </DM.Item>
        </DM.Content>
      </DM.Portal>
    </DM.Root>
  );
}

/** Always mounted (hidden when another page is active) so sessions survive navigation. */
export function TerminalPage({ visible }: { visible: boolean }) {
  const tabs = useSessions((s) => s.tabs);
  const active = useSessions((s) => s.tabs.find((t) => t.id === s.activeTabId));
  const setPage = useUi((s) => s.setPage);
  const defaultProfile = useLocalProfiles((s) => s.profiles.find((p) => p.isDefault) ?? s.profiles[0]);
  const banner =active && active.state !== "connected" ? STATE_META[active.state] : null;

  return (
    // visibility:hidden (not display:none) so xterm can still measure itself when
    // a tab is created from another page. Terminal elements never set
    // visibility:visible themselves, so nothing paints through.
    <div className={cn("absolute inset-0 flex flex-col px-5 pb-5", !visible && "pointer-events-none invisible")} aria-hidden={!visible} inert={!visible}>
      <Panel className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="flex h-11 shrink-0 items-center gap-2 border-b border-line px-2">
          <TerminalTabs />
          <NewTabMenu />
          <div className="ml-auto" />
          <TerminalToolbar />
        </div>
        {banner && (
          <div className={cn("flex items-center gap-2 border-b border-line px-4 py-1.5 text-[12px]", banner.color.includes("danger") ? "bg-danger/[0.08] text-danger" : "bg-warning/[0.06] text-warning")}>
            <span className="font-medium">{banner.label}</span>
            {active?.message && <span className="truncate text-muted">— {active.message}</span>}
          </div>
        )}
        <div className="relative min-h-0 flex-1">
          <TerminalHost visible={visible} />
          {tabs.length === 0 && (
            <div className="absolute inset-0 grid place-items-center">
              <EmptyState
                icon={<SquareTerminal />}
                title="No open sessions"
                body={
                  <>
                    Connect to a server, or open a shell on this PC using your Windows Terminal profiles. Copy with <Kbd>Ctrl Shift C</Kbd>, paste with <Kbd>Ctrl Shift V</Kbd> or right-click.
                  </>
                }
                action={
                  <div className="flex gap-2">
                    <Button variant="primary" onClick={() => setPage("servers")}>
                      Choose a server
                    </Button>
                    {defaultProfile && (
                      <Button onClick={() => void terminalEngine.openLocal(defaultProfile)}>
                        <Monitor /> {defaultProfile.name}
                      </Button>
                    )}
                  </div>
                }
              />
            </div>
          )}
        </div>
      </Panel>
    </div>
  );
}
