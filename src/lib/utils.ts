import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function relativeTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "Never";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "Unknown";
  const s = Math.round((now - t) / 1000);
  if (s < 45) return "Just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 30) return `${d} d ago`;
  return new Date(t).toLocaleDateString();
}

export function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/** Format a host for display, bracketing IPv6 literals. */
export function displayEndpoint(user: string, host: string, port: number): string {
  const h = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return port === 22 ? `${user}@${h}` : `${user}@${h}:${port}`;
}
