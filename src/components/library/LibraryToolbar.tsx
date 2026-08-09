import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { LayoutGrid, List, Search, X } from "lucide-react";
import { useEffect } from "react";
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
import {
  getEmbedStatus,
  getLibraryFacets,
  getOllamaStatus,
  startEmbedding,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type { EmbedProgress, RepoFilters, RepoSort, SearchMode } from "@/types";

type Props = {
  searchRef: React.Ref<HTMLInputElement>;
  total: number;
  searchHint?: string | null;
};

const MODES: { id: SearchMode; label: string }[] = [
  { id: "keyword", label: "Keyword" },
  { id: "semantic", label: "Semantic" },
  { id: "hybrid", label: "Hybrid" },
];

export function LibraryToolbar({ searchRef, total, searchHint }: Props) {
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
  const categoryId = useUiStore((s) => s.categoryId);

  const filters: RepoFilters = {
    hideUnstarred,
    hideArchived,
    language,
    topic,
    categoryId,
  };

  const facets = useQuery({
    queryKey: ["facets", filters],
    queryFn: () => getLibraryFacets(filters),
  });

  const ollama = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
    refetchInterval: 15_000,
  });

  const embedStatus = useQuery({
    queryKey: ["embedStatus"],
    queryFn: getEmbedStatus,
    refetchInterval: (q) => (q.state.data?.running ? 1000 : 10_000),
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

  // Default Hybrid when ≥90% of repos are embedded; otherwise Keyword + hint.
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

  const startEmbedMutation = useMutation({
    mutationFn: startEmbedding,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
    },
  });

  const offline = ollama.data != null && !ollama.data.available;
  const coverage = embedStatus.data?.coverage ?? 0;
  const needEmbeddings =
    (embedStatus.data?.staleOrMissing ?? 0) > 0 || coverage < 0.9;
  const hasChips = language != null || topic != null || categoryId != null;
  // Semantic/Hybrid results are RRF-ordered; sort control would be misleading.
  const sortedByRelevance =
    query.trim().length > 0 &&
    (searchMode === "semantic" || searchMode === "hybrid");

  const coverageHint =
    !searchModeInitialized || coverage >= 0.9
      ? null
      : `Embeddings at ${Math.round(coverage * 100)}% — defaulting to Keyword until ≥90%.`;

  return (
    <div className="flex flex-col gap-3 border-b border-border bg-background/80 px-4 py-3">
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
          />
          <kbd className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
            /
          </kbd>
        </div>
        <div className="flex items-center rounded-lg border border-border p-0.5">
          {MODES.map((m) => {
            const disabled = m.id !== "keyword" && offline;
            return (
              <Button
                key={m.id}
                type="button"
                size="sm"
                variant={searchMode === m.id ? "secondary" : "ghost"}
                className="h-8 px-2.5 text-xs"
                disabled={disabled}
                title={
                  offline && m.id !== "keyword" ? "Ollama offline" : undefined
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
          <Select value={sort} onValueChange={(v) => setSort(v as RepoSort)}>
            <SelectTrigger className="h-10 w-36">
              <SelectValue placeholder="Sort" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="starredAt">Starred</SelectItem>
              <SelectItem value="stars">Stars</SelectItem>
              <SelectItem value="pushedAt">Pushed</SelectItem>
              <SelectItem value="name">Name</SelectItem>
            </SelectContent>
          </Select>
        )}
        <div className="flex items-center rounded-lg border border-border p-0.5">
          <Button
            type="button"
            size="sm"
            variant={layout === "list" ? "secondary" : "ghost"}
            className="h-8 px-2"
            onClick={() => setLayout("list")}
            aria-label="List layout"
          >
            <List className="size-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant={layout === "grid" ? "secondary" : "ghost"}
            className="h-8 px-2"
            onClick={() => setLayout("grid")}
            aria-label="Grid layout"
          >
            <LayoutGrid className="size-4" />
          </Button>
        </div>
        <span className="ml-auto text-xs font-medium text-muted-foreground">
          {total.toLocaleString()} repositories
        </span>
      </div>

      {(coverageHint || searchHint || offline || needEmbeddings) && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {offline ? (
            <span>
              Ollama offline — Semantic/Hybrid disabled. Keyword still works.
            </span>
          ) : null}
          {coverageHint ? <span>{coverageHint}</span> : null}
          {searchHint ? <span>{searchHint}</span> : null}
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
          {embedStatus.data?.needRebuild ? (
            <Badge variant="secondary">
              Dimension changed — rebuild table in Settings
            </Badge>
          ) : null}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-4">
        <div className="flex items-center gap-2 text-sm">
          <Checkbox
            id="hide-unstarred"
            checked={hideUnstarred}
            onCheckedChange={(v) => setHideUnstarred(v === true)}
          />
          <Label htmlFor="hide-unstarred">Hide unstarred</Label>
        </div>
        <div className="flex items-center gap-2 text-sm">
          <Checkbox
            id="hide-archived"
            checked={hideArchived}
            onCheckedChange={(v) => setHideArchived(v === true)}
          />
          <Label htmlFor="hide-archived">Hide archived</Label>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-muted-foreground">
          Languages
        </span>
        {(facets.data?.languages ?? []).slice(0, 12).map((f) => (
          <button
            key={f.name}
            type="button"
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
          <span className="text-xs font-medium text-muted-foreground">
            Topics
          </span>
          {(facets.data?.topics ?? []).slice(0, 10).map((f) => (
            <button
              key={f.name}
              type="button"
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

      {hasChips ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 w-fit gap-1 px-2 text-xs"
          onClick={clearFilters}
        >
          <X className="size-3" />
          Clear filters
        </Button>
      ) : null}
    </div>
  );
}
