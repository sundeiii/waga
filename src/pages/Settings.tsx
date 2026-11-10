import { useEffect, useState, type ReactNode } from "react";
import { Fingerprint, KeyRound, Monitor, Palette, Plug, RotateCcw, Shield, SquareTerminal, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input, Select } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { Panel } from "@/components/ui/misc";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useServers } from "@/stores/servers";
import { api, errorMessage } from "@/services/api";
import { THEMES } from "@/services/terminalEngine";
import { formatTimestamp } from "@/lib/utils";
import type { AppSettings, KnownHost } from "@/types";

export const ACCENTS = [
  { name: "Lavender", value: "#A99CF6" },
  { name: "Periwinkle", value: "#8B9CF6" },
  { name: "Moonlight", value: "#C9D2E8" },
  { name: "Mint", value: "#6FD9B4" },
  { name: "Sky", value: "#7CC4F8" },
  { name: "Rose", value: "#F09AB8" },
  { name: "Amber", value: "#F2B76B" },
];

function Section({ icon: Icon, title, children }: { icon: typeof Palette; title: string; children: ReactNode }) {
  return (
    <Panel className="p-5">
      <h2 className="mb-4 flex items-center gap-2 text-[14px] font-semibold tracking-tight">
        <Icon className="size-4 text-accent" /> {title}
      </h2>
      <div className="divide-y divide-line">{children}</div>
    </Panel>
  );
}

function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0">
        <div className="text-[13px]">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] text-faint">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function NumberInput({ value, min, max, onCommit, suffix }: { value: number; min: number; max: number; onCommit: (n: number) => void; suffix?: string }) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(v));
    if (Number.isFinite(n) && n >= min && n <= max) onCommit(n);
    else setV(String(value));
  };
  return (
    <div className="flex items-center gap-2">
      <Input type="number" value={v} min={min} max={max} onChange={(e) => setV(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} className="w-24 text-right font-mono" />
      {suffix && <span className="w-8 text-[12px] text-faint">{suffix}</span>}
    </div>
  );
}

