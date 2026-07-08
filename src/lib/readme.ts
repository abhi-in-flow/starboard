/** Strip HTML tags (and dangling truncated tags) while keeping text. */
export function stripHtmlKeepText(input: string): string {
  const withoutComplete = input.replace(/<[^>]+>/gis, "");
  const withoutDangling = withoutComplete.replace(/<[^>]*$/is, "");
  return withoutDangling
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&apos;/g, "'")
    .replace(/&nbsp;/g, " ");
}

/** Prep already-synced excerpts that still contain raw HTML / badge chrome. */
export function prepareReadmeExcerpt(markdown: string): string {
  return markdown
    .split("\n")
    .filter((line) => {
      const lower = line.trim().toLowerCase();
      if (!lower) return false;
      if (
        lower.startsWith("<img") ||
        lower.startsWith("<picture") ||
        lower.startsWith("<p align") ||
        lower.startsWith("<div align") ||
        lower.startsWith("<br")
      ) {
        return false;
      }
      if (lower.includes("shields.io")) return false;
      if (
        lower.includes("![") &&
        lower.includes("](") &&
        lower.includes("badge")
      ) {
        return false;
      }
      return true;
    })
    .map((line) => stripHtmlKeepText(line))
    .filter((line) => line.trim().length > 0)
    .join("\n");
}
