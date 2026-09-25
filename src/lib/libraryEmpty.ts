import { hasFacetFilters } from "@/lib/libraryFilters";
import { isYoungLibrary } from "@/lib/review";
import type { ReviewPreset } from "@/types";

export type EmptyStateKind =
  | "has-results"
  | "loading"
  | "repo-error"
  | "setup-error"
  | "onboarding"
  | "no-pat"
  | "never-synced"
  | "true-empty"
  | "filter-empty"
  | "review-never-synced"
  | "review-young"
  | "review-unstarred-empty"
  | "review-filter-empty"
  | "review-caught-up";

export type EmptyStateAction = "clear-filters" | "sync" | "retry" | "settings";

export type EmptyStateCopy = {
  title: string;
  body: string;
  primary: EmptyStateAction | null;
  secondary: EmptyStateAction | null;
};

export type EmptyStateInput = {
  total: number;
  loading?: boolean;
  repoError?: boolean;
  setupError?: boolean;
  onboardingSurface: "full" | "banner" | "hidden";
  githubConnected: boolean;
  lastSyncedAt: string | null;
  repoCount: number;
  query: string;
  language: string | null;
  topic: string | null;
  categoryId: number | null;
  reviewPreset: ReviewPreset | null;
  reviewCounts?: {
    activeStars: number;
    oldestStarredAt: string | null;
    inactive: number;
    forgotten: number;
  } | null;
};

export function selectEmptyState(input: EmptyStateInput): EmptyStateKind {
  if (input.setupError) {
    return "setup-error";
  }
  if (input.onboardingSurface === "full") {
    return "onboarding";
  }
  if (input.loading) {
    return "loading";
  }
  if (input.repoError) {
    return "repo-error";
  }
  if (input.total > 0) {
    return "has-results";
  }

  const extra = hasFacetFilters({
    language: input.language,
    topic: input.topic,
    categoryId: input.categoryId,
    query: input.query,
  });

  if (input.reviewPreset) {
    const counts = input.reviewCounts;
    if (!counts || counts.activeStars === 0) {
      return "review-never-synced";
    }
    if (
      (input.reviewPreset === "inactive" ||
        input.reviewPreset === "forgotten") &&
      isYoungLibrary(
        counts.activeStars,
        counts.oldestStarredAt,
        counts.inactive,
        counts.forgotten,
      ) &&
      !extra
    ) {
      return "review-young";
    }
    if (extra) {
      return "review-filter-empty";
    }
    if (input.reviewPreset === "unstarred") {
      return "review-unstarred-empty";
    }
    return "review-caught-up";
  }

  if (extra) {
    return "filter-empty";
  }
  if (!input.githubConnected) {
    return "no-pat";
  }
  if (input.lastSyncedAt != null || input.repoCount > 0) {
    return "true-empty";
  }
  return "never-synced";
}

export function emptyStateCopy(
  kind: EmptyStateKind,
  reviewLabel?: string,
): EmptyStateCopy {
  const queue = reviewLabel ?? "this queue";
  switch (kind) {
    case "setup-error":
      return {
        title: "Could not load setup status",
        body: "Starboard could not check first-run progress. You can still open Settings or try again.",
        primary: "retry",
        secondary: "settings",
      };
    case "no-pat":
      return {
        title: "Connect GitHub to get started",
        body: "Add a personal access token in Settings. The token stays in the OS keyring and is never stored in this library.",
        primary: "settings",
        secondary: null,
      };
    case "never-synced":
      return {
        title: "Library is empty",
        body: "GitHub is connected, but Starboard has not synced any stars yet.",
        primary: "sync",
        secondary: null,
      };
    case "true-empty":
      return {
        title: "No starred repositories",
        body: "The last sync completed and found no starred repositories. Star repos on GitHub, then Sync again.",
        primary: "sync",
        secondary: null,
      };
    case "filter-empty":
      return {
        title: "No matching repositories",
        body: "Nothing matches the current search or filters.",
        primary: "clear-filters",
        secondary: null,
      };
    case "review-never-synced":
      return {
        title: "Nothing local to review",
        body: "Sync your starred repos first. Review is local only and never writes stars back to GitHub.",
        primary: "sync",
        secondary: "clear-filters",
      };
    case "review-young":
      return {
        title: "This library is still young",
        body: "Inactive (no push in 12+ months) and Possibly forgotten (starred 24+ months ago, no push in 18+ months) fill as repositories age.",
        primary: "clear-filters",
        secondary: null,
      };
    case "review-unstarred-empty":
      return {
        title: "No unstarred history yet",
        body: "Previously starred, later-unstarred repos appear here after a sync records them. They stay out of active counts.",
        primary: "clear-filters",
        secondary: null,
      };
    case "review-filter-empty":
      return {
        title: `No matches in ${queue}`,
        body: `Nothing matches ${queue} plus the current search or filters.`,
        primary: "clear-filters",
        secondary: null,
      };
    case "review-caught-up":
      return {
        title: `Caught up on ${queue}`,
        body: "Reviewed and snoozed repositories stay out of this queue.",
        primary: "clear-filters",
        secondary: null,
      };
    case "repo-error":
      return {
        title: "Could not load repositories",
        body: "The library query failed. Try again, or open Settings if GitHub is disconnected.",
        primary: "retry",
        secondary: "settings",
      };
    default:
      return { title: "", body: "", primary: null, secondary: null };
  }
}
