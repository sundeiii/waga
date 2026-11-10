import type { Snippet, SnippetCategory } from "@/types";

export const CATEGORY_LABEL: Record<SnippetCategory, string> = {
  linux: "Linux administration",
  networking: "Networking",
  docker: "Docker",
  git: "Git",
  diagnostics: "System diagnostics",
  powershell: "Windows PowerShell",
  security: "Security diagnostics",
  custom: "Custom commands",
};

/** All search words must appear in title, description, command, tags or category. */
export function matchesSnippet(s: Pick<Snippet, "title" | "description" | "command" | "tags" | "category">, q: string) {
  if (!q.trim()) return true;
  const hay = `${s.title} ${s.description} ${s.command} ${s.tags.join(" ")} ${CATEGORY_LABEL[s.category]}`.toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}
