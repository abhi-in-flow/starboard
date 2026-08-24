import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, Star } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { REPO_DND_TYPE } from "@/components/library/CategoryTree";
import { RepoAvatar } from "@/components/library/RepoAvatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatCount, formatRelative, languageColor } from "@/lib/format";
import { SNOOZE_OPTIONS } from "@/lib/review";
import { setRepoReview } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { type LibraryLayout, useUiStore } from "@/store/ui";
import type { RepoSummary } from "@/types";

type Props = {
  items: RepoSummary[];
  layout: LibraryLayout;
  selectedId: number | null;
  onSelect: (id: number) => void;
  onEndReached?: () => void;
  emptySlot?: React.ReactNode;
  reviewMode?: boolean;
};

function repoNameParts(fullName: string): { owner: string; name: string } {
  const slash = fullName.indexOf("/");
  if (slash <= 0) {
    return { owner: fullName, name: fullName };
  }
  return {
    owner: fullName.slice(0, slash),
    name: fullName.slice(slash + 1),
  };
}

function ReviewActions({
  repoId,
  htmlUrl,
}: {
  repoId: number;
  htmlUrl?: string;
}) {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: setRepoReview,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
      void queryClient.invalidateQueries({ queryKey: ["repo"] });
      void queryClient.invalidateQueries({ queryKey: ["reviewCounts"] });
      void queryClient.invalidateQueries({ queryKey: ["facets"] });
    },
  });

  return (
    <div className="flex flex-wrap items-center gap-1">
      {htmlUrl ? (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="h-7 px-2 text-[11px]"
          onClick={(e) => {
            e.stopPropagation();
            void openUrl(htmlUrl);
          }}
        >
          <ExternalLink className="size-3" />
          GitHub
        </Button>
      ) : null}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="h-7 px-2 text-[11px]"
        disabled={mutation.isPending}
        onClick={(e) => {
          e.stopPropagation();
          mutation.mutate({ repoId, reviewed: true, snoozeDays: null });
        }}
      >
        Reviewed
      </Button>
      <Select
        onValueChange={(v) => {
          const days = Number(v);
          if (Number.isFinite(days)) {
            mutation.mutate({ repoId, reviewed: null, snoozeDays: days });
          }
        }}
      >
        <SelectTrigger
          className="h-7 w-[6.5rem] px-2 text-[11px]"
          aria-label="Snooze review"
          onClick={(e) => e.stopPropagation()}
        >
          <SelectValue placeholder="Snooze" />
        </SelectTrigger>
        <SelectContent>
          {SNOOZE_OPTIONS.map((opt) => (
            <SelectItem key={opt.days} value={String(opt.days)}>
              {opt.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function RepoRow({
  repo,
  selected,
  onSelect,
  compact,
  reviewMode,
  posinset,
  setsize,
}: {
  repo: RepoSummary;
  selected: boolean;
  onSelect: () => void;
  compact?: boolean;
  reviewMode?: boolean;
  posinset: number;
  setsize: number;
}) {
  const { owner, name } = repoNameParts(repo.fullName);
  const topics = repo.topics.slice(0, compact ? 2 : 4);
  const relevance =
    typeof repo.relevance === "number" && Number.isFinite(repo.relevance)
      ? Math.round(repo.relevance)
      : null;
  const setTopic = useUiStore((s) => s.setTopic);
  const setLanguage = useUiStore((s) => s.setLanguage);

  return (
    <div
      id={`repo-${repo.id}`}
      role="option"
      aria-selected={selected}
      aria-posinset={posinset}
      aria-setsize={setsize}
      aria-label={`${repo.fullName}${repo.description ? `, ${repo.description}` : ""}`}
      tabIndex={-1}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(REPO_DND_TYPE, String(repo.id));
        e.dataTransfer.effectAllowed = "move";
      }}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        "flex w-full cursor-grab gap-3 border-b border-border/80 px-4 py-3 text-left outline-none active:cursor-grabbing",
        selected
          ? "bg-accent ring-1 ring-inset ring-ring/40"
          : "hover:bg-muted/40",
        compact &&
          "rounded-xl border border-border bg-card px-3 py-3 shadow-none",
      )}
    >
      <div className="flex min-w-0 flex-1 gap-3 text-left">
        <RepoAvatar fullName={repo.fullName} size={compact ? 36 : 40} />

        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold tracking-tight">
                <span className="text-foreground/70">{owner}</span>
                <span className="text-foreground/70"> / </span>
                <span>{name}</span>
              </p>
              {repo.description ? (
                <p className="mt-0.5 line-clamp-1 text-sm text-foreground/70">
                  {repo.description}
                </p>
              ) : null}
            </div>
            <div className="flex shrink-0 flex-col items-end gap-1 text-xs text-foreground/70">
              <span className="inline-flex items-center gap-1 font-medium text-foreground">
                <Star className="size-3.5 fill-amber-400 text-amber-400" />
                {formatCount(repo.starsCount)}
              </span>
              {!compact ? (
                <>
                  <span>Updated {formatRelative(repo.pushedAt)}</span>
                  <span>Starred {formatRelative(repo.starredAt)}</span>
                </>
              ) : null}
            </div>
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            {repo.reviewReason ? (
              <Badge
                variant="secondary"
                className="rounded-full px-2 py-0.5 font-normal text-[11px]"
                title={repo.reviewReason}
              >
                {repo.reviewReason}
              </Badge>
            ) : null}
            {relevance != null ? (
              <Badge
                variant="secondary"
                className="rounded-full px-2 py-0.5 font-normal text-[11px] text-foreground/70"
                title="Relevance vs top result"
              >
                {relevance}%
              </Badge>
            ) : null}
            {repo.language ? (
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md bg-muted/80 px-2 py-0.5 text-[11px] text-foreground"
                onClick={(e) => {
                  e.stopPropagation();
                  setLanguage(repo.language);
                }}
              >
                <span
                  className="size-2 rounded-full"
                  style={{ backgroundColor: languageColor(repo.language) }}
                />
                {repo.language}
              </button>
            ) : null}
            {topics.map((t) => (
              <button
                key={t}
                type="button"
                className="rounded-md bg-muted/60 px-2 py-0.5 text-[11px] text-foreground/75 hover:bg-muted"
                onClick={(e) => {
                  e.stopPropagation();
                  setTopic(t);
                }}
              >
                {t}
              </button>
            ))}
            {repo.archived ? (
              <span className="rounded-md bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-900">
                archived
              </span>
            ) : null}
            {repo.unstarred ? (
              <span className="rounded-md bg-rose-500/10 px-2 py-0.5 text-[11px] text-rose-900">
                unstarred
              </span>
            ) : null}
            {compact ? (
              <span className="text-[11px] text-foreground/70">
                starred {formatRelative(repo.starredAt)}
              </span>
            ) : null}
          </div>
        </div>
      </div>
      {reviewMode ? (
        <div className="flex shrink-0 items-start">
          <ReviewActions
            repoId={repo.id}
            htmlUrl={`https://github.com/${repo.fullName}`}
          />
        </div>
      ) : null}
    </div>
  );
}

function useGridColumns(
  parentRef: React.RefObject<HTMLDivElement | null>,
  isGrid: boolean,
): number {
  const [columns, setColumns] = useState(isGrid ? 2 : 1);

  useEffect(() => {
    if (!isGrid) {
      setColumns(1);
      return;
    }
    const el = parentRef.current;
    if (!el || typeof ResizeObserver === "undefined") {
      setColumns(2);
      return;
    }
    const apply = (width: number) => {
      if (width < 560) {
        setColumns(1);
      } else if (width < 900) {
        setColumns(2);
      } else {
        setColumns(3);
      }
    };
    apply(el.clientWidth);
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width ?? el.clientWidth;
      apply(width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [isGrid, parentRef]);

  return isGrid ? columns : 1;
}

export function RepoVirtualList({
  items,
  layout,
  selectedId,
  onSelect,
  onEndReached,
  emptySlot,
  reviewMode,
}: Props) {
  const parentRef = useRef<HTMLDivElement>(null);
  const isGrid = layout === "grid";
  const columns = useGridColumns(parentRef, isGrid);
  const rowCount = Math.ceil(items.length / columns);
  const estimate = isGrid ? 132 : reviewMode ? 136 : 104;

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimate,
    overscan: 8,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const lastVirtual = virtualItems[virtualItems.length - 1];
  useEffect(() => {
    if (!onEndReached || lastVirtual == null) {
      return;
    }
    if (lastVirtual.index >= rowCount - 2) {
      onEndReached();
    }
  }, [lastVirtual, onEndReached, rowCount]);

  useEffect(() => {
    if (selectedId == null) {
      return;
    }
    const itemIndex = items.findIndex((r) => r.id === selectedId);
    if (itemIndex < 0) {
      return;
    }
    const rowIndex = Math.floor(itemIndex / columns);
    virtualizer.scrollToIndex(rowIndex, { align: "auto" });
  }, [selectedId, items, columns, virtualizer]);

  return (
    <div
      id="library-repo-listbox"
      ref={parentRef}
      role="listbox"
      aria-label="Starred repositories"
      aria-activedescendant={
        selectedId != null ? `repo-${selectedId}` : undefined
      }
      tabIndex={0}
      className="h-full min-h-0 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring/50"
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const start = virtualRow.index * columns;
          const slice = items.slice(start, start + columns);
          return (
            <div
              key={virtualRow.key}
              data-index={virtualRow.index}
              ref={virtualizer.measureElement}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${virtualRow.start}px)`,
              }}
              className={cn(
                isGrid &&
                  cn(
                    "grid gap-2 p-2",
                    columns === 1 && "grid-cols-1",
                    columns === 2 && "grid-cols-2",
                    columns >= 3 && "grid-cols-3",
                  ),
              )}
            >
              {slice.map((repo, offset) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  selected={selectedId === repo.id}
                  onSelect={() => onSelect(repo.id)}
                  compact={isGrid}
                  reviewMode={reviewMode}
                  posinset={start + offset + 1}
                  setsize={items.length}
                />
              ))}
            </div>
          );
        })}
      </div>
      {items.length === 0 ? emptySlot : null}
    </div>
  );
}
