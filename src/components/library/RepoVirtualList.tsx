import { useVirtualizer } from "@tanstack/react-virtual";
import { Star } from "lucide-react";
import { useRef } from "react";
import { REPO_DND_TYPE } from "@/components/library/CategoryTree";
import { RepoAvatar } from "@/components/library/RepoAvatar";
import { formatCount, formatRelative, languageColor } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { LibraryLayout } from "@/store/ui";
import type { RepoSummary } from "@/types";

type Props = {
  items: RepoSummary[];
  layout: LibraryLayout;
  selectedId: number | null;
  onSelect: (id: number) => void;
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

function RepoRow({
  repo,
  selected,
  onSelect,
  compact,
}: {
  repo: RepoSummary;
  selected: boolean;
  onSelect: () => void;
  compact?: boolean;
}) {
  const { owner, name } = repoNameParts(repo.fullName);
  const topics = repo.topics.slice(0, compact ? 2 : 4);

  return (
    <button
      type="button"
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(REPO_DND_TYPE, String(repo.id));
        e.dataTransfer.effectAllowed = "move";
      }}
      onClick={onSelect}
      className={cn(
        "flex w-full cursor-grab gap-3 border-b border-border/80 px-4 py-3 text-left transition-colors active:cursor-grabbing",
        selected ? "bg-accent" : "hover:bg-muted/40",
        compact &&
          "rounded-xl border border-border bg-card px-3 py-3 shadow-sm",
      )}
    >
      <RepoAvatar fullName={repo.fullName} size={compact ? 36 : 40} />

      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold tracking-tight">
              <span className="text-muted-foreground">{owner}</span>
              <span className="text-muted-foreground"> / </span>
              <span>{name}</span>
            </p>
            {repo.description ? (
              <p className="mt-0.5 line-clamp-1 text-sm text-muted-foreground">
                {repo.description}
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 flex-col items-end gap-1 text-xs text-muted-foreground">
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
          {repo.language ? (
            <span className="inline-flex items-center gap-1.5 rounded-md bg-muted/80 px-2 py-0.5 text-[11px] text-foreground">
              <span
                className="size-2 rounded-full"
                style={{ backgroundColor: languageColor(repo.language) }}
              />
              {repo.language}
            </span>
          ) : null}
          {topics.map((t) => (
            <span
              key={t}
              className="rounded-md bg-muted/60 px-2 py-0.5 text-[11px] text-muted-foreground"
            >
              {t}
            </span>
          ))}
          {repo.archived ? (
            <span className="rounded-md bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-800">
              archived
            </span>
          ) : null}
          {repo.unstarred ? (
            <span className="rounded-md bg-rose-500/10 px-2 py-0.5 text-[11px] text-rose-800">
              unstarred
            </span>
          ) : null}
          {compact ? (
            <span className="text-[11px] text-muted-foreground">
              starred {formatRelative(repo.starredAt)}
            </span>
          ) : null}
        </div>
      </div>
    </button>
  );
}

export function RepoVirtualList({
  items,
  layout,
  selectedId,
  onSelect,
}: Props) {
  const parentRef = useRef<HTMLDivElement>(null);
  const isGrid = layout === "grid";
  const columns = isGrid ? 2 : 1;
  const rowCount = Math.ceil(items.length / columns);
  const estimate = isGrid ? 132 : 104;

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimate,
    overscan: 8,
  });

  return (
    <div ref={parentRef} className="h-full overflow-auto">
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
              className={cn(isGrid && "grid grid-cols-2 gap-2 p-2")}
            >
              {slice.map((repo) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  selected={selectedId === repo.id}
                  onSelect={() => onSelect(repo.id)}
                  compact={isGrid}
                />
              ))}
            </div>
          );
        })}
      </div>
      {items.length === 0 ? (
        <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
          No repositories match your filters.
        </div>
      ) : null}
    </div>
  );
}
