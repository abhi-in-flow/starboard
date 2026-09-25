export type ListGenerationInput = {
  query: string;
  language: string | null;
  topic: string | null;
  categoryId: number | null;
  reviewPreset: string | null;
  hideUnstarred: boolean;
  hideArchived: boolean;
  sort: string;
  sortDesc: boolean;
  searchMode: string;
};

export function serializeListGeneration(input: ListGenerationInput): string {
  return [
    input.query,
    input.language ?? "",
    input.topic ?? "",
    input.categoryId == null ? "" : String(input.categoryId),
    input.reviewPreset ?? "",
    input.hideUnstarred ? "1" : "0",
    input.hideArchived ? "1" : "0",
    input.sort,
    input.sortDesc ? "1" : "0",
    input.searchMode,
  ].join("\u001f");
}

export type PendingAdvance = {
  generation: string;
  sourceId: number;
};

export function createPendingAdvance(input: {
  generation: string;
  sourceId: number;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchFailed: boolean;
}): PendingAdvance | null {
  if (
    !input.hasNextPage ||
    input.isFetchingNextPage ||
    input.fetchFailed ||
    input.sourceId <= 0
  ) {
    return null;
  }
  return { generation: input.generation, sourceId: input.sourceId };
}

export type PendingAdvanceResult =
  | { kind: "keep" }
  | { kind: "clear" }
  | { kind: "select"; id: number };

export function resolvePendingAdvance(input: {
  pending: PendingAdvance | null;
  generation: string;
  selectedId: number | null;
  itemIds: number[];
  fetchFailed: boolean;
  fetchSettled: boolean;
}): PendingAdvanceResult {
  const pending = input.pending;
  if (pending == null) {
    return { kind: "keep" };
  }
  if (input.generation !== pending.generation) {
    return { kind: "clear" };
  }
  if (input.selectedId !== pending.sourceId) {
    return { kind: "clear" };
  }
  if (input.fetchFailed) {
    return { kind: "clear" };
  }
  const idx = input.itemIds.indexOf(pending.sourceId);
  if (idx < 0) {
    return { kind: "clear" };
  }
  if (idx < input.itemIds.length - 1) {
    return { kind: "select", id: input.itemIds[idx + 1] };
  }
  if (input.fetchSettled) {
    return { kind: "clear" };
  }
  return { kind: "keep" };
}
