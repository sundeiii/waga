// Types mirrored from the Rust backend (serde camelCase).

export type AuthMethod = "password" | "key" | "agent";
export type ProfileKind = "linux" | "debian" | "windows" | "localVm" | "proxmox" | "custom";

export interface ServerInput {
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  keyPath: string | null;
  profileKind: ProfileKind;
  groupName: string | null;
  tags: string[];
  notes: string;
  favorite: boolean;
  autoReconnect: boolean;
  keepalive: boolean;
}

export interface ServerView extends ServerInput {
  id: string;
  osDetected: string | null;
  lastConnectedAt: string | null;
  createdAt: string;
  updatedAt: string;
  hasPassword: boolean;
  hasPassphrase: boolean;
}

/** Secret changes sent once to Rust; never stored in the frontend. */
export interface SecretUpdate {
  password?: string;
  passphrase?: string;
  clearPassword?: boolean;
  clearPassphrase?: boolean;
}

export interface AppSettings {
  appearance: {
    accent: string;
    windowMaterial: "acrylic" | "mica" | "none";
    windowTint: number;
    transparency: number;
    blur: number;
    animations: boolean;
    reducedMotion: boolean;
    sidebarCollapsed: boolean;
  };
  terminal: {
    fontFamily: string;
    fontSize: number;
    cursorStyle: "block" | "bar" | "underline";
    cursorBlink: boolean;
    theme: string;
    opacity: number;
    scrollback: number;
    copyOnSelect: boolean;
    rightClickPaste: boolean;
  };
  connections: {
    timeoutSecs: number;
    keepaliveSecs: number;
    defaultPort: number;
    defaultUsername: string;
  };
  monitoring: {
    enabled: boolean;
    intervalSecs: number;
  };
  system: {
    closeToTray: boolean;
    startWithWindows: boolean;
    startMinimized: boolean;
    notifications: boolean;
  };
}

export type SessionState =
  | "connecting"
  | "awaitingHostKey"
  | "authenticating"
  | "awaitingSecret"
  | "connected"
  | "disconnected"
  | "authFailed"
  | "timeout"
  | "hostKeyRejected"
  | "hostKeyMismatch"
  | "error";

export type HostKeyStatus = "trusted" | "unknown" | "changed";

export type SessionEvent =
  | { type: "status"; state: SessionState; message: string | null }
  | {
      type: "hostKeyPrompt";
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      status: HostKeyStatus;
      previous: { algorithm: string; fingerprint: string; addedAt: string }[];
    }
  | { type: "secretPrompt"; kind: "password" | "passphrase"; prompt: string; attempt: number }
  | { type: "interactivePrompt"; name: string; instructions: string; prompts: { prompt: string; echo: boolean }[]; round: number };

export interface ActivityEntry {
  id: number;
  ts: string;
  serverId: string | null;
  serverName: string | null;
  kind: string;
  message: string;
}

export interface KnownHost {
  host: string;
  port: number;
  algorithm: string;
  publicKey: string;
  fingerprint: string;
  addedAt: string;
}

/** A local shell profile (from Windows Terminal settings or built-in). */
export interface LocalProfile {
  id: string;
  name: string;
  source: "windowsTerminal" | "builtin";
  program: string;
  isDefault: boolean;
  theme: Record<string, string> | null;
  colorScheme: string | null;
  fontFace: string | null;
  fontSizePx: number | null;
  opacity: number | null;
}

export const SNIPPET_CATEGORIES = ["linux", "networking", "docker", "git", "diagnostics", "powershell", "security", "custom"] as const;
export type SnippetCategory = (typeof SNIPPET_CATEGORIES)[number];

export interface SnippetInput {
  title: string;
  description: string;
  command: string;
  category: SnippetCategory;
  tags: string[];
  favorite: boolean;
}

export interface Snippet extends SnippetInput {
  id: string;
  useCount: number;
  lastUsedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface FileEntry {
  name: string;
  path: string;
  kind: "dir" | "file" | "symlink" | "other";
  isDir: boolean;
  size: number | null;
  modified: number | null;
  permissions: string | null;
  hidden: boolean;
}

export interface Listing {
  path: string;
  parent: string | null;
  entries: FileEntry[];
  truncated: boolean;
  total: number;
}

export interface TransferEvent {
  id: string;
  serverId: string;
  direction: "upload" | "download";
  name: string;
  source: string;
  dest: string;
  bytes: number;
  total: number | null;
  state: "running" | "done" | "failed" | "cancelled";
  error: string | null;
}

export interface DiskUsage {
  mount: string;
  totalBytes: number;
  usedBytes: number;
}

/** One real sample; null = not available (never estimated). */
export interface MetricsSample {
  serverId: string;
  ts: string;
  cpuPercent: number | null;
  cpuCores: number | null;
  memTotalBytes: number | null;
  memUsedBytes: number | null;
  disks: DiskUsage[];
  netRxBps: number | null;
  netTxBps: number | null;
  load: [number, number, number] | null;
  uptimeSecs: number | null;
  hostname: string | null;
  os: string | null;
}

export type MonitorState = "running" | "noConnection" | "error" | "stopped";

export interface MonitorStatus {
  serverId: string;
  state: MonitorState;
  message: string | null;
  ts: string;
}

export interface AppError {
  kind: string;
  message: string;
}

export type Page =
  | "overview"
  | "servers"
  | "terminal"
  | "sftp"
  | "monitoring"
  | "snippets"
  | "activity"
  | "settings";
