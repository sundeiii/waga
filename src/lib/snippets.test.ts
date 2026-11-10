import { describe, expect, it } from "vitest";
import { matchesSnippet } from "./snippets";

const s = { title: "Listening ports", description: "TCP/UDP sockets", command: "ss -tulpn", tags: ["ports"], category: "networking" as const };

describe("matchesSnippet", () => {
  it("matches every word across fields", () => {
    expect(matchesSnippet(s, "")).toBe(true);
    expect(matchesSnippet(s, "ss")).toBe(true);
    expect(matchesSnippet(s, "LISTENING tulpn")).toBe(true);
    expect(matchesSnippet(s, "networking ports")).toBe(true);
    expect(matchesSnippet(s, "ports docker")).toBe(false);
  });
});
