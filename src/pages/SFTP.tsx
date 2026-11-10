import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, ArrowRight, Eye, EyeOff, FolderTree, Plug } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { EmptyState, Panel, STATE_META, StatusDot } from "@/components/ui/misc";
import { terminalEngine } from "@/services/terminalEngine";
import { isLive } from "@/stores/sessions";
import { FilePane, type DragPayload } from "@/components/sftp/FilePane";
import { TransfersPanel } from "@/components/sftp/TransfersPanel";
import { api, errorKind, errorMessage } from "@/services/api";
import { useServers } from "@/stores/servers";
import { connectedServerIds, useSessions } from "@/stores/sessions";
import { onTransferUpdate } from "@/stores/transfers";
import { useUi } from "@/stores/ui";
import { cn } from "@/lib/utils";
import { displayName } from "@/lib/paths";
import type { FileEntry, Listing } from "@/types";

interface PaneState {
  listing: Listing | null;
  loading: boolean;
  error: string | null;
  selected: Set<string>;
}
const emptyPane = (): PaneState => ({ listing: null, loading: false, error: null, selected: new Set() });

type NameDialog = { side: "local" | "remote"; mode: "mkdir" } | { side: "local" | "remote"; mode: "rename"; entry: FileEntry };

const remoteDirOf = (p: string) => p.replace(/\/[^/]*$/, "") || "/";
const localDirOf = (p: string) => p.replace(/\\[^\\]*$/, "");

