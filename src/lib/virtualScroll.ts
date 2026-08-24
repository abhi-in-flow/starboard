export type ScrollSelectionKey = {
  selectedId: number | null;
  columns: number;
};

/** Scroll only when selection or column count changes — never on items identity. */
export function shouldScrollSelectedRow(
  prev: ScrollSelectionKey,
  next: ScrollSelectionKey,
): boolean {
  if (next.selectedId == null) {
    return false;
  }
  return prev.selectedId !== next.selectedId || prev.columns !== next.columns;
}

export type EndReachedTrigger = {
  index: number;
  rowCount: number;
};

export function shouldFetchNextPage(input: {
  lastIndex: number | null;
  rowCount: number;
  lastTriggered: EndReachedTrigger | null;
  hasNextPage: boolean;
  isFetching: boolean;
  fetchFailed: boolean;
}): boolean {
  if (input.lastIndex == null || input.rowCount <= 0) {
    return false;
  }
  if (!input.hasNextPage || input.isFetching || input.fetchFailed) {
    return false;
  }
  if (input.lastIndex < input.rowCount - 2) {
    return false;
  }
  if (
    input.lastTriggered &&
    input.lastTriggered.index === input.lastIndex &&
    input.lastTriggered.rowCount === input.rowCount
  ) {
    return false;
  }
  return true;
}
