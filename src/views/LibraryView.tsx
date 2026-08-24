import {
  useInfiniteQuery,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { CategoryTree } from "@/components/library/CategoryTree";
import { LibraryEmptyState } from "@/components/library/LibraryEmptyState";
import { LibraryToolbar } from "@/components/library/LibraryToolbar";
import { RepoDetailPanel } from "@/components/library/RepoDetailPanel";
import { RepoVirtualList } from "@/components/library/RepoVirtualList";
import { OnboardingGuide } from "@/components/OnboardingGuide";
import { Button } from "@/components/ui/button";
import {
  keyOwnerFromEventTarget,
  nextEscapeAction,
  shouldHandleLibraryNavKeys,
} from "@/lib/keyboard";
import { selectEmptyState } from "@/lib/libraryEmpty";
import { hasClearableLibraryState } from "@/lib/libraryFilters";
import { onboardingSurface } from "@/lib/onboarding";
import {
  createPendingAdvance,
  type PendingAdvance,
  resolvePendingAdvance,
  serializeListGeneration,
} from "@/lib/pendingAdvance";
import { reviewPresetMeta } from "@/lib/review";
import {
  getReviewCounts,
  getSetupStatus,
  listRepos,
  searchRepos,
  startSync,
} from "@/lib/tauri";
import { useDockedDetail, useWideLayout } from "@/lib/useMediaQuery";
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
  const queryClient = useQueryClient();
  const wide = useWideLayout();
  const dockedDetail = useDockedDetail();
  const pendingAdvance = useRef<PendingAdvance | null>(null);

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
  const selectedRepoId = useUiStore((s) => s.selectedRepoId);
  const setSelectedRepoId = useUiStore((s) => s.setSelectedRepoId);
  const setQuery = useUiStore((s) => s.setQuery);
  const clearFilters = useUiStore((s) => s.clearFilters);
  const openSettings = useUiStore((s) => s.openSettings);

  const setupQuery = useQuery({
    queryKey: ["setupStatus"],
    queryFn: getSetupStatus,
  });
  const surface = setupQuery.data
    ? onboardingSurface(setupQuery.data)
    : "hidden";

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
  const modeUsed = reposQuery.data?.pages[0]?.modeUsed;

  const reviewCounts = useQuery({
    queryKey: ["reviewCounts"],
    queryFn: getReviewCounts,
  });

  const listGeneration = serializeListGeneration({
    query: deferredQuery,
    language,
    topic,
    categoryId,
    reviewPreset,
    hideUnstarred,
    hideArchived,
    sort,
    sortDesc,
    searchMode,
  });

  const emptyKind = selectEmptyState({
    total,
    loading: reposQuery.isLoading || setupQuery.isLoading,
    repoError: reposQuery.isError,
    setupError: setupQuery.isError,
    onboardingSurface: surface,
    githubConnected: setupQuery.data?.githubConnected ?? false,
    lastSyncedAt: setupQuery.data?.lastSyncedAt ?? null,
    repoCount: setupQuery.data?.repoCount ?? 0,
    query: deferredQuery,
    language,
    topic,
    categoryId,
    reviewPreset,
    reviewCounts: reviewCounts.data ?? null,
  });

  useEffect(() => {
    const result = resolvePendingAdvance({
      pending: pendingAdvance.current,
      generation: listGeneration,
      selectedId: selectedRepoId,
      itemIds: items.map((item) => item.id),
      fetchFailed: reposQuery.isFetchNextPageError,
      fetchSettled: !reposQuery.isFetchingNextPage,
    });
    if (result.kind === "clear") {
      pendingAdvance.current = null;
      return;
    }
    if (result.kind === "select") {
      pendingAdvance.current = null;
      setSelectedRepoId(result.id);
    }
  }, [
    listGeneration,
    selectedRepoId,
    items,
    reposQuery.isFetchNextPageError,
    reposQuery.isFetchingNextPage,
    setSelectedRepoId,
  ]);

  const navRef = useRef({
    items,
    selectedRepoId,
    query,
    language,
    topic,
    categoryId,
    reviewPreset,
    hideUnstarred,
    hideArchived,
    sort,
    sortDesc,
    listGeneration,
    overlayBlocksNav: !dockedDetail && selectedRepoId != null,
    hasNextPage: reposQuery.hasNextPage,
    isFetchingNextPage: reposQuery.isFetchingNextPage,
    fetchFailed: reposQuery.isFetchNextPageError,
    fetchNextPage: () => {
      void reposQuery.fetchNextPage();
    },
  });
  navRef.current = {
    items,
    selectedRepoId,
    query,
    language,
    topic,
    categoryId,
    reviewPreset,
    hideUnstarred,
    hideArchived,
    sort,
    sortDesc,
    listGeneration,
    overlayBlocksNav: !dockedDetail && selectedRepoId != null,
    hasNextPage: reposQuery.hasNextPage,
    isFetchingNextPage: reposQuery.isFetchingNextPage,
    fetchFailed: reposQuery.isFetchNextPageError,
    fetchNextPage: () => {
      void reposQuery.fetchNextPage();
    },
  };

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const s = navRef.current;
      const ownsNavKeys =
        e.key === "/" || e.key === "ArrowDown" || e.key === "ArrowUp";
      if (
        ownsNavKeys &&
        !shouldHandleLibraryNavKeys({
          defaultPrevented: e.defaultPrevented,
          target: keyOwnerFromEventTarget(e.target),
        })
      ) {
        return;
      }

      if (e.key === "/" && !s.overlayBlocksNav) {
        e.preventDefault();
        searchRef.current?.focus();
        return;
      }

      if (e.key === "Escape") {
        if (e.defaultPrevented) {
          return;
        }
        const action = nextEscapeAction({
          detailOpen: s.selectedRepoId != null,
          hasQuery: Boolean(s.query),
          hasFiltersOrReview: hasClearableLibraryState({
            language: s.language,
            topic: s.topic,
            categoryId: s.categoryId,
            reviewPreset: s.reviewPreset,
            hideUnstarred: s.hideUnstarred,
            hideArchived: s.hideArchived,
            sort: s.sort,
            sortDesc: s.sortDesc,
          }),
        });
        if (action === "close-detail") {
          setSelectedRepoId(null);
        } else if (action === "clear-query") {
          setQuery("");
        } else if (action === "clear-filters") {
          clearFilters();
        }
        return;
      }

      if (s.overlayBlocksNav) {
        return;
      }

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (s.items.length === 0) {
          return;
        }
        const idx = s.items.findIndex((r) => r.id === s.selectedRepoId);
        let next = idx;
        if (e.key === "ArrowDown") {
          next = idx < 0 ? 0 : Math.min(s.items.length - 1, idx + 1);
          if (next === s.items.length - 1) {
            pendingAdvance.current = createPendingAdvance({
              generation: s.listGeneration,
              sourceId: s.items[next].id,
              hasNextPage: Boolean(s.hasNextPage),
              isFetchingNextPage: s.isFetchingNextPage,
              fetchFailed: s.fetchFailed,
            });
            if (pendingAdvance.current) {
              s.fetchNextPage();
            }
          }
        } else {
          next = idx < 0 ? 0 : Math.max(0, idx - 1);
        }
        const nextId = s.items[next].id;
        setSelectedRepoId(nextId);
        document
          .getElementById(`library-repo-${nextId}`)
          ?.focus({ preventScroll: true });
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setSelectedRepoId, setQuery, clearFilters]);

  const onEndReached = useCallback(() => {
    if (
      reposQuery.hasNextPage &&
      !reposQuery.isFetchingNextPage &&
      !reposQuery.isFetchNextPageError
    ) {
      void reposQuery.fetchNextPage();
    }
  }, [
    reposQuery.hasNextPage,
    reposQuery.isFetchingNextPage,
    reposQuery.isFetchNextPageError,
    reposQuery.fetchNextPage,
  ]);

  if (emptyKind === "onboarding") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <OnboardingGuide variant="full" />
      </div>
    );
  }

  const showDetail = selectedRepoId != null;
  const docked = dockedDetail && showDetail;
  const overlay = !dockedDetail && showDetail;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {emptyKind === "setup-error" ? (
        <div className="shrink-0 border-b border-border px-4 py-3">
          <LibraryEmptyState
            kind="setup-error"
            errorDetail={
              setupQuery.error &&
              typeof setupQuery.error === "object" &&
              "message" in setupQuery.error
                ? String((setupQuery.error as { message: unknown }).message)
                : null
            }
            onClearFilters={clearFilters}
            onSync={() => void startSync(false)}
            onRetry={() => void setupQuery.refetch()}
            onSettings={() => openSettings("github")}
          />
        </div>
      ) : null}
      {surface === "banner" ? <OnboardingGuide variant="banner" /> : null}
      <div className="flex min-h-0 flex-1">
        {wide ? (
          <div className="hidden w-52 shrink-0 md:block">
            <CategoryTree />
          </div>
        ) : null}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <LibraryToolbar
            searchRef={searchRef}
            total={total}
            searchHint={searchHint}
            modeUsed={modeUsed}
          />
          <div className="min-h-0 flex-1">
            {emptyKind === "loading" ? (
              <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
                Loading library…
              </div>
            ) : emptyKind === "has-results" || items.length > 0 ? (
              <div className="flex h-full min-h-0 flex-col">
                <div className="min-h-0 flex-1">
                  <RepoVirtualList
                    items={items}
                    layout={layout}
                    selectedId={selectedRepoId}
                    onSelect={setSelectedRepoId}
                    reviewMode={reviewPreset != null}
                    total={total}
                    listGeneration={listGeneration}
                    hasNextPage={Boolean(reposQuery.hasNextPage)}
                    isFetchingNextPage={reposQuery.isFetchingNextPage}
                    fetchFailed={reposQuery.isFetchNextPageError}
                    onEndReached={onEndReached}
                  />
                </div>
                {reposQuery.isFetchNextPageError ? (
                  <div
                    className="flex shrink-0 items-center justify-between gap-3 border-t border-border px-4 py-2"
                    role="alert"
                  >
                    <p className="text-sm text-destructive">
                      Could not load more repositories.
                    </p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => void reposQuery.fetchNextPage()}
                    >
                      Retry
                    </Button>
                  </div>
                ) : null}
              </div>
            ) : emptyKind !== "setup-error" ? (
              <LibraryEmptyState
                kind={emptyKind}
                reviewLabel={
                  reviewPreset
                    ? reviewPresetMeta(reviewPreset).label
                    : undefined
                }
                onClearFilters={() => {
                  setQuery("");
                  clearFilters();
                }}
                onSync={() => {
                  void startSync(false).then(() => {
                    void queryClient.invalidateQueries({
                      queryKey: ["syncStatus"],
                    });
                    void queryClient.invalidateQueries({
                      queryKey: ["setupStatus"],
                    });
                    void queryClient.invalidateQueries({ queryKey: ["repos"] });
                  });
                }}
                onRetry={() => {
                  void reposQuery.refetch();
                  void setupQuery.refetch();
                }}
                onSettings={() => openSettings("github")}
                syncDisabled={!setupQuery.data?.githubConnected}
              />
            ) : null}
          </div>
        </div>
        {docked && selectedRepoId != null ? (
          <RepoDetailPanel repoId={selectedRepoId} variant="docked" />
        ) : null}
        {overlay && selectedRepoId != null ? (
          <RepoDetailPanel repoId={selectedRepoId} variant="overlay" />
        ) : null}
      </div>
    </div>
  );
}
