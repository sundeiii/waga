import { Area, AreaChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { ReactNode } from "react";
import { Panel } from "@/components/ui/misc";
import { formatBytes } from "@/stores/monitoring";

/** Validated (dark surface #12151D): lightness band, chroma, CVD ΔE 13.6, contrast ≥ 3:1. */
export const SERIES = ["#8A7AE8", "#2FA69E"] as const;

const AXIS = { stroke: "var(--chart-axis)", tick: { fill: "var(--faint)", fontSize: 11 }, tickLine: false } as const;

export const timeLabel = (iso: string) => new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });

function TooltipBox({ label, rows }: { label: string; rows: { name: string; color: string; value: string }[] }) {
  return (
    <div className="glass-elevated rounded-lg px-3 py-2 text-[12px]">
      <div className="mb-1 font-mono text-[11px] text-faint">{label}</div>
      {rows.map((r) => (
        <div key={r.name} className="flex items-center gap-2">
          <span className="size-2 rounded-full" style={{ background: r.color }} />
          <span className="text-muted">{r.name}</span>
          <span className="ml-auto pl-4 font-mono tabular-nums text-fg">{r.value}</span>
        </div>
      ))}
    </div>
  );
}

export function ChartCard({ title, value, unit, children, footer }: { title: string; value: ReactNode; unit?: string; children: ReactNode; footer?: ReactNode }) {
  return (
    <Panel className="flex flex-col p-4">
      <div className="flex items-baseline justify-between">
        <h3 className="text-[12px] font-medium uppercase tracking-[0.08em] text-faint">{title}</h3>
        <div className="font-mono text-[20px] font-semibold tabular-nums tracking-tight">
          {value}
          {unit && <span className="ml-1 text-[12px] font-normal text-muted">{unit}</span>}
        </div>
      </div>
      <div className="mt-3 h-40">{children}</div>
      {footer && <div className="mt-2 text-[11.5px] text-faint">{footer}</div>}
    </Panel>
  );
}

export function NoData({ children }: { children: ReactNode }) {
  return <div className="grid h-full place-items-center rounded-xl border border-dashed border-line text-center text-[12px] text-faint">{children}</div>;
}

/** Single-series percentage chart, fixed 0–100 axis. */
export function PercentChart({ data, name }: { data: { ts: string; v: number | null }[]; name: string }) {
  const id = `g-${name.replace(/\W/g, "")}`;
  return (
    <ResponsiveContainer width="100%" height="100%">
      <AreaChart data={data} margin={{ top: 4, right: 4, bottom: 0, left: -18 }}>
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor={SERIES[0]} stopOpacity={0.35} />
            <stop offset="1" stopColor={SERIES[0]} stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid vertical={false} stroke="var(--chart-grid)" />
        <XAxis dataKey="ts" tickFormatter={timeLabel} minTickGap={48} {...AXIS} />
        <YAxis domain={[0, 100]} ticks={[0, 50, 100]} unit="%" {...AXIS} />
        <Tooltip
          cursor={{ stroke: "var(--chart-axis)", strokeWidth: 1.5 }}
          content={({ active, payload, label }) =>
            active && payload?.length ? (
              <TooltipBox label={timeLabel(String(label))} rows={[{ name, color: SERIES[0], value: payload[0].value == null ? "n/a" : `${Number(payload[0].value).toFixed(1)} %` }]} />
            ) : null
          }
        />
        <Area type="monotone" dataKey="v" name={name} stroke={SERIES[0]} strokeWidth={2} fill={`url(#${id})`} connectNulls={false} isAnimationActive={false} dot={false} activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--color-surface)" }} />
      </AreaChart>
    </ResponsiveContainer>
  );
}

/** Network throughput: download vs upload on ONE bytes/s axis. */
export function NetworkChart({ data }: { data: { ts: string; rx: number | null; tx: number | null }[] }) {
  return (
    <ResponsiveContainer width="100%" height="100%">
      <LineChart data={data} margin={{ top: 4, right: 4, bottom: 0, left: 6 }}>
        <CartesianGrid vertical={false} stroke="var(--chart-grid)" />
        <XAxis dataKey="ts" tickFormatter={timeLabel} minTickGap={48} {...AXIS} />
        <YAxis tickFormatter={(v) => formatBytes(v, 0)} width={56} {...AXIS} />
        <Tooltip
          cursor={{ stroke: "var(--chart-axis)", strokeWidth: 1.5 }}
          content={({ active, payload, label }) =>
            active && payload?.length ? (
              <TooltipBox
                label={timeLabel(String(label))}
                rows={payload.map((p, i) => ({ name: String(p.name), color: SERIES[i], value: p.value == null ? "n/a" : `${formatBytes(Number(p.value))}/s` }))}
              />
            ) : null
          }
        />
        <Line type="monotone" dataKey="rx" name="Download" stroke={SERIES[0]} strokeWidth={2} dot={false} isAnimationActive={false} connectNulls={false} activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--color-surface)" }} />
        <Line type="monotone" dataKey="tx" name="Upload" stroke={SERIES[1]} strokeWidth={2} dot={false} isAnimationActive={false} connectNulls={false} activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--color-surface)" }} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function Legend({ items }: { items: { label: string; value: string }[] }) {
  return (
    <div className="flex gap-4">
      {items.map((it, i) => (
        <span key={it.label} className="flex items-center gap-1.5">
          <span className="h-0.5 w-3 rounded-full" style={{ background: SERIES[i] }} />
          <span className="text-muted">{it.label}</span>
          <span className="font-mono tabular-nums text-fg">{it.value}</span>
        </span>
      ))}
    </div>
  );
}

export { Meter } from "@/components/ui/Meter";
