import { useState } from "react";
import { Zap } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useUi } from "@/stores/ui";
import { useSettings } from "@/stores/settings";
import { useServers } from "@/stores/servers";
import { terminalEngine } from "@/services/terminalEngine";
import { parseQuickConnect } from "@/lib/quickConnect";

/**
 * Quick Connect: if the target matches a saved profile, connect to it;
 * otherwise open a prefilled profile form (a profile is required so the
 * host key can be tracked and credentials stored safely).
 */
export function QuickConnectDialog() {
  const open = useUi((s) => s.quickConnectOpen);
  const setOpen = useUi((s) => s.setQuickConnect);
  const openServerDialog = useUi((s) => s.openServerDialog);
  const setPage = useUi((s) => s.setPage);
  const conn = useSettings((s) => s.settings?.connections);
  const servers = useServers((s) => s.servers);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const go = () => {
    const t = parseQuickConnect(value, conn?.defaultUsername ?? "", conn?.defaultPort ?? 22);
    if (typeof t === "string") {
      setError(t);
      return;
    }
    const match = servers.find((s) => s.host.toLowerCase() === t.host.toLowerCase() && s.port === t.port && s.username === t.username);
    setOpen(false);
    setValue("");
    setError(null);
    if (match) {
      setPage("terminal");
      void terminalEngine.openTab(match);
    } else {
      openServerDialog({ prefill: { name: t.host, host: t.host, port: t.port, username: t.username }, connectAfterSave: true });
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      icon={
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          <Zap className="size-5" />
        </div>
      }
      title="Quick connect"
      description="user@host[:port]. Matches a saved server, or creates a profile first."
      footer={
        <>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button variant="primary" onClick={go}>
            Connect
          </Button>
        </>
      }
    >
      <form
        className="pb-3"
        onSubmit={(e) => {
          e.preventDefault();
          go();
        }}
      >
        <Input
          autoFocus
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setError(null);
          }}
          placeholder="root@192.168.1.10:22"
          className="h-10 font-mono text-[13.5px]"
          aria-invalid={!!error}
        />
        {error && <p className="mt-2 text-[12px] text-danger">{error}</p>}
      </form>
    </Dialog>
  );
}
