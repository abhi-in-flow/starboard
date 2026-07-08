export function formatRelative(iso: string | null | undefined): string {
  if (!iso) {
    return "—";
  }
  const then = Date.parse(iso);
  if (Number.isNaN(then)) {
    return "—";
  }
  const seconds = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (seconds < 60) {
    return "just now";
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ago`;
  }
  if (seconds < 86400) {
    return `${Math.floor(seconds / 3600)}h ago`;
  }
  if (seconds < 86400 * 30) {
    return `${Math.floor(seconds / 86400)}d ago`;
  }
  if (seconds < 86400 * 365) {
    return `${Math.floor(seconds / (86400 * 30))}mo ago`;
  }
  return `${Math.floor(seconds / (86400 * 365))}y ago`;
}

export function formatCount(n: number | null | undefined): string {
  if (n == null) {
    return "—";
  }
  if (n >= 1_000_000) {
    return `${(n / 1_000_000).toFixed(1)}m`;
  }
  if (n >= 1_000) {
    return `${(n / 1_000).toFixed(1)}k`;
  }
  return String(n);
}

const LANGUAGE_COLORS: Record<string, string> = {
  Rust: "#dea584",
  TypeScript: "#3178c6",
  JavaScript: "#f1e05a",
  Python: "#3572A5",
  Go: "#00ADD8",
  Java: "#b07219",
  "C++": "#f34b7d",
  C: "#555555",
  Ruby: "#701516",
  Swift: "#F05138",
  Kotlin: "#A97BFF",
  Shell: "#89e051",
  HTML: "#e34c26",
  CSS: "#563d7c",
  Dart: "#00B4AB",
  PHP: "#4F5D95",
  Scala: "#c22d40",
  Lua: "#000080",
  Zig: "#ec915c",
};

export function languageColor(language: string | null | undefined): string {
  if (!language) {
    return "#94a3b8";
  }
  return LANGUAGE_COLORS[language] ?? "#64748b";
}
