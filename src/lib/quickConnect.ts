import { validateHost, validatePort, validateUsername } from "./validation";

export interface QuickTarget {
  username: string;
  host: string;
  port: number;
}

/**
 * Parse "user@host", "user@host:port", "user@[v6]:port" or "user@v6".
 * Returns an error string on invalid input.
 */
export function parseQuickConnect(input: string, defaultUser: string, defaultPort: number): QuickTarget | string {
  let s = input.trim();
  if (!s) return "Enter user@host";
  if (s.startsWith("ssh ")) s = s.slice(4).trim();

  let username = defaultUser;
  const at = s.lastIndexOf("@");
  if (at >= 0) {
    username = s.slice(0, at);
    s = s.slice(at + 1);
  }

  let host = s;
  let port = defaultPort;
  const bracket = /^\[([^\]]+)\](?::(\d+))?$/.exec(s);
  if (bracket) {
    host = bracket[1];
    if (bracket[2]) port = Number(bracket[2]);
  } else if ((s.match(/:/g) ?? []).length === 1) {
    const [h, p] = s.split(":");
    host = h;
    port = Number(p);
  }

  const e = validateUsername(username) ?? validateHost(host) ?? validatePort(port);
  if (e) return e;
  return { username: username.trim(), host, port };
}
