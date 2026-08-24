import { describe, expect, test } from "bun:test";
import {
  activeFilterChips,
  applyCategoryId,
  applyHideArchived,
  applyHideUnstarred,
  applyReviewPreset,
  clearedLibraryFilters,
  DEFAULT_LIBRARY_FILTERS,
  hasClearableLibraryState,
  type LibraryFilterState,
  removeFilterChip,
} from "./libraryFilters";

function state(partial: Partial<LibraryFilterState> = {}): LibraryFilterState {
  return { ...DEFAULT_LIBRARY_FILTERS, ...partial };
}

describe("applyReviewPreset", () => {
  test("Inactive and Forgotten force hideArchived so counts match the list", () => {
    const open = state({ hideArchived: false, hideUnstarred: false });
    expect(applyReviewPreset(open, "inactive")).toMatchObject({
      reviewPreset: "inactive",
      hideArchived: true,
      hideUnstarred: true,
      sort: "stale",
      sortDesc: false,
    });
    expect(applyReviewPreset(open, "forgotten")).toMatchObject({
      reviewPreset: "forgotten",
      hideArchived: true,
      hideUnstarred: true,
    });
  });

  test("leaving Unstarred restores hideUnstarred and hideArchived", () => {
    const unstarred = applyReviewPreset(state(), "unstarred");
    expect(unstarred.hideUnstarred).toBe(false);
    expect(unstarred.hideArchived).toBe(false);
    expect(applyReviewPreset(unstarred, null)).toMatchObject({
      reviewPreset: null,
      hideUnstarred: true,
      hideArchived: true,
      sort: "starredAt",
      sortDesc: true,
    });
  });

  test("Uncategorized clears categoryId; Archived shows archived", () => {
    expect(
      applyReviewPreset(state({ categoryId: 4 }), "uncategorized").categoryId,
    ).toBeNull();
    expect(applyReviewPreset(state(), "archived")).toMatchObject({
      hideArchived: false,
      hideUnstarred: true,
    });
  });
});

describe("clear-all and chip visibility", () => {
  test("clear all resets review plus category/language/topic/visibility", () => {
    const dirty = state({
      reviewPreset: "inactive",
      language: "Rust",
      topic: "cli",
      categoryId: 9,
      hideUnstarred: false,
      hideArchived: false,
      sort: "stale",
      sortDesc: false,
    });
    expect(hasClearableLibraryState(dirty)).toBe(true);
    expect(clearedLibraryFilters()).toEqual(DEFAULT_LIBRARY_FILTERS);
  });

  test("review preset is a named removable chip; category uses the name not only the id", () => {
    const reviewing = applyReviewPreset(
      state({ categoryId: 3, language: "Go" }),
      "forgotten",
    );
    const chips = activeFilterChips(reviewing, "Developer Tools / CLIs");
    expect(chips.map((c) => c.kind)).toEqual([
      "review",
      "category",
      "language",
    ]);
    expect(chips[0].label).toBe("Possibly forgotten");
    expect(chips[1].label).toBe("Developer Tools / CLIs");
    expect(chips.every((c) => c.removable)).toBe(true);
  });

  test("removing the review chip independently keeps other facets", () => {
    const reviewing = applyReviewPreset(
      state({ language: "Rust", topic: "web" }),
      "inactive",
    );
    const chips = activeFilterChips(reviewing);
    const reviewChip = chips.find((c) => c.kind === "review");
    expect(reviewChip).toBeDefined();
    if (!reviewChip) {
      throw new Error("expected review chip");
    }
    const next = removeFilterChip(reviewing, reviewChip);
    expect(next.reviewPreset).toBeNull();
    expect(next.language).toBe("Rust");
    expect(next.topic).toBe("web");
    expect(next.hideUnstarred).toBe(true);
  });

  test("hiding unstarred while in Unstarred review leaves the preset", () => {
    const unstarred = applyReviewPreset(state(), "unstarred");
    const next = applyHideUnstarred(unstarred, true);
    expect(next.reviewPreset).toBeNull();
    expect(next.hideUnstarred).toBe(true);
  });

  test("hiding archived while in Archived review leaves the preset", () => {
    const archived = applyReviewPreset(state(), "archived");
    const next = applyHideArchived(archived, true);
    expect(next.reviewPreset).toBeNull();
    expect(next.hideArchived).toBe(true);
  });

  test("choosing a category leaves the Uncategorized preset", () => {
    const next = applyCategoryId(
      applyReviewPreset(state(), "uncategorized"),
      12,
    );
    expect(next.reviewPreset).toBeNull();
    expect(next.categoryId).toBe(12);
  });
});