export function SFTPPage({ preferredServer }: { preferredServer?: string | null }) {
  const servers = useServers((s) => s.servers);
  const tabs = useSessions((s) => s.tabs);
  const toast = useUi((s) => s.toast);
  const confirmAsync = useUi((s) => s.confirmAsync);
  const openServerDialog = useUi((s) => s.openServerDialog);

  const connected = useMemo(() => connectedServerIds(tabs), [tabs]);
  const [serverId, setServerId] = useState<string | null>(preferredServer ?? null);
  const isConnected = !!serverId && connected.has(serverId);
  // Live state of the selected server's SSH session (if any tab exists).
  const sessionTab = tabs.filter((t) => t.kind === "ssh" && t.serverId === serverId).sort((a, b) => Number(b.state === "connected") - Number(a.state === "connected"))[0];
  const [local, setLocal] = useState<PaneState>(emptyPane);
  const [remote, setRemote] = useState<PaneState>(emptyPane);
  const [showHidden, setShowHidden] = useState(false);
  const [nameDialog, setNameDialog] = useState<NameDialog | null>(null);
  const [nameValue, setNameValue] = useState("");

  // Default selection: a connected server, else none (user picks one).
  useEffect(() => {
    if (serverId && servers.some((s) => s.id === serverId)) return;
    setServerId(servers.find((s) => connected.has(s.id))?.id ?? null);
  }, [servers, connected, serverId]);

  /** Select a server; connect it in the background if needed (stays on this page). */
  const pick = (id: string) => {
    setServerId(id);
    const server = servers.find((s) => s.id === id);
    const hasLiveTab = tabs.some((t) => t.kind === "ssh" && t.serverId === id && isLive(t.state));
    if (server && !connected.has(id) && !hasLiveTab) void terminalEngine.openTab(server);
  };

  const loadLocal = useCallback(async (path: string) => {
    setLocal((s) => ({ ...s, loading: true, error: null }));
    try {
      const listing = await api.localList(path);
      setLocal({ listing, loading: false, error: null, selected: new Set() });
    } catch (e) {
      setLocal((s) => ({ ...s, loading: false, error: errorMessage(e) }));
    }
  }, []);

  const loadRemote = useCallback(
    async (path: string) => {
      if (!serverId) return;
      setRemote((s) => ({ ...s, loading: true, error: null }));
      try {
        const listing = await api.sftpList(serverId, path);
        setRemote({ listing, loading: false, error: null, selected: new Set() });
      } catch (e) {
        setRemote((s) => ({ ...s, loading: false, error: errorMessage(e) }));
      }
    },
    [serverId],
  );

  // initial local folder
  useEffect(() => {
    void api.localHome().then(loadLocal);
  }, [loadLocal]);

  // remote home when the server changes or its session comes up
  useEffect(() => {
    setRemote(emptyPane());
    if (!serverId || !isConnected) return;
    setRemote((s) => ({ ...s, loading: true }));
    api
      .sftpHome(serverId)
      .then(loadRemote)
      .catch((e) => setRemote({ ...emptyPane(), error: errorMessage(e) }));
  }, [serverId, isConnected, loadRemote]);

  // refresh the destination pane when a transfer finishes into it
  const localPath = local.listing?.path;
  const remotePath = remote.listing?.path;
  useEffect(
    () =>
      onTransferUpdate((t) => {
        if (t.state !== "done") return;
        if (t.direction === "upload" && t.serverId === serverId && remotePath && remoteDirOf(t.dest) === remotePath) void loadRemote(remotePath);
        if (t.direction === "download" && localPath && localDirOf(t.dest) === localPath) void loadLocal(localPath);
      }),
    [serverId, remotePath, localPath, loadRemote, loadLocal],
  );

  // ---- transfers (sequential so overwrite prompts appear one at a time)
  const transfer = async (p: DragPayload) => {
    if (!serverId) return;
    const fromPane = p.side === "local" ? local : remote;
    const byPath = new Map((fromPane.listing?.entries ?? []).map((e) => [e.path, e]));
    const destDir = p.side === "local" ? remote.listing?.path : local.listing?.path;
    if (!destDir) {
      toast(p.side === "local" ? "Open a remote folder first" : "Open a local folder first (not the drive list)", "warning");
      return;
    }
    let skipped = 0;
    for (const path of p.paths) {
      const entry = byPath.get(path);
      if (entry?.isDir) {
        skipped++;
        continue;
      }
      const run = (overwrite: boolean) =>
        p.side === "local" ? api.sftpUpload(serverId, path, destDir, overwrite) : api.sftpDownload(serverId, path, destDir, overwrite);
      try {
        await run(false);
      } catch (e) {
        if (errorKind(e) === "exists") {
          const ok = await confirmAsync({
            title: "Replace existing file?",
            body: `${errorMessage(e)}. Replace it with the file you're ${p.side === "local" ? "uploading" : "downloading"}? The old file is only replaced after the transfer completes.`,
            confirmLabel: "Replace",
            danger: true,
          });
          if (ok) {
            try {
              await run(true);
            } catch (e2) {
              toast(errorMessage(e2), "danger");
            }
          }
        } else {
          toast(errorMessage(e), "danger");
        }
      }
    }
    if (skipped) toast(`${skipped} folder(s) skipped: folder transfers aren't supported yet`, "warning");
  };

  const upload = () => remote.listing && transfer({ side: "local", paths: [...local.selected] });
  const download = () => local.listing && transfer({ side: "remote", paths: [...remote.selected] });

  // ---- file operations
  const del = async (side: "local" | "remote", entries: FileEntry[]) => {
    const pane = side === "local" ? local : remote;
    const dir = pane.listing?.path;
    if (!dir || !entries.length) return;
    const names = entries.map((e) => displayName(e.name));
    const ok = await confirmAsync({
      title: `Delete ${entries.length === 1 ? `“${names[0]}”` : `${entries.length} items`}?`,
      body:
        side === "local"
          ? "Moves to the Recycle Bin."
          : `Permanently deletes from the server. This cannot be undone.${entries.some((e) => e.isDir) ? " Folders must be empty." : ""}`,
      confirmLabel: side === "local" ? "Move to Recycle Bin" : "Delete permanently",
      danger: true,
    });
    if (!ok) return;
    for (const e of entries) {
      try {
        if (side === "local") await api.localDelete(dir, e.name);
        else if (serverId) await api.sftpDelete(serverId, dir, e.name, e.isDir);
      } catch (err) {
        toast(`${displayName(e.name)}: ${errorMessage(err)}`, "danger");
      }
    }
    void (side === "local" ? loadLocal(dir) : loadRemote(dir));
  };

  const submitName = async () => {
    if (!nameDialog) return;
    const { side } = nameDialog;
    const dir = (side === "local" ? local : remote).listing?.path;
    const name = nameValue.trim();
    if (!dir || !name) return;
    try {
      if (nameDialog.mode === "mkdir") {
        if (side === "local") await api.localMkdir(dir, name);
        else if (serverId) await api.sftpMkdir(serverId, dir, name);
      } else {
        if (side === "local") await api.localRename(dir, nameDialog.entry.name, name);
        else if (serverId) await api.sftpRename(serverId, dir, nameDialog.entry.name, name);
      }
      setNameDialog(null);
      void (side === "local" ? loadLocal(dir) : loadRemote(dir));
    } catch (e) {
      toast(errorMessage(e), "danger");
    }
  };

  const openName = (d: NameDialog) => {
    setNameDialog(d);
    setNameValue(d.mode === "rename" ? d.entry.name : "");
  };

  if (servers.length === 0) {
    return (
      <Panel className="mx-auto mt-6 max-w-2xl">
        <EmptyState
          icon={<FolderTree />}
          title="No servers yet"
          body="Add a server first. SFTP then runs over its SSH session, with no extra login."
          action={
            <Button variant="primary" onClick={() => openServerDialog()}>
              <Plug /> Add a server
            </Button>
          }
        />
      </Panel>
    );
  }

  const selectedServer = servers.find((s) => s.id === serverId);
  const remotePlaceholder = (
    <Panel className="flex min-h-0 min-w-0 flex-col items-center justify-center gap-3 p-6 text-center">
      {!selectedServer ? (
        <p className="text-[13px] text-muted">Pick a server above to browse its files.</p>
      ) : sessionTab && isLive(sessionTab.state) ? (
        <>
          <StatusDot state={sessionTab.state} />
          <p className="text-[13px]">
            {STATE_META[sessionTab.state].label} to <span className="font-medium">{selectedServer.name}</span>…
          </p>
          <p className="text-[12px] text-faint">Answer any host key or password prompt to continue.</p>
        </>
      ) : (
        <>
          {sessionTab && (
            <p className="max-w-sm text-[12.5px] text-danger">
              {STATE_META[sessionTab.state].label}
              {sessionTab.message ? `: ${sessionTab.message}` : ""}
            </p>
          )}
          <p className="text-[13px] text-muted">{selectedServer.name} is not connected.</p>
          <Button variant="primary" onClick={() => pick(selectedServer.id)}>
            <Plug /> Connect
          </Button>
        </>
      )}
    </Panel>
  );

  return (
    <div className="flex h-full flex-col gap-3 pb-1">
      <div className="flex items-center gap-2">
        <div className="flex flex-wrap gap-1" role="radiogroup" aria-label="Server">
          {servers.map((s) => {
            const st = tabs.find((t) => t.kind === "ssh" && t.serverId === s.id && t.state === "connected")
              ? "connected"
              : (tabs.find((t) => t.kind === "ssh" && t.serverId === s.id && isLive(t.state))?.state ?? "idle");
            return (
              <button
                key={s.id}
                role="radio"
                aria-checked={s.id === serverId}
                onClick={() => pick(s.id)}
                title={st === "connected" ? `${s.name} (connected)` : `Connect to ${s.name}`}
                className={cn("flex h-8 items-center gap-2 rounded-[10px] border px-3 text-[12.5px] transition-colors", s.id === serverId ? "border-accent/50 bg-accent-soft text-fg" : "border-line text-muted hover:text-fg")}
              >
                <StatusDot state={st} />
                {s.name}
              </button>
            );
          })}
        </div>
        <Button variant="ghost" size="sm" className="ml-auto" onClick={() => setShowHidden((v) => !v)}>
          {showHidden ? <EyeOff /> : <Eye />} {showHidden ? "Hide hidden files" : "Show hidden files"}
        </Button>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[1fr_auto_1fr] gap-3">
        <FilePane
          side="local"
          title="This PC"
          {...local}
          showHidden={showHidden}
          onSelect={(selected) => setLocal((s) => ({ ...s, selected }))}
          onNavigate={loadLocal}
          onRefresh={() => local.listing && loadLocal(local.listing.path)}
          onMkdir={() => openName({ side: "local", mode: "mkdir" })}
          onRename={(entry) => openName({ side: "local", mode: "rename", entry })}
          onDelete={(e) => void del("local", e)}
          onDropFiles={(p) => void transfer(p)}
        />
        <div className="flex flex-col items-center justify-center gap-2">
          <Button variant="primary" size="icon" className="size-10 rounded-xl" disabled={!local.selected.size || !remote.listing} onClick={upload} aria-label="Upload selected" title="Upload selected →">
            <ArrowRight />
          </Button>
          <Button variant="secondary" size="icon" className="size-10 rounded-xl" disabled={!remote.selected.size || !local.listing?.path} onClick={download} aria-label="Download selected" title="← Download selected">
            <ArrowLeft />
          </Button>
        </div>
        {!isConnected ? remotePlaceholder : <FilePane
          side="remote"
          title={selectedServer?.name ?? "Server"}
          {...remote}
          showHidden={showHidden}
          onSelect={(selected) => setRemote((s) => ({ ...s, selected }))}
          onNavigate={loadRemote}
          onRefresh={() => remote.listing && loadRemote(remote.listing.path)}
          onMkdir={() => openName({ side: "remote", mode: "mkdir" })}
          onRename={(entry) => openName({ side: "remote", mode: "rename", entry })}
          onDelete={(e) => void del("remote", e)}
          onDropFiles={(p) => void transfer(p)}
        />}
      </div>

      <TransfersPanel />
      <p className="text-[11px] text-faint">Drag files between panes or use the arrows. Existing files are never replaced without asking; transfers write to a temporary “.yoru-partial” file first.</p>

      <Dialog
        open={!!nameDialog}
        onOpenChange={(o) => !o && setNameDialog(null)}
        title={nameDialog?.mode === "mkdir" ? "New folder" : `Rename “${nameDialog?.mode === "rename" ? displayName(nameDialog.entry.name) : ""}”`}
        footer={
          <>
            <Button variant="ghost" onClick={() => setNameDialog(null)}>
              Cancel
            </Button>
            <Button variant="primary" disabled={!nameValue.trim()} onClick={() => void submitName()}>
              {nameDialog?.mode === "mkdir" ? "Create" : "Rename"}
            </Button>
          </>
        }
      >
        <form
          className="pb-3"
          onSubmit={(e) => {
            e.preventDefault();
            void submitName();
          }}
        >
          <Input autoFocus value={nameValue} onChange={(e) => setNameValue(e.target.value)} placeholder="Name" className="font-mono" />
        </form>
      </Dialog>
    </div>
  );
}
