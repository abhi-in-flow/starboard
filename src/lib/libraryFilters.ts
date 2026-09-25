import { reviewPresetMeta } from "@/lib/review";
import type { RepoSort, ReviewPreset } from "@/types";

export type LibraryFilterState = {
  language: string | null;
  topic: string | null;
  categoryId: number | null;
  reviewPreset: ReviewPreset | null;
  hideUnstarred: boolean;
  hideArchived: boolean;
  sort: RepoSort;
  sortDesc: boolean;
};

export const DEFAULT_LIBRARY_FILTERS: LibraryFilterState = {
  language: null,
  topic: null,
  categoryId: null,
  reviewPreset: null,
  hideUnstarred: true,
  hideArchived: true,
  sort: "starredAt",
  sortDesc: true,
};

export type FilterChipKind =
  | "review"
  | "category"
  | "language"
  | "topic"
  | "visibility";

export type FilterChip = {
  kind: FilterChipKind;
  id: string;
  label: string;
  removable: boolean;
};

/** Defaults after leaving any review preset, including Unstarred. */
export function clearedLibraryFilters(): LibraryFilterState {
  return { ...DEFAULT_LIBRARY_FILTERS };
}

/**
 * Review presets own hide flags so list membership matches queue counts.
 * Inactive/Forgotten force hideArchived; leaving Unstarred restores hideUnstarred.
 */
export function applyReviewPreset(
  state: LibraryFilterState,
  preset: ReviewPreset | null,
): LibraryFilterState {
  if (preset == null) {
    return {
      ...state,
      reviewPreset: null,
      sort: "starredAt",
      sortDesc: true,
      hideUnstarred: true,
      hideArchived:
        state.reviewPreset === "unstarred" || state.reviewPreset === "archived"
          ? true
          : state.hideArchived,
    };
  }

  const base: LibraryFilterState = {
    ...state,
    reviewPreset: preset,
    sort: "stale",
    sortDesc: false,
  };

  if (preset === "archived") {
    return { ...base, hideArchived: false, hideUnstarred: true };
  }
  if (preset === "unstarred") {
    return { ...base, hideUnstarred: false, hideArchived: false };
  }
  if (preset === "uncategorized") {
    return {
      ...base,
      hideUnstarred: true,
      hideArchived: true,
      categoryId: null,
    };
  }
  // inactive + forgotten: hide archived so the list matches count_preset
  return { ...base, hideUnstarred: true, hideArchived: true };
}

export function applyHideUnstarred(
  state: LibraryFilterState,
  hideUnstarred: boolean,
): LibraryFilterState {
  if (hideUnstarred && state.reviewPreset === "unstarred") {
    return applyReviewPreset({ ...state, hideUnstarred }, null);
  }
  return { ...state, hideUnstarred };
}

/** Inactive/Forgotten keep hideArchived on so counts cannot diverge. */
export function hideArchivedDisabled(preset: ReviewPreset | null): boolean {
  return (
    preset === "inactive" ||
    preset === "forgotten" ||
    preset === "archived" ||
    preset === "unstarred"
  );
}

export function applyHideArchived(
  state: LibraryFilterState,
  hideArchived: boolean,
): LibraryFilterState {
  if (state.reviewPreset === "inactive" || state.reviewPreset === "forgotten") {
    return { ...state, hideArchived: true };
  }
  if (hideArchived && state.reviewPreset === "archived") {
    return applyReviewPreset({ ...state, hideArchived }, null);
  }
  return { ...state, hideArchived };
}

export function applyCategoryId(
  state: LibraryFilterState,
  categoryId: number | null,
): LibraryFilterState {
  return {
    ...state,
    categoryId,
    reviewPreset:
      categoryId != null && state.reviewPreset === "uncategorized"
        ? null
        : state.reviewPreset,
  };
}

export function hasFacetFilters(state: {
  language: string | null;
  topic: string | null;
  categoryId: number | null;
  query?: string;
}): boolean {
  return (
    state.language != null ||
    state.topic != null ||
    state.categoryId != null ||
    Boolean(state.query?.trim())
  );
}

export function hasClearableLibraryState(state: LibraryFilterState): boolean {
  return (
    state.reviewPreset != null ||
    state.language != null ||
    state.topic != null ||
    state.categoryId != null ||
    !state.hideUnstarred ||
    !state.hideArchived
  );
}

export function activeFilterChips(
  state: LibraryFilterState,
  categoryName?: string | null,
): FilterChip[] {
  const chips: FilterChip[] = [];
  if (state.reviewPreset) {
    chips.push({
      kind: "review",
      id: `review:${state.reviewPreset}`,
      label: reviewPresetMeta(state.reviewPreset).label,
      removable: true,
    });
  }
  if (state.categoryId != null) {
    chips.push({
      kind: "category",
      id: `category:${state.categoryId}`,
      label: categoryName?.trim() || `Category ${state.categoryId}`,
      removable: true,
    });
  }
  if (state.language) {
    chips.push({
      kind: "language",
      id: `language:${state.language}`,
      label: state.language,
      removable: true,
    });
  }
  if (state.topic) {
    chips.push({
      kind: "topic",
      id: `topic:${state.topic}`,
      label: state.topic,
      removable: true,
    });
  }
  if (!state.hideUnstarred && state.reviewPreset !== "unstarred") {
    chips.push({
      kind: "visibility",
      id: "visibility:unstarred",
      label: "Showing unstarred",
      removable: true,
    });
  }
  if (
    !state.hideArchived &&
    state.reviewPreset !== "archived" &&
    state.reviewPreset !== "unstarred"
  ) {
    chips.push({
      kind: "visibility",
      id: "visibility:archived",
      label: "Showing archived",
      removable: true,
    });
  }
  return chips;
}

export function removeFilterChip(
  state: LibraryFilterState,
  chip: FilterChip,
): LibraryFilterState {
  switch (chip.kind) {
    case "review":
      return applyReviewPreset(state, null);
    case "category":
      return applyCategoryId(state, null);
    case "language":
      return { ...state, language: null };
    case "topic":
      return { ...state, topic: null };
    case "visibility":
      if (chip.id === "visibility:unstarred") {
        return applyHideUnstarred(state, true);
      }
      return applyHideArchived(state, true);
  }
}
