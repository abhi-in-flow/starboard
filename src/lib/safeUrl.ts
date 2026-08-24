/** Allow http/https, in-page hashes, and safe relative paths. Block dangerous schemes. */
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
  // Protocol-relative is not a known safe origin in the desktop shell.
  if (trimmed.startsWith("//")) {
    return false;
  }
  if (trimmed.startsWith("/")) {
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
  if (scheme !== "http" && scheme !== "https") {
    return false;
  }
  const rest = trimmed.slice(colon + 1);
  if (rest.toLowerCase().startsWith("//javascript")) {
    return false;
  }
  return !trimmed.toLowerCase().includes("javascript:");
}

export function safeMarkdownUrl(raw: string | undefined | null): string | null {
  return isSafeMarkdownUrl(raw) ? (raw ?? "").trim() : null;
}
