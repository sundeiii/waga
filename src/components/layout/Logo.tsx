import { cn } from "@/lib/utils";

/** yoru mark: a crescent moon with a terminal cursor. */
export function YoruMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={cn("size-7", className)} aria-hidden="true">
      <defs>
        <linearGradient id="yoru-moon" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#ECE8FF" />
          <stop offset="1" stopColor="var(--accent)" />
        </linearGradient>
        <mask id="yoru-crescent">
          <rect width="32" height="32" fill="#000" />
          <circle cx="15" cy="15" r="9.5" fill="#fff" />
          <circle cx="19" cy="11.8" r="8" fill="#000" />
        </mask>
      </defs>
      <rect width="32" height="32" fill="url(#yoru-moon)" mask="url(#yoru-crescent)" />
      <rect x="17.5" y="21.6" width="6.5" height="1.9" rx="0.95" fill="var(--accent)" />
    </svg>
  );
}

export function YoruWordmark({ collapsed }: { collapsed?: boolean }) {
  return (
    <div className="flex items-center gap-2.5">
      <div className="grid size-8 place-items-center rounded-[10px] border border-line bg-gradient-to-br from-[#1d1a33] to-[#0e1017] shadow-[0_0_24px_-8px_var(--accent)]">
        <YoruMark className="size-6" />
      </div>
      {!collapsed && (
        <div className="leading-none">
          <div className="text-[17px] font-semibold tracking-[-0.02em] lowercase">yoru</div>
          <div className="mt-1 text-[10.5px] tracking-wide text-faint">ssh & servers</div>
        </div>
      )}
    </div>
  );
}