export function SettingsPage({ visible }: { visible: boolean }) {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const reset = useSettings((s) => s.reset);
  const error = useSettings((s) => s.error);
  const askConfirm = useUi((s) => s.askConfirm);
  const toast = useUi((s) => s.toast);
  const refreshServers = useServers((s) => s.refresh);
  const [hosts, setHosts] = useState<KnownHost[]>([]);

  const loadHosts = () => api.listKnownHosts().then(setHosts).catch(() => undefined);
  useEffect(() => {
    if (visible) void loadHosts();
  }, [visible]);

  if (!settings) return null;
  const set = <K extends keyof AppSettings>(section: K, patch: Partial<AppSettings[K]>) => update((s) => ({ ...s, [section]: { ...s[section], ...patch } }));
  const a = settings.appearance;
  const t = settings.terminal;
  const c = settings.connections;

  const exportSettings = async () => {
    const blob = JSON.stringify({ app: "yoru", version: 1, settings }, null, 2);
    try {
      await navigator.clipboard.writeText(blob);
      toast("Settings JSON copied to clipboard (contains no secrets)", "success");
    } catch {
      toast("Clipboard is unavailable", "warning");
    }
  };

  return (
    <div className="mx-auto grid max-w-5xl grid-cols-1 gap-4 pb-6 xl:grid-cols-2">
      {error && <p className="col-span-full rounded-xl border border-danger/30 bg-danger/10 px-4 py-2 text-[13px] text-danger">{error}</p>}

      <Section icon={Palette} title="Appearance">
        <Row label="Theme" hint="Light mode is not available yet">
          <span className="text-[13px] text-muted">Dark</span>
        </Row>
        <Row label="Accent color">
          <div className="flex gap-1.5">
            {ACCENTS.map((x) => (
              <button
                key={x.value}
                title={x.name}
                aria-label={`Accent ${x.name}`}
                onClick={() => set("appearance", { accent: x.value })}
                className="size-6 rounded-full border-2 transition-transform hover:scale-110"
                style={{ background: x.value, borderColor: a.accent.toLowerCase() === x.value.toLowerCase() ? "#fff" : "transparent" }}
              />
            ))}
          </div>
        </Row>
        <Row label="Window material" hint="Windows 11 backdrop behind the app">
          <div className="w-40">
            <Select value={a.windowMaterial} onChange={(e) => set("appearance", { windowMaterial: e.target.value as AppSettings["appearance"]["windowMaterial"] })}>
              <option value="acrylic">Acrylic (see-through)</option>
              <option value="mica">Mica (wallpaper tint)</option>
              <option value="none">Solid</option>
            </Select>
          </div>
        </Row>
        <Row label="Window tint" hint={a.windowMaterial === "none" ? "Solid window" : `${Math.round(a.windowTint * 100)}% — lower shows more of what's behind`}>
          <input type="range" min={0} max={1} step={0.02} value={a.windowTint} disabled={a.windowMaterial === "none"} onChange={(e) => set("appearance", { windowTint: Number(e.target.value) })} className="w-40 accent-[var(--accent)] disabled:opacity-40" aria-label="Window tint" />
        </Row>
        <Row label="Panel opacity" hint={`${Math.round(a.transparency * 100)}%`}>
          <input type="range" min={0.3} max={1} step={0.02} value={a.transparency} onChange={(e) => set("appearance", { transparency: Number(e.target.value) })} className="w-40 accent-[var(--accent)]" aria-label="Panel opacity" />
        </Row>
        <Row label="Blur intensity" hint={`${a.blur}px`}>
          <input type="range" min={0} max={40} step={1} value={a.blur} onChange={(e) => set("appearance", { blur: Number(e.target.value) })} className="w-40 accent-[var(--accent)]" aria-label="Blur intensity" />
        </Row>
        <Row label="Animations">
          <Switch checked={a.animations} onCheckedChange={(v) => set("appearance", { animations: v })} label="Animations" />
        </Row>
        <Row label="Reduce motion" hint="Windows' “Animation effects” setting is also respected">
          <Switch checked={a.reducedMotion} onCheckedChange={(v) => set("appearance", { reducedMotion: v })} label="Reduce motion" />
        </Row>
        <Row label="Collapsed sidebar" hint="Ctrl+B">
          <Switch checked={a.sidebarCollapsed} onCheckedChange={(v) => set("appearance", { sidebarCollapsed: v })} label="Collapsed sidebar" />
        </Row>
      </Section>

      <Section icon={SquareTerminal} title="Terminal">
        <Row label="Font family">
          <div className="w-52">
            <Select value={t.fontFamily} onChange={(e) => set("terminal", { fontFamily: e.target.value })}>
              <option value="JetBrains Mono Variable">JetBrains Mono</option>
              <option value="Cascadia Code">Cascadia Code</option>
              <option value="Cascadia Mono">Cascadia Mono</option>
              <option value="Consolas">Consolas</option>
            </Select>
          </div>
        </Row>
        <Row label="Font size">
          <NumberInput value={t.fontSize} min={8} max={32} onCommit={(n) => set("terminal", { fontSize: n })} suffix="px" />
        </Row>
        <Row label="Color scheme">
          <div className="w-40">
            <Select value={t.theme} onChange={(e) => set("terminal", { theme: e.target.value })}>
              {Object.keys(THEMES).map((k) => (
                <option key={k} value={k}>
                  {k[0].toUpperCase() + k.slice(1)}
                </option>
              ))}
            </Select>
          </div>
        </Row>
        <Row label="Background opacity" hint={`${Math.round(t.opacity * 100)}% · local shells use their Windows Terminal opacity`}>
          <input type="range" min={0.3} max={1} step={0.01} value={t.opacity} onChange={(e) => set("terminal", { opacity: Number(e.target.value) })} className="w-40 accent-[var(--accent)]" aria-label="Terminal background opacity" />
        </Row>
        <Row label="Cursor style">
          <div className="w-40">
            <Select value={t.cursorStyle} onChange={(e) => set("terminal", { cursorStyle: e.target.value as AppSettings["terminal"]["cursorStyle"] })}>
              <option value="bar">Bar</option>
              <option value="block">Block</option>
              <option value="underline">Underline</option>
            </Select>
          </div>
        </Row>
        <Row label="Cursor blinking">
          <Switch checked={t.cursorBlink} onCheckedChange={(v) => set("terminal", { cursorBlink: v })} label="Cursor blinking" />
        </Row>
        <Row label="Scrollback" hint="Applies to all open terminals">
          <NumberInput value={t.scrollback} min={100} max={100000} onCommit={(n) => set("terminal", { scrollback: n })} suffix="lines" />
        </Row>
        <Row label="Copy on select">
          <Switch checked={t.copyOnSelect} onCheckedChange={(v) => set("terminal", { copyOnSelect: v })} label="Copy on select" />
        </Row>
        <Row label="Right-click copies / pastes" hint="Like Windows Terminal">
          <Switch checked={t.rightClickPaste} onCheckedChange={(v) => set("terminal", { rightClickPaste: v })} label="Right-click paste" />
        </Row>
      </Section>

      <Section icon={Plug} title="Connections">
        <Row label="Connection timeout">
          <NumberInput value={c.timeoutSecs} min={3} max={120} onCommit={(n) => set("connections", { timeoutSecs: n })} suffix="s" />
        </Row>
        <Row label="Keepalive interval" hint="0 disables keepalive">
          <NumberInput value={c.keepaliveSecs} min={0} max={600} onCommit={(n) => set("connections", { keepaliveSecs: n })} suffix="s" />
        </Row>
        <Row label="Default SSH port">
          <NumberInput value={c.defaultPort} min={1} max={65535} onCommit={(n) => set("connections", { defaultPort: n })} />
        </Row>
        <Row label="Default username">
          <Input value={c.defaultUsername} onChange={(e) => set("connections", { defaultUsername: e.target.value })} className="w-40 font-mono" />
        </Row>
        <Row label="Reconnect behavior" hint="Configured per server (Auto-reconnect), up to 5 attempts with backoff">
          <span className="text-[12px] text-faint">Per server</span>
        </Row>
      </Section>

      <Section icon={Monitor} title="Monitoring">
        <Row label="Monitoring" hint="Starts automatically when a terminal session connects; reuses that SSH connection">
          <Switch checked={settings.monitoring.enabled} onCheckedChange={(v) => set("monitoring", { enabled: v })} label="Monitoring enabled" />
        </Row>
        <Row label="Polling interval">
          <NumberInput value={settings.monitoring.intervalSecs} min={1} max={300} onCommit={(n) => set("monitoring", { intervalSecs: n })} suffix="s" />
        </Row>
      </Section>

      <Section icon={Fingerprint} title="Trusted host keys">
        {hosts.length === 0 ? (
          <p className="text-[12.5px] text-faint">No trusted host keys yet. They are added when you accept a server's fingerprint.</p>
        ) : (
          hosts.map((h) => (
            <div key={`${h.host}:${h.port}:${h.publicKey}`} className="flex items-center gap-3 py-2.5 first:pt-0">
              <div className="min-w-0 flex-1">
                <div className="font-mono text-[12.5px]">
                  {h.host}:{h.port}
                </div>
                <div className="selectable truncate font-mono text-[11.5px] text-faint">
                  {h.algorithm} · {h.fingerprint}
                </div>
                <div className="text-[11px] text-faint">Trusted {formatTimestamp(h.addedAt)}</div>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() =>
                  askConfirm({
                    title: `Forget host key for ${h.host}:${h.port}?`,
                    body: "You'll be asked to verify the fingerprint again on the next connection.",
                    confirmLabel: "Forget",
                    danger: true,
                    onConfirm: async () => {
                      await api.removeKnownHost(h.host, h.port);
                      await loadHosts();
                    },
                  })
                }
              >
                Forget
              </Button>
            </div>
          ))
        )}
      </Section>

      <Section icon={Shield} title="Privacy & data">
        <Row label="Stored credentials" hint="Passwords and key passphrases in Windows Credential Manager">
          <Button
            variant="danger"
            size="sm"
            onClick={() =>
              askConfirm({
                title: "Delete all stored credentials?",
                body: "Removes every Yoru password and key passphrase from Windows Credential Manager. Server profiles are kept.",
                confirmLabel: "Delete credentials",
                danger: true,
                onConfirm: async () => {
                  const n = await api.deleteAllCredentials();
                  await refreshServers();
                  toast(`Credentials removed for ${n} servers`, "success");
                },
              })
            }
          >
            <KeyRound /> Delete all
          </Button>
        </Row>
        <Row label="Activity history">
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              askConfirm({
                title: "Clear activity history?",
                body: "Removes local connection history.",
                confirmLabel: "Clear",
                danger: true,
                onConfirm: async () => {
                  await api.clearActivity();
                  toast("Activity cleared", "success");
                },
              })
            }
          >
            <Trash2 /> Clear
          </Button>
        </Row>
        <Row label="Export settings" hint="Non-sensitive preferences only">
          <Button variant="secondary" size="sm" onClick={exportSettings}>
            Copy JSON
          </Button>
        </Row>
        <Row label="Reset preferences" hint="Restores default appearance, terminal and connection settings">
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              reset()
                .then(() => toast("Settings reset", "success"))
                .catch((e) => toast(errorMessage(e), "danger"))
            }
          >
            <RotateCcw /> Reset
          </Button>
        </Row>
        <Row label="Delete all application data" hint="Not available in-app yet. Uninstall, then remove %APPDATA%\dev.yoru.app">
          <span className="text-[12px] text-faint">Manual</span>
        </Row>
      </Section>
    </div>
  );
}
