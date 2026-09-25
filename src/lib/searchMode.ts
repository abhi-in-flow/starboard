import type { SearchMode } from "@/types";

const MODE_LABELS: Record<SearchMode, string> = {
  keyword: "Keyword",
  semantic: "Semantic",
  hybrid: "Hybrid",
};

export function searchModeLabel(mode: SearchMode): string {
  return MODE_LABELS[mode];
}

export type SearchModeFallback = {
  fallback: boolean;
  requested: SearchMode;
  used: SearchMode;
  requestedLabel: string;
  usedLabel: string;
  badge: string | null;
  title: string | null;
};

/** When the backend falls back (Hybrid → Keyword), surface the mode actually used. */
export function searchModeFallback(
  requested: SearchMode,
  modeUsed?: SearchMode | null,
): SearchModeFallback {
  const used = modeUsed ?? requested;
  const fallback = used !== requested;
  return {
    fallback,
    requested,
    used,
    requestedLabel: searchModeLabel(requested),
    usedLabel: searchModeLabel(used),
    badge: fallback ? `Using ${searchModeLabel(used)}` : null,
    title: fallback
      ? `${searchModeLabel(requested)} was requested; results used ${searchModeLabel(used)}.`
      : null,
  };
}

/** Semantic/Hybrid relevance sort only applies to the mode that actually ran. */
export function searchSortIsRelevance(
  query: string,
  requested: SearchMode,
  modeUsed?: SearchMode | null,
): boolean {
  if (query.trim().length === 0) {
    return false;
  }
  const used = modeUsed ?? requested;
  return used === "semantic" || used === "hybrid";
}
