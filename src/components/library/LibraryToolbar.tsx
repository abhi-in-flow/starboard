import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDownWideNarrow,
  ArrowUpWideNarrow,
  ChevronDown,
  LayoutGrid,
  List,
  Search,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { findCategoryName, flattenCategories } from "@/lib/categories";
import { isCancelledMessage } from "@/lib/jobStatus";
import { KEYBOARD_HELP } from "@/lib/keyboard";
import {
  activeFilterChips,
  type FilterChip,
  hasClearableLibraryState,
  removeFilterChip,
} from "@/lib/libraryFilters";
import { REVIEW_PRESETS, reviewPresetMeta } from "@/lib/review";
import { searchModeFallback, searchSortIsRelevance } from "@/lib/searchMode";
import { ollamaAvailabilityLabel } from "@/lib/statusCopy";
import {
  cancelEmbedding,
  getEmbedStatus,
  getLibraryFacets,
  getOllamaStatus,
  getReviewCounts,
  listCategories,
  startEmbedding,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type {
  EmbedProgress,
  RepoFilters,
  RepoSort,
  ReviewPreset,
  SearchMode,
} from "@/types";

type Props = {
  searchRef: React.Ref<HTMLInputElement>;
  total: number;
  searchHint?: string | null;
  modeUsed?: SearchMode | null;
};

const MODES: { id: SearchMode; label: string }[] = [
  { id: "keyword", label: "Keyword" },
  { id: "semantic", label: "Semantic" },
  { id: "hybrid", label: "Hybrid" },
];

export function LibraryToolbar({
  searchRef,
  total,
  searchHint,
  modeUsed,
}: Props) {
  const queryClient = useQueryClient();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const searchMode = useUiStore((s) => s.searchMode);
  const setSearchMode = useUiStore((s) => s.setSearchMode);
  const searchModeInitialized = useUiStore((s) => s.searchModeInitialized);
  const setSearchModeInitialized = useUiStore(
    (s) => s.setSearchModeInitialized,
  );
  const sort = useUiStore((s) => s.sort);
  const setSort = useUiStore((s) => s.setSort);
  const sortDesc = useUiStore((s) => s.sortDesc);
  const setSortDesc = useUiStore((s) => s.setSortDesc);
  const layout = useUiStore((s) => s.layout);
  const setLayout = useUiStore((s) => s.setLayout);
  const hideUnstarred = useUiStore((s) => s.hideUnstarred);
  const hideArchived = useUiStore((s) => s.hideArchived);
  const setHideUnstarred = useUiStore((s) => s.setHideUnstarred);
  const setHideArchived = useUiStore((s) => s.setHideArchived);
  const language = useUiStore((s) => s.language);
  const topic = useUiStore((s) => s.topic);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTopic = useUiStore((s) => s.setTopic);
  const clearFilters = useUiStore((s) => s.clearFilters);
  const applyFilterPatch = useUiStore((s) => s.applyFilterPatch);
  const categoryId = useUiStore((s) => s.categoryId);
  const setCategoryId = useUiStore((s) => s.setCategoryId);
  const reviewPreset = useUiStore((s) => s.reviewPreset);
  const setReviewPreset = useUiStore((s) => s.setReviewPreset);

  const filterState = {
    language,
    topic,
    categoryId,
    reviewPreset,
    hideUnstarred,
    hideArchived,
    sort,
    sortDesc,
  };

  const filters: RepoFilters = {
    hideUnstarred,
    hideArchived,
    language,
    topic,
    categoryId,
    reviewPreset,
  };

  const facets = useQuery({
    queryKey: ["facets", filters],
    queryFn: () => getLibraryFacets(filters),
  });

  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const reviewCounts = useQuery({
    queryKey: ["reviewCounts"],
    queryFn: getReviewCounts,
  });

  const ollama = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
    staleTime: 15_000,
    refetchInterval: 15_000,
  });

  const embedStatus = useQuery({
    queryKey: ["embedStatus"],
    queryFn: getEmbedStatus,
    refetchInterval: (q) => (q.state.data?.running ? 1000 : 15_000),
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<EmbedProgress>("embed://progress", () => {
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [queryClient]);

  useEffect(() => {
    if (searchModeInitialized || !embedStatus.data) {
      return;
    }
    if (embedStatus.data.coverage >= 0.9 && embedStatus.data.totalRepos > 0) {
      setSearchMode("hybrid");
    } else {
      setSearchMode("keyword");
    }
    setSearchModeInitialized(true);
  }, [
    embedStatus.data,
    searchModeInitialized,
    setSearchMode,
    setSearchModeInitialized,
  ]);

  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [jobError, setJobError] = useState<string | null>(null);
  const [filtersOpen, setFiltersOpen] = useState(false);

  const startEmbedMutation = useMutation({
    mutationFn: startEmbedding,
    onSuccess: () => {
      setJobNotice(null);
      setJobError(null);
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => {
      const message =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: unknown }).message)
          : "Embedding failed";
      if (isCancelledMessage(message)) {
        setJobNotice(message);
        setJobError(null);
      } else {
        setJobError(message);
      }
    },
  });

  const cancelEmbedMutation = useMutation({
    mutationFn: cancelEmbedding,
    onSuccess: () => {
      setJobNotice("Cancelled");
      setJobError(null);
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
    },
    onError: (err) => {
      const message =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: unknown }).message)
          : "Cancel failed";
      setJobError(message);
    },
  });

  const offline = ollama.data != null && !ollama.data.available;
  const coverage = embedStatus.data?.coverage ?? 0;
  const needEmbeddings =
    (embedStatus.data?.staleOrMissing ?? 0) > 0 || coverage < 0.9;
  const fallback = searchModeFallback(searchMode, modeUsed);
  const sortedByRelevance = searchSortIsRelevance(query, searchMode, modeUsed);
  const categoryName = findCategoryName(categories.data ?? [], categoryId);
  const chips = activeFilterChips(filterState, categoryName);
  const showClear = hasClearableLibraryState(filterState);
  const flatCats = flattenCategories(categories.data ?? []);
  const countFor = (id: ReviewPreset) => reviewCounts.data?.[id] ?? null;

  const coverageHint =
    !searchModeInitialized || coverage >= 0.9
      ? null
      : `Embeddings at ${Math.round(coverage * 100)}% — defaulting to Keyword until ≥90%.`;

  function onChipRemove(chip: FilterChip) {
    applyFilterPatch(removeFilterChip(filterState, chip));
  }

  return (
    <div className="flex shrink-0 flex-col gap-2 border-b border-border bg-background/80 px-4 py-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search repositories, descriptions, topics…"
            className="h-10 pr-12 pl-9"
            aria-label="Search repositories"
            aria-describedby="library-keyboard-help"
          />
          <kbd className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-foreground/70">
            /
          </kbd>
        </div>
        <div
          role="radiogroup"
          aria-label="Search mode"
          className="flex items-center rounded-lg border border-border p-0.5"
        >
          {MODES.map((m) => {
            const disabled = m.id !== "keyword" && offline;
            const checked = searchMode === m.id;
            return (
              <Button
                key={m.id}
                type="button"
                size="sm"
                role="radio"
                aria-checked={checked}
                variant={checked ? "secondary" : "ghost"}
                className="h-8 px-2.5 text-xs"
                disabled={disabled}
                title={
                  offline && m.id !== "keyword"
                    ? ollamaAvailabilityLabel(false)
                    : undefined
                }
                onClick={() => setSearchMode(m.id)}
              >
                {m.label}
              </Button>
            );
          })}
        </div>
        {sortedByRelevance ? (
          <Badge
            variant="secondary"
            className="h-10 rounded-md px-3 font-normal"
            title="Results are ordered by fused relevance score"
          >
            Sorted by relevance
          </Badge>
        ) : (
          <div className="flex items-center gap-1">
            <Select value={sort} onValueChange={(v) => setSort(v as RepoSort)}>
              <SelectTrigger className="h-10 w-36" aria-label="Sort by">
                <SelectValue placeholder="Sort" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="starredAt">Starred</SelectItem>
                <SelectItem value="stars">Stars</SelectItem>
                <SelectItem value="pushedAt">Pushed</SelectItem>
                <SelectItem value="name">Name</SelectItem>
                <SelectItem value="stale">Oldest / stale</SelectItem>
              </SelectContent>
            </Select>
            <Button
              type="button"
              variant="outline"
              size="icon"
              className="size-10"
              aria-label={sortDesc ? "Sort descending" : "Sort ascending"}
              aria-pressed={sortDesc}
              title={sortDesc ? "Descending" : "Ascending"}
              onClick={() => setSortDesc(!sortDesc)}
            >
              {sortDesc ? (
                <ArrowDownWideNarrow className="size-4" />
              ) : (
                <ArrowUpWideNarrow className="size-4" />
              )}
            </Button>
          </div>
        )}
        <div
          role="radiogroup"
          aria-label="Library layout"
          className="flex items-center rounded-lg border border-border p-0.5"
        >
          <Button
            type="button"
            size="sm"
            role="radio"
            aria-checked={layout === "list"}
            variant={layout === "list" ? "secondary" : "ghost"}
            className="h-8 px-2"
            onClick={() => setLayout("list")}
            aria-label="List layout"
            title="List layout"
          >
            <List className="size-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            role="radio"
            aria-checked={layout === "grid"}
            variant={layout === "grid" ? "secondary" : "ghost"}
            className="h-8 px-2"
            onClick={() => setLayout("grid")}
            aria-label="Grid layout"
            title="Grid layout"
          >
            <LayoutGrid className="size-4" />
          </Button>
        </div>
        <span
          className="ml-auto text-xs font-medium text-foreground/70"
          aria-live="polite"
        >
          {total.toLocaleString()}{" "}
          {reviewPreset
            ? reviewPreset === "unstarred"
              ? "in history"
              : "in review"
            : "repositories"}
        </span>
      </div>

      {(fallback.badge ||
        coverageHint ||
        searchHint ||
        offline ||
        needEmbeddings) && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-foreground/70">
          {fallback.badge ? (
            <Badge
              variant="outline"
              className="font-normal"
              title={fallback.title ?? undefined}
            >
              {fallback.badge}
              <span className="text-foreground/60">
                {" "}
                (requested {fallback.requestedLabel})
              </span>
            </Badge>
          ) : null}
          {offline ? (
            <span role="status">
              {ollamaAvailabilityLabel(false)} — Semantic/Hybrid disabled.
              Keyword still works.
            </span>
          ) : null}
          {coverageHint ? <span role="status">{coverageHint}</span> : null}
          {searchHint ? <span role="status">{searchHint}</span> : null}
          {needEmbeddings ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-7 px-2 text-xs"
              disabled={
                offline ||
                embedStatus.data?.running ||
                startEmbedMutation.isPending ||
                embedStatus.data?.needRebuild
              }
              onClick={() => startEmbedMutation.mutate()}
            >
              {embedStatus.data?.running
                ? `Embedding… ${embedStatus.data.embeddedRepos}/${embedStatus.data.totalRepos}`
                : "Build embeddings"}
            </Button>
          ) : null}
          {embedStatus.data?.running ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-7 px-2 text-xs"
              disabled={cancelEmbedMutation.isPending}
              onClick={() => cancelEmbedMutation.mutate()}
            >
              Cancel
            </Button>
          ) : null}
          {jobNotice ? <span role="status">{jobNotice}</span> : null}
          {jobError ? (
            <span className="text-destructive" role="alert">
              {jobError}
            </span>
          ) : null}
          {embedStatus.data?.needRebuild ? (
            <Badge variant="secondary">
              Dimension changed — rebuild table in Settings
            </Badge>
          ) : null}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 gap-1.5"
          aria-expanded={filtersOpen}
          aria-controls="library-more-filters"
          onClick={() => setFiltersOpen((v) => !v)}
        >
          <SlidersHorizontal className="size-3.5" />
          {filtersOpen ? "Hide filters" : "Filters"}
          <ChevronDown
            className={cn(
              "size-3.5 transition-transform",
              filtersOpen && "rotate-180",
            )}
          />
        </Button>
        {chips.map((chip) => (
          <button
            key={chip.id}
            type="button"
            onClick={() => onChipRemove(chip)}
            aria-label={`Remove ${chip.kind} filter ${chip.label}`}
          >
            <Badge
              variant={chip.kind === "review" ? "default" : "secondary"}
              className="cursor-pointer gap-1 rounded-full px-2.5 py-0.5 font-normal"
              title={
                chip.kind === "review" && reviewPreset
                  ? reviewPresetMeta(reviewPreset).description
                  : `Remove ${chip.label}`
              }
            >
              {chip.kind === "review"
                ? `Review: ${chip.label}`
                : chip.kind === "category"
                  ? chip.label
                  : chip.label}
              <X className="size-3" />
            </Badge>
          </button>
        ))}
        {showClear ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-fit gap-1 px-2 text-xs"
            onClick={clearFilters}
          >
            <X className="size-3" />
            Clear all
          </Button>
        ) : null}
      </div>

      {filtersOpen ? (
        <div
          id="library-more-filters"
          className="flex flex-col gap-3 rounded-md border border-border bg-muted/20 px-3 py-3"
        >
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex min-w-0 flex-col gap-1">
              <Label className="text-xs text-foreground/70">Review queue</Label>
              <Select
                value={reviewPreset ?? "browse"}
                onValueChange={(v) =>
                  setReviewPreset(v === "browse" ? null : (v as ReviewPreset))
                }
              >
                <SelectTrigger
                  className="h-9 w-[14rem]"
                  aria-label="Review queue"
                >
                  <SelectValue placeholder="Review" />
                </SelectTrigger>
                <SelectContent className="w-80">
                  <SelectItem value="browse">Browse library</SelectItem>
                  {REVIEW_PRESETS.map((p) => {
                    const n = countFor(p.id);
                    return (
                      <SelectItem key={p.id} value={p.id} title={p.description}>
                        {p.label}
                        {n != null ? ` (${n})` : ""}
                      </SelectItem>
                    );
                  })}
                </SelectContent>
              </Select>
            </div>
            <div className="flex min-w-0 flex-col gap-1 md:hidden">
              <Label className="text-xs text-foreground/70">Category</Label>
              <Select
                value={categoryId != null ? String(categoryId) : "all"}
                onValueChange={(v) =>
                  setCategoryId(v === "all" ? null : Number(v))
                }
              >
                <SelectTrigger className="h-9 w-[14rem]" aria-label="Category">
                  <SelectValue placeholder="All categories" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All categories</SelectItem>
                  {flatCats.map((c) => (
                    <SelectItem key={c.id} value={String(c.id)}>
                      {c.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center gap-2 text-sm">
              <Checkbox
                id="hide-unstarred"
                checked={hideUnstarred}
                disabled={reviewPreset === "unstarred"}
                onCheckedChange={(v) => setHideUnstarred(v === true)}
              />
              <Label htmlFor="hide-unstarred">Hide unstarred</Label>
            </div>
            <div className="flex items-center gap-2 text-sm">
              <Checkbox
                id="hide-archived"
                checked={hideArchived}
                disabled={
                  reviewPreset === "archived" || reviewPreset === "unstarred"
                }
                onCheckedChange={(v) => setHideArchived(v === true)}
              />
              <Label htmlFor="hide-archived">Hide archived</Label>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs font-medium text-foreground/70">
              Languages
            </span>
            {(facets.data?.languages ?? []).slice(0, 12).map((f) => (
              <button
                key={f.name}
                type="button"
                aria-pressed={language === f.name}
                onClick={() => setLanguage(language === f.name ? null : f.name)}
              >
                <Badge
                  variant={language === f.name ? "default" : "secondary"}
                  className={cn(
                    "cursor-pointer rounded-full px-2.5 py-0.5 font-normal",
                    language !== f.name && "bg-muted hover:bg-muted/80",
                  )}
                >
                  {f.name} {f.count}
                </Badge>
              </button>
            ))}
          </div>

          {(facets.data?.topics?.length ?? 0) > 0 ? (
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-medium text-foreground/70">
                Topics
              </span>
              {(facets.data?.topics ?? []).slice(0, 10).map((f) => (
                <button
                  key={f.name}
                  type="button"
                  aria-pressed={topic === f.name}
                  onClick={() => setTopic(topic === f.name ? null : f.name)}
                >
                  <Badge
                    variant={topic === f.name ? "default" : "secondary"}
                    className={cn(
                      "cursor-pointer rounded-full px-2.5 py-0.5 font-normal",
                      topic !== f.name && "bg-muted hover:bg-muted/80",
                    )}
                  >
                    {f.name} {f.count}
                  </Badge>
                </button>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      <p id="library-keyboard-help" className="text-[11px] text-foreground/65">
        {KEYBOARD_HELP}
      </p>
    </div>
  );
}
