import { cn } from "@/lib/utils";

/**
 * Thin usage meter (disks, card mini-metrics). Kept separate from the charts
 * so server cards don't pull the charting library into the startup bundle.
 */
export function Meter({ percent, className, label }: { percent: number | null; className?: string; label?: string }) {
  const tone = percent == null ? "" : percent >= 90 ? "bg-danger" : percent >= 75 ? "bg-warning" : "bg-accent";
  return (
    <div
      className={cn("h-1.5 overflow-hidden rounded-full bg-white/[0.06]", className)}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent == null ? undefined : Math.round(percent)}
    >
      {percent != null && <div className={cn("h-full rounded-full transition-[width] duration-300", tone)} style={{ width: `${Math.max(2, Math.min(100, percent))}%` }} />}
    </div>
  );
}
