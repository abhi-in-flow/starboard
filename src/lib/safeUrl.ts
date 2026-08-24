/** Allow http/https (and in-page hashes / relative paths). Block javascript/data/file. */
export function isSafeMarkdownUrl(raw: string | undefined | null): boolean {
  if (!raw) {
    return false;
  }
  const trimmed = raw.trim();
  if (!trimmed) {
    return false;
  }
  if (trimmed.startsWith("#") && !trimmed.includes(":")) {
    return true;
  }
  if (trimmed.startsWith("/") && !trimmed.startsWith("//")) {
    return true;
  }
  const colon = trimmed.indexOf(":");
  if (colon === -1) {
    return !trimmed.includes("\\") && !trimmed.includes("..");
  }
  const scheme = trimmed.slice(0, colon).toLowerCase();
  if (
    scheme === "javascript" ||
    scheme === "data" ||
    scheme === "file" ||
    scheme === "vbscript"
  ) {
    return false;
  }
  return scheme === "http" || scheme === "https";
}

export function safeMarkdownUrl(raw: string | undefined | null): string | null {
  return isSafeMarkdownUrl(raw) ? (raw ?? "").trim() : null;
}
