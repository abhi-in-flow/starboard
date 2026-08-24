export const ESCAPE_ORDER = [
  "Close the repository detail panel",
  "Clear the search query",
  "Clear all filters and review",
] as const;

export type EscapeAction =
  | "close-detail"
  | "clear-query"
  | "clear-filters"
  | "none";

export function nextEscapeAction(input: {
  detailOpen: boolean;
  hasQuery: boolean;
  hasFiltersOrReview: boolean;
}): EscapeAction {
  if (input.detailOpen) {
    return "close-detail";
  }
  if (input.hasQuery) {
    return "clear-query";
  }
  if (input.hasFiltersOrReview) {
    return "clear-filters";
  }
  return "none";
}

export const KEYBOARD_HELP =
  "/ focus search · ↑/↓ move selection · Esc closes detail, then search, then filters and review";
