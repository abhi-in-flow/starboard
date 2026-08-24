import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { CategoryTree } from "@/components/library/CategoryTree";
import { LibraryToolbar } from "@/components/library/LibraryToolbar";
import { RepoDetailPanel } from "@/components/library/RepoDetailPanel";
import { RepoVirtualList } from "@/components/library/RepoVirtualList";
import { OnboardingGuide } from "@/components/OnboardingGuide";
import { onboardingSurface } from "@/lib/onboarding";
import { isYoungLibrary, reviewPresetMeta } from "@/lib/review";
import { getReviewCounts, getSetupStatus, listRepos, searchRepos } from "@/lib/tauri";
import { useUiStore } from "@/store/ui";
import type { RepoFilters, RepoListResult } from "@/types";

export const LIBRARY_PAGE_SIZE = 100;

function useDebouncedValue<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), ms);
    return () => window.clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

export function LibraryView() {
  const searchRef = useRef<HTMLInputElement>(null);
  const query = useUiStore((s) => s.query);
  const debouncedQuery = useDebouncedValue(query, 150);
  const deferredQuery = useDeferredValue(debouncedQuery);

  const searchMode = useUiStore((s) => s.searchMode);
  const sort = useUiStore((s) => s.sort);
  const sortDesc = useUiStore((s) => s.sortDesc);
  const layout = useUiStore((s) => s.layout);
  const hideUnstarred = useUiStore((s) => s.hideUnstarred);
  const hideArchived = useUiStore((s) => s.hideArchived);
  const language = useUiStore((s) => s.language);
  const topic = useUiStore((s) => s.topic);
  const categoryId = useUiStore((s) => s.categoryId);
  const reviewPreset = useUiStore((s) => s.reviewPreset);
  const setReviewPreset = useUiStore((s) => s.setReviewPreset);
  const selectedRepoId = useUiStore((s) => s.selectedRepoId);
  const setSelectedRepoId = useUiStore((s) => s.setSelectedRepoId);
  const setQuery = useUiStore((s) => s.setQuery);
  const clearFilters = useUiStore((s) => s.clearFilters);

  const setupQuery = useQuery({
    queryKey: ["setupStatus"],
    queryFn: getSetupStatus,
  });
  const surface = setupQuery.data
    ? onboardingSurface(setupQuery.data)
    : setupQuery.isLoading
      ? "hidden"
      : "full";

  const filters: RepoFilters = useMemo(
    () => ({
      hideUnstarred,
      hideArchived,
      language,
      topic,
      categoryId,
      reviewPreset,
    }),
    [hideUnstarred, hideArchived, language, topic, categoryId, reviewPreset],
  );

  const reposQuery = useInfiniteQuery({
    queryKey: ["repos", deferredQuery, filters, sort, sortDesc, searchMode],
    initialPageParam: 0,
    queryFn: async ({ pageParam }) => {
      const started = performance.now();
      const result: RepoListResult =
        deferredQuery.trim().length === 0
          ? await listRepos({
              filters,
              sort,
              sortDesc,
              limit: LIBRARY_PAGE_SIZE,
              offset: pageParam,
            })
          : await searchRepos({
              query: deferredQuery,
              filters,
              sort,
              sortDesc,
              mode: searchMode,
              limit: LIBRARY_PAGE_SIZE,
              offset: pageParam,
            });
      if (import.meta.env.DEV) {
        console.debug(
          `[search] ${(performance.now() - started).toFixed(1)}ms — ${result.total} hits` +
            (result.searchMs != null
              ? ` (db ${result.searchMs}ms excl. embed)`
              : ""),
        );
      }
      return result;
    },
    getNextPageParam: (lastPage, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < lastPage.total ? loaded : undefined;
    },
  });

  const items = useMemo(
    () => reposQuery.data?.pages.flatMap((p) => p.items) ?? [],
    [reposQuery.data],
  );
  const total = reposQuery.data?.pages[0]?.total ?? 0;
  const searchHint = reposQuery.data?.pages[0]?.hint ?? null;

  const reviewCounts = useQuery({
    queryKey: ["reviewCounts"],
    queryFn: getReviewCounts,
  });

  const extraFilters = language != null || topic != null || categoryId != null;
  const emptyMessage = useMemo(() => {
    if (total > 0) {
      return null;
    }
    const counts = reviewCounts.data;
    if (!reviewPreset) {
      return extraFilters || deferredQuery.trim()
        ? "No repositories match your filters."
        : "No repositories yet. Sync your GitHub stars to fill the library.";
    }
    const meta = reviewPresetMeta(reviewPreset);
    if (!counts || counts.activeStars === 0) {
      return "Sync your starred repos first — there is nothing local to review yet. Review never writes stars back to GitHub.";
    }
    if (
      (reviewPreset === "inactive" || reviewPreset === "forgotten") &&
      isYoungLibrary(
        counts.activeStars,
        counts.oldestStarredAt,
        counts.inactive,
        counts.forgotten,
      ) &&
      !extraFilters &&
      !deferredQuery.trim()
    ) {
      return `Your library is still young. Inactive (no push in 12+ months) and forgotten (starred 24+ months ago, no push in 18+ months) queues fill as repos age.`;
    }
    if (extraFilters || deferredQuery.trim()) {
      return `No repositories match ${meta.label} plus your current search or filters.`;
    }
    if (reviewPreset === "unstarred") {
      return "No previously unstarred repos in local history yet. Unstars are recorded on sync and stay out of active counts.";
    }
    return `You're caught up on ${meta.label}. Reviewed and snoozed repos stay out of this queue.`;
  }, [total, reviewPreset, reviewCounts.data, extraFilters, deferredQuery]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchRef.current?.focus();
        return;
      }

      if (e.key === "Escape") {
        if (selectedRepoId != null) {
          setSelectedRepoId(null);
          return;
        }
        if (query) {
          setQuery("");
          return;
        }
        if (reviewPreset) {
          setReviewPreset(null);
          return;
        }
        clearFilters();
        return;
      }

      if (typing) {
        return;
      }

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (items.length === 0) {
          return;
        }
        const idx = items.findIndex((r) => r.id === selectedRepoId);
        let next = idx;
        if (e.key === "ArrowDown") {
          next = idx < 0 ? 0 : Math.min(items.length - 1, idx + 1);
          if (
            next === items.length - 1 &&
            reposQuery.hasNextPage &&
            !reposQuery.isFetchingNextPage
          ) {
            void reposQuery.fetchNextPage();
          }
        } else {
          next = idx < 0 ? 0 : Math.max(0, idx - 1);
        }
        setSelectedRepoId(items[next].id);
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    items,
    selectedRepoId,
    setSelectedRepoId,
    query,
    setQuery,
    reviewPreset,
    setReviewPreset,
    clearFilters,
    reposQuery,
  ]);

  if (surface === "full") {
    return (
      <div className="min-h-[calc(100vh-7.5rem)]">
        <OnboardingGuide variant="full" />
      </div>
    );
  }

  return (
    <div className="flex h-[calc(100vh-7.5rem)] min-h-0 flex-col">
      {surface === "banner" ? <OnboardingGuide variant="banner" /> : null}
      <div className="flex min-h-0 flex-1">
        <div className="hidden w-52 shrink-0 md:block">
          <CategoryTree />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <LibraryToolbar
            searchRef={searchRef}
            total={total}
            searchHint={searchHint}
          />
          <div className="min-h-0 flex-1">
            {reposQuery.isLoading || setupQuery.isLoading ? (
              <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
                Loading library…
              </div>
            ) : reposQuery.isError ? (
              <div className="flex h-40 items-center justify-center text-sm text-destructive">
                Failed to load repositories.
              </div>
            ) : (
              <RepoVirtualList
                items={items}
                layout={layout}
                selectedId={selectedRepoId}
                onSelect={setSelectedRepoId}
                emptyMessage={emptyMessage}
                reviewMode={reviewPreset != null}
                onEndReached={() => {
                  if (
                    reposQuery.hasNextPage &&
                    !reposQuery.isFetchingNextPage
                  ) {
                    void reposQuery.fetchNextPage();
                  }
                }}
              />
            )}
          </div>
        </div>
        {selectedRepoId != null ? (
          <RepoDetailPanel repoId={selectedRepoId} />
        ) : null}
      </div>
    </div>
  );
}
