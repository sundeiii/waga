# yoru — SSH & Server Manager

A calm, local-first SSH client and server manager for Windows, built with
Tauri 2 (Rust) + React. Dark glassmorphism, lavender accents, real SSH.

> Status: **Phase 1–3 vertical slice.** Server profiles, secure credential
> storage, host-key verification and real interactive terminals work and are
> tested. SFTP, monitoring and snippets are visibly marked "not implemented yet".

## What works today

| Area | State |
| --- | --- |
| App shell, sidebar (collapsible), top bar, design system, settings persistence | ✅ |
| Server profiles in SQLite: add / edit / delete / duplicate / favorite / search / tag + group filters / sort | ✅ |
| Passwords & key passphrases in **Windows Credential Manager** (never in SQLite, never in localStorage) | ✅ |
| Host key verification: fingerprint prompt on first connect; **changed keys blocked** unless explicitly acknowledged | ✅ |
| Auth: password, private key (OpenSSH/PEM/PKCS#8, encrypted keys), ssh-agent (Windows OpenSSH agent or Pageant) | ✅ |
| Interactive terminal: xterm.js, remote PTY (`xterm-256color`), resize, UTF-8, ANSI, vim/htop/tmux, multiple tabs, search, clear, font size, reconnect, auto-reconnect | ✅ |
| Activity log (no secrets, no terminal contents), clearable | ✅ |
| Trusted host key manager (view / forget) | ✅ |
| Monitoring: CPU, memory, disks, network, uptime, load, OS — over the **already-open** SSH connection (no agent, no extra login); Linux via `/proc` + POSIX `df`, Windows via PowerShell/CIM; live charts, pause, interval, history of collected samples only | ✅ |
| SFTP dual-pane file manager over the open SSH session: browse, breadcrumbs, hidden files, mkdir, rename, delete (local → Recycle Bin), drag-and-drop transfers with progress + cancel, overwrite only after confirmation, atomic `.yoru-partial` writes | ✅ |
| Local terminals (cmd, Windows PowerShell, PowerShell 7, WSL) from your **Windows Terminal profiles**, incl. their color scheme and font, via ConPTY. Ctrl+Shift+T opens the default profile | ✅ |
| Command snippets: categories, search, tags, favorites, starter set of read-only diagnostics; **Insert** (default) never executes — multi-line inserts into shells without bracketed paste ask first; **Run** always confirms showing the exact command and target; quick picker with Ctrl+Shift+S | ✅ |
| Keyboard-interactive / **2FA** (verification codes, PAM): prompts shown as-is, hidden ones masked, answers never stored; saved password is pre-filled so only the code is typed; works after key/agent login too | ✅ |
| **Jump hosts** (ProxyJump): connect through other saved servers (≤ 3 hops) via direct-tcpip tunnels; host key + login verified per hop | ✅ |
| SFTP **folder transfers** (recursive, symlinks skipped, bounded), 32 requests in flight | ✅ |
| **System tray** (click to show/hide; Open / New local terminal / Quit), optional close-to-tray, start with Windows (optionally minimized), notifications for dropped sessions and finished transfers, single instance | ✅ |
| Settings export to a JSON file (preferences only) and in-app "delete all data" | ✅ |
| Local **port forwarding** (ssh -L): saved per server, auto-start, 127.0.0.1-only listeners, live stats | ✅ |
| **Host certificates**: trusted CAs with host patterns; strict checks (host type, principal, validity, signature) | ✅ |
| **Light theme** (dark / light / follow Windows), incl. title bar, Mica variant and auto terminal scheme | ✅ |
| **Encrypted backup / restore** (`.yorubackup`, Argon2id + XChaCha20-Poly1305): profiles, snippets, forwards, CAs, trusted host keys; saved passwords only if you opt in. Restore merges | ✅ |
| **Sync between PCs** through any shared folder (OneDrive, Dropbox, Syncthing, NAS): one encrypted file per device, newest edit wins, deletions carried over; passphrase kept in Credential Manager | ✅ |
| **Import from cloud**: DigitalOcean, Hetzner, Linode (read-only API token, optionally kept in Credential Manager); AWS and Azure through your signed-in `aws` / `az` CLI | ✅ |
| Remote / dynamic (SOCKS) forwarding | ⏳ not yet |

## Architecture

```
React (UI only)                       Rust (all privileged work)
──────────────────────────────        ─────────────────────────────────────────
pages/ components/ stores/   invoke   commands/      thin typed Tauri commands
services/api.ts  ─────────────────▶   database/      SQLite + migrations (no secrets)
services/terminalEngine.ts  ◀──────   credentials/   Windows Credential Manager (keyring)
  xterm instances live outside        security/      validation, host-key trust policy
  React; bytes go channel→xterm       ssh/           russh sessions, auth, prompts
                         IPC Channels  state.rs      AppState (db, secrets, sessions)
```

* **Streaming.** Each SSH session gets two ordered Tauri IPC `Channel`s: one for
  JSON lifecycle events, one for **raw terminal bytes**. Output is written
  straight into xterm.js without touching React state, so heavy output never
  re-renders the UI.
* **Prompts.** Host-key confirmation and password/passphrase entry happen
  *inside* the SSH handshake. The Rust task parks on a one-shot channel,
  emits a prompt event, and resumes when the UI answers
  (`ssh_host_key_decision` / `ssh_provide_secret`). Prompts time out (5 min)
  as a rejection.
* **Secrets.** Typed once into a form, sent to Rust, stored via `keyring`
  (Windows Credential Manager, target `*.yoru`), wiped from memory with
  `zeroize`. The frontend only ever learns `hasPassword: true/false`.
* **Host keys.** Stored in SQLite `known_hosts` (`host`, `port`, key,
  SHA-256 fingerprint). Policy mirrors OpenSSH `StrictHostKeyChecking=ask`:
  unknown → ask; match → proceed; **any different key for a known host:port →
  CHANGED → blocked**, replaceable only after an explicit "I verified"
  acknowledgement, which the backend enforces (not just the UI).
* **Monitoring.** After a terminal session authenticates, its connection
  handle is registered; a per-server monitor task opens a short-lived `exec`
  channel on it each interval and runs one *constant* command (Linux: a
  `/bin/sh -c` script reading `/proc`; Windows: PowerShell via
  `-EncodedCommand`). No user input is ever interpolated. CPU % and network
  rates come from deltas between two real samples, so the first sample shows
  them as unavailable. If no session is open the monitor waits, then stops
  itself; it never connects on its own. Remote strings are stripped of
  control characters. History lives in memory only (last 360 samples).
* **Least privilege.** The webview gets `core:default`, `dialog:allow-open`
  (key file picker) and `dialog:allow-save` (settings export) only. Tray,
  notifications and autostart are driven from Rust. No shell, fs, or http plugins.
  Strict CSP. No network listeners. No telemetry, no cloud.

### Tradeoffs

* **russh (pure Rust) vs libssh2/OpenSSH.** russh is async, maintained, has no
  C dependency and supports modern algorithms (curve25519, ed25519, chacha20,
  AES-GCM, ML-KEM hybrid) with insecure ones disabled by default. Tradeoff:
  it doesn't read `~/.ssh/config` or `~/.ssh/known_hosts`; yoru keeps its own
  trusted-host store. Crypto backend is `ring` so building needs only MSVC
  (no CMake/NASM).
* **Native title bar.** Kept the native Windows title bar (dark theme) instead
  of a custom one, so Snap Layouts, drag, resize, and accessibility stay
  reliable.
* **Quick Connect creates a profile.** A profile is required to track the host
  key and store credentials safely, so Quick Connect matches a saved server
  or opens a prefilled form.
* **Certificates.** OpenSSH host certificates are verified as plain keys; CA
  trust isn't implemented yet.
* **Links in output.** Remote output is untrusted, so Ctrl+click on a URL
  copies it instead of opening a browser.

## Prerequisites (Windows 10/11)

1. **Microsoft C++ Build Tools** (VS 2022 Build Tools, "Desktop development with C++")
   ```
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
2. **Rust** (stable, MSVC toolchain): `winget install Rustlang.Rustup` then `rustup default stable`
3. **Node.js LTS**: `winget install OpenJS.NodeJS.LTS`
4. **WebView2** — preinstalled on Windows 11 and current Windows 10.

Open a new terminal afterwards so PATH updates apply.

## Develop

```
npm install
npm run tauri dev
```

## Test

```
cd src-tauri
cargo test                                   # unit + integration tests (incl. real ConPTY cmd.exe)
cargo test --lib keyring_roundtrip -- --ignored   # real Credential Manager round-trip
cargo test --test real_sshd -- --ignored     # real OpenSSH: host key, key auth, shell, monitoring
cargo test --test real_sftp -- --ignored     # real OpenSSH: SFTP browse/upload/download/cancel/delete
cd ..
npm test                                     # frontend validation tests
npx tsc --noEmit -p .                        # typecheck
```

* **Unit tests** (Rust): profile persistence, migrations, settings validation,
  input validation, host-key trust decisions, credential deletion, prompt
  registry. Credential tests use an in-memory store; one `#[ignore]` test hits
  the real Credential Manager.
* **Integration tests** (`src-tauri/tests/ssh_integration.rs`): real TCP to an
  in-process **russh SSH server** on 127.0.0.1. Covers unknown-key prompt →
  trust, trusted reconnect without prompt, changed key blocked unless
  acknowledged, rejection/timeout of prompts, password retry + remember,
  cancelled auth, private-key auth (accept/reject), PTY request, UTF-8 echo,
  window resize, remote exit, connection refused, handshake timeout, and
  monitoring over the session's connection (sample parsing, CPU delta,
  OS persisted, auto-stop after disconnect).
