import { useQuery } from "@tanstack/react-query";
import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { CategoryTree } from "@/components/library/CategoryTree";
import { LibraryToolbar } from "@/components/library/LibraryToolbar";
import { RepoDetailPanel } from "@/components/library/RepoDetailPanel";
import { RepoVirtualList } from "@/components/library/RepoVirtualList";
import { listRepos, searchRepos } from "@/lib/tauri";
import { useUiStore } from "@/store/ui";
import type { RepoFilters } from "@/types";

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
  const selectedRepoId = useUiStore((s) => s.selectedRepoId);
  const setSelectedRepoId = useUiStore((s) => s.setSelectedRepoId);
  const setQuery = useUiStore((s) => s.setQuery);
  const clearFilters = useUiStore((s) => s.clearFilters);

  const filters: RepoFilters = useMemo(
    () => ({
      hideUnstarred,
      hideArchived,
      language,
      topic,
      categoryId,
    }),
    [hideUnstarred, hideArchived, language, topic, categoryId],
  );

  const reposQuery = useQuery({
    queryKey: ["repos", deferredQuery, filters, sort, sortDesc, searchMode],
    queryFn: async () => {
      const started = performance.now();
      const result =
        deferredQuery.trim().length === 0
          ? await listRepos({
              filters,
              sort,
              sortDesc,
            })
          : await searchRepos({
              query: deferredQuery,
              filters,
              sort,
              sortDesc,
              mode: searchMode,
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
  });

  const items = reposQuery.data?.items ?? [];
  const total = reposQuery.data?.total ?? 0;
  const searchHint = reposQuery.data?.hint ?? null;

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
        } else {
          next = idx < 0 ? 0 : Math.max(0, idx - 1);
        }
        setSelectedRepoId(items[next].id);
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [items, selectedRepoId, setSelectedRepoId, query, setQuery, clearFilters]);

  return (
    <div className="flex h-[calc(100vh-7.5rem)] min-h-0">
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
          {reposQuery.isLoading ? (
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
            />
          )}
        </div>
      </div>
      {selectedRepoId != null ? (
        <RepoDetailPanel repoId={selectedRepoId} />
      ) : null}
    </div>
  );
}
