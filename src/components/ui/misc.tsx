import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { TabState } from "@/stores/sessions";

export function Panel({ className, children, ...p }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn("glass rounded-2xl", className)} {...p}>
      {children}
    </div>
  );
}

export function Badge({ children, tone = "neutral", className }: { children: ReactNode; tone?: "neutral" | "accent" | "success" | "warning" | "danger"; className?: string }) {
  const tones = {
    neutral: "bg-ink/[0.06] text-muted border-line",
    accent: "bg-accent-soft text-accent border-accent/25",
    success: "bg-success/12 text-success border-success/25",
    warning: "bg-warning/12 text-warning border-warning/25",
    danger: "bg-danger/12 text-danger border-danger/25",
  } as const;
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px] font-medium leading-none", tones[tone], className)}>
      {children}
    </span>
  );
}

const DOT = { neutral: "bg-faint", accent: "bg-accent", success: "bg-success", warning: "bg-warning", danger: "bg-danger" } as const;

/** Toggleable filter pill. The selected state is part of the chip itself (no outer ring). */
export function FilterChip({
  active,
  onClick,
  tone = "neutral",
  children,
}: {
  active: boolean;
  onClick: () => void;
  tone?: keyof typeof DOT;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-lg border px-2.5 text-[12px] font-medium transition-colors duration-150",
        active ? "border-accent/45 bg-accent-soft text-fg" : "border-line text-muted hover:border-line-strong hover:text-fg",
      )}
    >
      <span className={cn("size-1.5 rounded-full", DOT[tone])} />
      {children}
    </button>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="rounded-md border border-line bg-ink/5 px-1.5 py-0.5 font-mono text-[10.5px] text-muted">{children}</kbd>;
}

export const STATE_META: Record<TabState | "idle", { label: string; color: string; pulse?: boolean }> = {
  idle: { label: "Not connected", color: "bg-faint" },
  connecting: { label: "Connecting", color: "bg-warning", pulse: true },
  awaitingHostKey: { label: "Verify host key", color: "bg-warning", pulse: true },
  authenticating: { label: "Authenticating", color: "bg-warning", pulse: true },
  awaitingSecret: { label: "Authentication required", color: "bg-warning", pulse: true },
  connected: { label: "Connected", color: "bg-success" },
  reconnecting: { label: "Reconnecting", color: "bg-warning", pulse: true },
  disconnected: { label: "Disconnected", color: "bg-faint" },
  authFailed: { label: "Authentication failed", color: "bg-danger" },
  timeout: { label: "Timed out", color: "bg-danger" },
  hostKeyRejected: { label: "Host key not trusted", color: "bg-danger" },
  hostKeyMismatch: { label: "Host key CHANGED", color: "bg-danger" },
  error: { label: "Error", color: "bg-danger" },
};

export function StatusDot({ state, className }: { state: TabState | "idle"; className?: string }) {
  const m = STATE_META[state];
  return (
    <span className={cn("relative inline-flex size-2 shrink-0", className)} title={m.label} aria-label={m.label}>
      {m.pulse && <span className={cn("pulse-ring absolute inset-0 rounded-full", m.color)} />}
      <span className={cn("relative inline-flex size-2 rounded-full", m.color, state === "connected" && "shadow-[0_0_8px_var(--color-success)]")} />
    </span>
  );
}

export function EmptyState({ icon, title, body, action }: { icon: ReactNode; title: string; body: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
      <div className="mb-5 grid size-14 place-items-center rounded-2xl border border-line bg-ink/[0.04] text-accent shadow-[0_0_40px_-10px_var(--accent)] [&_svg]:size-6">
        {icon}
      </div>
      <h3 className="text-[15px] font-semibold tracking-tight">{title}</h3>
      <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-muted">{body}</p>
      {action && <div className="mt-6">{action}</div>}
    </div>
  );
}

export function SectionTitle({ children, right }: { children: ReactNode; right?: ReactNode }) {
  return (
    <div className="mb-3 flex items-center justify-between">
      <h2 className="text-[12px] font-semibold uppercase tracking-[0.08em] text-faint">{children}</h2>
      {right}
    </div>
  );
}