* **Parser unit tests**: Linux `/proc`/`df`/`net/dev` (incl. CRLF, busybox-ish
  missing sections, bind-mount de-dup, control-char stripping) and Windows
  PowerShell JSON (single-object arrays, null CPU, CLIXML prefix).
* Not yet covered: a real OpenSSH server (Linux / Windows), ssh-agent auth,
  encrypted key files end-to-end, and UI tests.

## Build the installer

```
npm run tauri build
```

Outputs:

* `src-tauri/target/release/bundle/nsis/Yoru_0.1.0_x64-setup.exe` (per-user installer)
* `src-tauri/target/release/bundle/msi/Yoru_0.1.0_x64_en-US.msi`

The installer is unsigned. Windows SmartScreen will warn on first run; for
wider distribution, sign it with a code-signing certificate
(`bundle.windows.certificateThumbprint` in `tauri.conf.json`).

## Where data lives

* Profiles, settings, trusted host keys, activity: `%APPDATA%\dev.yoru.app\yoru.db`
* Secrets: Windows Credential Manager → *Windows Credentials* → entries for service `yoru`
* Sync files: `<your folder>yoru-sync<device-id>.yorusync` (encrypted; nothing readable without the passphrase)

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| Ctrl+K | Focus search |
| Ctrl+Shift+T | New local terminal (default Windows Terminal profile) |
| Ctrl+Shift+S | Snippet picker (Enter inserts, Ctrl+Enter runs after confirmation) |
| Ctrl+N | New server |
| Ctrl+Shift+Q | Quick connect |
| Ctrl+B | Toggle sidebar |
| Ctrl+Shift+1…8 | Switch page |
| Ctrl+, | Settings |
| Ctrl+Shift+C / Ctrl+Shift+V | Copy / paste in terminal |
| Right-click | Copy selection, or paste (configurable) |
| Ctrl+Shift+F | Find in terminal output |
| Middle-click tab | Close tab |

