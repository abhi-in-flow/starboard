import { describe, expect, test } from "bun:test";
import {
  type EmptyStateInput,
  emptyStateCopy,
  selectEmptyState,
} from "./libraryEmpty";

function input(partial: Partial<EmptyStateInput> = {}): EmptyStateInput {
  return {
    total: 0,
    onboardingSurface: "hidden",
    githubConnected: true,
    lastSyncedAt: null,
    repoCount: 0,
    query: "",
    language: null,
    topic: null,
    categoryId: null,
    reviewPreset: null,
    ...partial,
  };
}

describe("selectEmptyState", () => {
  test("setup errors win over the full onboarding overlay", () => {
    expect(
      selectEmptyState(
        input({
          setupError: true,
          onboardingSurface: "full",
          githubConnected: false,
        }),
      ),
    ).toBe("setup-error");
  });

  test("onboarding owns no-PAT and never-synced zero-library surfaces", () => {
    expect(
      selectEmptyState(
        input({
          onboardingSurface: "full",
          githubConnected: false,
        }),
      ),
    ).toBe("onboarding");
    expect(
      selectEmptyState(
        input({
          onboardingSurface: "full",
          githubConnected: true,
          lastSyncedAt: null,
          repoCount: 0,
        }),
      ),
    ).toBe("onboarding");
  });

  test("distinguishes dismissed no-PAT, never synced, true empty, and filter miss", () => {
    expect(selectEmptyState(input({ githubConnected: false }))).toBe("no-pat");
    expect(selectEmptyState(input({ githubConnected: true }))).toBe(
      "never-synced",
    );
    expect(
      selectEmptyState(
        input({ lastSyncedAt: "2026-01-01T00:00:00Z", repoCount: 0 }),
      ),
    ).toBe("true-empty");
    expect(selectEmptyState(input({ query: "xyzzy" }))).toBe("filter-empty");
    expect(selectEmptyState(input({ language: "Rust" }))).toBe("filter-empty");
    expect(selectEmptyState(input({ total: 3 }))).toBe("has-results");
  });

  test("review queues: never synced, young library, caught up, filter miss", () => {
    expect(
      selectEmptyState(input({ reviewPreset: "inactive", reviewCounts: null })),
    ).toBe("review-never-synced");
    expect(
      selectEmptyState(
        input({
          reviewPreset: "forgotten",
          reviewCounts: {
            activeStars: 12,
            oldestStarredAt: new Date().toISOString(),
            inactive: 0,
            forgotten: 0,
          },
        }),
      ),
    ).toBe("review-young");
    expect(
      selectEmptyState(
        input({
          reviewPreset: "inactive",
          language: "Go",
          reviewCounts: {
            activeStars: 40,
            oldestStarredAt: "2020-01-01T00:00:00Z",
            inactive: 4,
            forgotten: 1,
          },
        }),
      ),
    ).toBe("review-filter-empty");
    expect(
      selectEmptyState(
        input({
          reviewPreset: "uncategorized",
          reviewCounts: {
            activeStars: 40,
            oldestStarredAt: "2020-01-01T00:00:00Z",
            inactive: 4,
            forgotten: 1,
          },
        }),
      ),
    ).toBe("review-caught-up");
    expect(
      selectEmptyState(
        input({
          reviewPreset: "unstarred",
          reviewCounts: {
            activeStars: 10,
            oldestStarredAt: "2020-01-01T00:00:00Z",
            inactive: 1,
            forgotten: 0,
          },
        }),
      ),
    ).toBe("review-unstarred-empty");
  });
});

describe("emptyStateCopy", () => {
  test("filter-empty offers Clear filters; sync-empty offers Sync; true empty is honest", () => {
    expect(emptyStateCopy("filter-empty").primary).toBe("clear-filters");
    expect(emptyStateCopy("never-synced").primary).toBe("sync");
    expect(emptyStateCopy("true-empty").body).toContain("found no starred");
    expect(emptyStateCopy("setup-error").primary).toBe("retry");
    expect(emptyStateCopy("setup-error").secondary).toBe("settings");
  });
});
