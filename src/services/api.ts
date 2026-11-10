import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ActivityEntry,
  AppError,
  AppSettings,
  ForwardInput,
  ForwardStatus,
  HostCa,
  KnownHost,
  Listing,
  SavedForward,
  LocalProfile,
  SecretUpdate,
  ServerInput,
  ServerView,
  SessionEvent,
  Snippet,
  SnippetInput,
  UpdateInfo,
} from "@/types";

/** Normalize anything thrown by invoke into a readable message. */
export function errorKind(e: unknown): string | null {
  return e && typeof e === "object" && "kind" in e ? String((e as AppError).kind) : null;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as AppError).message);
  return "Unexpected error";
}

export const api = {
  // settings
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => invoke<AppSettings>("update_settings", { settings }),
  resetSettings: () => invoke<AppSettings>("reset_settings"),
  exportSettings: (path: string) => invoke<void>("export_settings", { path }),
  deleteAllData: () => invoke<void>("delete_all_data"),
  appVersion: () => invoke<string>("app_version"),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),
  setAutostart:(enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  autostartEnabled: () => invoke<boolean>("autostart_enabled"),
  /** Native notification (backend honours the user's setting). */
  notify: (title: string, body: string) => invoke<void>("notify", { title, body }).catch(() => undefined),
  setWindowAppearance: (material: AppSettings["appearance"]["windowMaterial"], theme: AppSettings["appearance"]["theme"]) =>
    invoke<void>("set_window_appearance", { material, theme }),

  // servers
  listServers: () => invoke<ServerView[]>("list_servers"),
  createServer: (input: ServerInput, secrets: SecretUpdate) =>
    invoke<ServerView>("create_server", { input, secrets }),
  updateServer: (id: string, input: ServerInput, secrets: SecretUpdate) =>
    invoke<ServerView>("update_server", { id, input, secrets }),
  deleteServer: (id: string) => invoke<void>("delete_server", { id }),
  duplicateServer: (id: string) => invoke<ServerView>("duplicate_server", { id }),
  setFavorite: (id: string, favorite: boolean) => invoke<void>("set_server_favorite", { id, favorite }),
  deleteServerCredentials: (id: string) => invoke<void>("delete_server_credentials", { id }),
  deleteAllCredentials: () => invoke<number>("delete_all_credentials"),

  // activity & known hosts
  listActivity: (limit?: number) => invoke<ActivityEntry[]>("list_activity", { limit }),
  clearActivity: () => invoke<void>("clear_activity"),
  listKnownHosts: () => invoke<KnownHost[]>("list_known_hosts"),
  listHostCas: () => invoke<HostCa[]>("list_host_cas"),
  addHostCa: (pattern: string, publicKey: string, comment: string) => invoke<HostCa>("add_host_ca", { pattern, publicKey, comment }),
  removeHostCa: (id: string) => invoke<void>("remove_host_ca", { id }),
  removeKnownHost:(host: string, port: number) => invoke<number>("remove_known_host", { host, port }),

  // port forwarding
  listSavedForwards: (serverId: string) => invoke<SavedForward[]>("list_saved_forwards", { serverId }),
  saveForward: (serverId: string, input: ForwardInput, id?: string) => invoke<SavedForward>("save_forward", { serverId, id: id ?? null, input }),
  deleteSavedForward: (id: string) => invoke<void>("delete_saved_forward", { id }),
  startForward: (serverId: string, savedId: string | null, adhoc?: { localPort: number; remoteHost: string; remotePort: number }) =>
    invoke<ForwardStatus>("start_forward", { serverId, savedId, localPort: adhoc?.localPort ?? null, remoteHost: adhoc?.remoteHost ?? null, remotePort: adhoc?.remotePort ?? null }),
  stopForward: (id: string) => invoke<boolean>("stop_forward", { id }),
  listActiveForwards: () => invoke<ForwardStatus[]>("list_active_forwards"),

  // snippets
  listSnippets: () => invoke<Snippet[]>("list_snippets"),
  createSnippet: (input: SnippetInput) => invoke<Snippet>("create_snippet", { input }),
  updateSnippet: (id: string, input: SnippetInput) => invoke<Snippet>("update_snippet", { id, input }),
  deleteSnippet: (id: string) => invoke<void>("delete_snippet", { id }),
  setSnippetFavorite: (id: string, favorite: boolean) => invoke<void>("set_snippet_favorite", { id, favorite }),
  markSnippetUsed: (id: string) => invoke<void>("mark_snippet_used", { id }),

  // local terminals
  localProfiles: () => invoke<LocalProfile[]>("local_profiles"),
  localOpen: (sessionId: string, profileId: string, cols: number, rows: number, events: Channel<SessionEvent>, output: Channel<ArrayBuffer>) =>
    invoke<void>("local_open", { sessionId, profileId, cols, rows, events, output }),
  sessionsCloseAll: () => invoke<number>("sessions_close_all"),

  // sftp (remote) — uses the server's open SSH session
  sftpHome: (serverId: string) => invoke<string>("sftp_home", { serverId }),
  sftpList: (serverId: string, path: string) => invoke<Listing>("sftp_list", { serverId, path }),
  sftpMkdir: (serverId: string, dir: string, name: string) => invoke<string>("sftp_mkdir", { serverId, dir, name }),
  sftpRename: (serverId: string, dir: string, from: string, to: string) => invoke<void>("sftp_rename", { serverId, dir, from, to }),
  sftpDelete: (serverId: string, dir: string, name: string, isDir: boolean) => invoke<void>("sftp_delete", { serverId, dir, name, isDir }),
  sftpUpload: (serverId: string, localPath: string, remoteDir: string, overwrite: boolean) =>
    invoke<string>("sftp_upload", { serverId, localPath, remoteDir, overwrite }),
  sftpDownload: (serverId: string, remotePath: string, localDir: string, overwrite: boolean) =>
    invoke<string>("sftp_download", { serverId, remotePath, localDir, overwrite }),
  sftpCancel: (id: string) => invoke<boolean>("sftp_cancel", { id }),
  // local files
  localHome: () => invoke<string>("local_home"),
  localList: (path: string) => invoke<Listing>("local_list", { path }),
  localMkdir: (dir: string, name: string) => invoke<string>("local_mkdir", { dir, name }),
  localRename: (dir: string, from: string, to: string) => invoke<void>("local_rename", { dir, from, to }),
  localDelete: (dir: string, name: string) => invoke<void>("local_delete", { dir, name }),

  // monitoring
  monitorStart: (serverId: string) => invoke<void>("monitor_start", { serverId }),
  monitorStop: (serverId: string) => invoke<void>("monitor_stop", { serverId }),
  monitorActive: () => invoke<string[]>("monitor_active"),

  // ssh
  sshOpen: (
    sessionId: string,
    serverId: string,
    cols: number,
    rows: number,
    events: Channel<SessionEvent>,
    output: Channel<ArrayBuffer>,
  ) => invoke<void>("ssh_open", { sessionId, serverId, cols, rows, events, output }),
  sshWrite: (sessionId: string, data: string) => invoke<void>("ssh_write", { sessionId, data }),
  sshResize: (sessionId: string, cols: number, rows: number) =>
    invoke<void>("ssh_resize", { sessionId, cols, rows }),
  sshClose: (sessionId: string) => invoke<void>("ssh_close", { sessionId }),
  sshHostKeyDecision: (sessionId: string, accept: boolean, acknowledgeChanged: boolean) =>
    invoke<void>("ssh_host_key_decision", { sessionId, accept, acknowledgeChanged }),
  sshProvideAnswers: (sessionId: string, answers: string[] | null) => invoke<void>("ssh_provide_answers", { sessionId, answers }),
  sshProvideSecret: (sessionId: string, secret: string | null, remember: boolean) =>
    invoke<void>("ssh_provide_secret", { sessionId, secret, remember }),
};

export { Channel };