Ctrl+C, Ctrl+D, Ctrl+Z, Ctrl+L etc. always go to the remote shell.

## Troubleshooting

* **`link.exe` not found / MSVC errors** — install the C++ Build Tools above, then open a *new* terminal.
* **`npm`/`cargo` not recognized** — open a new terminal after installing (PATH refresh).
* **"No SSH agent found"** — start the agent: `Get-Service ssh-agent | Set-Service -StartupType Manual; Start-Service ssh-agent`, then `ssh-add`.
* **Host key CHANGED warning** — confirm with the server admin or on the server console (`ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`). Only then tick the acknowledgement. To forget a key: Settings → Trusted host keys.
* **Server rejected the private key** — make sure the matching `.pub` is in `~/.ssh/authorized_keys` on the server with correct permissions (`chmod 600`).
* **Port 1420 in use** when running `tauri dev` — stop the other Vite instance.

## Project layout

```
src/                      React + TypeScript UI
  components/{ui,layout,servers,terminal}
  pages/                  Overview, Servers, Terminal, Activity, Settings, ComingSoon
  services/               api.ts (typed invoke), terminalEngine.ts (xterm + sessions)
  stores/                 zustand stores (settings, servers, sessions, ui)
  lib/                    validation, quick-connect parser, utils (+ tests)
  styles/globals.css      design tokens, glass utilities, ambient background
src-tauri/
  src/commands/           Tauri commands
  src/database/           SQLite access + migrations runner
  src/credentials/        OS secret store
  src/security/           validation, host-key policy
  src/ssh/                session lifecycle, auth, handler, prompts, events
  migrations/             SQL migrations
  capabilities/           Tauri permissions
  tests/                  SSH integration tests
```
