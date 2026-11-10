// Mirrors src-tauri/src/security/validation.rs for instant form feedback.
// The Rust side is authoritative and re-validates everything.

const IPV4 = /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}$/;

function isIpv6(s: string): boolean {
  if (!s.includes(":")) return false;
  try {
    // URL parser validates IPv6 literals.
    new URL(`http://[${s}]/`);
    return true;
  } catch {
    return false;
  }
}

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

export function validateHost(raw: string): string | null {
  const h = raw.trim();
  if (!h) return "Hostname is required";
  if (h.length > 253) return "Hostname is too long";
  if (h.startsWith("-")) return "Hostname cannot start with '-'";
  if (CONTROL.test(h) || /\s/.test(h)) return "Hostname cannot contain spaces or control characters";
  const unbracketed = h.startsWith("[") && h.endsWith("]") ? h.slice(1, -1) : h;
  if (isIpv6(unbracketed)) return null;
  if (IPV4.test(h)) return null;
  if (/^[\d.]+$/.test(h)) return "Invalid IPv4 address";
  const name = h.endsWith(".") ? h.slice(0, -1) : h;
  for (const label of name.split(".")) {
    if (!label || label.length > 63) return "Invalid hostname";
    if (label.startsWith("-") || label.endsWith("-")) return "Hostname labels cannot start or end with '-'";
    if (!/^[A-Za-z0-9_-]+$/.test(label)) return "Hostname may contain only letters, digits, '-', '_' and '.'";
  }
  return null;
}

export function validateUsername(raw: string): string | null {
  const u = raw.trim();
  if (!u) return "Username is required";
  if (u.length > 256) return "Username is too long";
  if (u.startsWith("-")) return "Username cannot start with '-'";
  if (CONTROL.test(u)) return "Username cannot contain control characters";
  return null;
}

export function validatePort(port: number): string | null {
  if (!Number.isInteger(port) || port < 1 || port > 65535) return "Port must be between 1 and 65535";
  return null;
}

export function parseTags(raw: string): string[] {
  const out: string[] = [];
  for (const t of raw.split(",")) {
    const v = t.trim().toLowerCase();
    if (v && !out.includes(v)) out.push(v);
  }
  return out;
}
