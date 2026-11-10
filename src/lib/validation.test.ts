import { describe, expect, it } from "vitest";
import { parseTags, validateHost, validatePort, validateUsername } from "./validation";
import { parseQuickConnect } from "./quickConnect";

describe("validateHost", () => {
  it.each(["10.0.0.1", "example.com", "web-01.prod.local", "::1", "[fe80::1]", "2001:db8::42", "my_host", "fqdn.example."])(
    "accepts %s",
    (h) => expect(validateHost(h)).toBeNull(),
  );
  it.each(["", "-oProxyCommand=x", "a b", "300.1.1.1", "bad..host", "host-.com", "foo;rm", "a/b"])("rejects %s", (h) =>
    expect(validateHost(h)).not.toBeNull(),
  );
});

describe("validateUsername / port / tags", () => {
  it("usernames", () => {
    expect(validateUsername("root")).toBeNull();
    expect(validateUsername("DOMAIN\\user")).toBeNull();
    expect(validateUsername("-l")).not.toBeNull();
    expect(validateUsername("")).not.toBeNull();
  });
  it("ports", () => {
    expect(validatePort(22)).toBeNull();
    expect(validatePort(0)).not.toBeNull();
    expect(validatePort(70000)).not.toBeNull();
    expect(validatePort(22.5)).not.toBeNull();
  });
  it("tags are trimmed, lowercased, deduped", () => {
    expect(parseTags(" Web, web ,, DB")).toEqual(["web", "db"]);
  });
});

describe("parseQuickConnect", () => {
  it("parses user@host:port", () => {
    expect(parseQuickConnect("root@10.0.0.2:2222", "", 22)).toEqual({ username: "root", host: "10.0.0.2", port: 2222 });
  });
  it("uses defaults", () => {
    expect(parseQuickConnect("example.com", "me", 22)).toEqual({ username: "me", host: "example.com", port: 22 });
  });
  it("handles IPv6", () => {
    expect(parseQuickConnect("u@[fe80::1]:2200", "", 22)).toEqual({ username: "u", host: "fe80::1", port: 2200 });
    expect(parseQuickConnect("u@2001:db8::1", "", 22)).toEqual({ username: "u", host: "2001:db8::1", port: 22 });
  });
  it("strips leading ssh", () => {
    expect(parseQuickConnect("ssh pi@raspberrypi.local", "", 22)).toEqual({ username: "pi", host: "raspberrypi.local", port: 22 });
  });
  it("rejects option injection", () => {
    expect(typeof parseQuickConnect("-oProxyCommand=calc", "u", 22)).toBe("string");
    expect(typeof parseQuickConnect("u@host:99999", "", 22)).toBe("string");
  });
});
