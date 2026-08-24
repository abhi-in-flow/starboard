import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, Loader2, X } from "lucide-react";
import { lazy, Suspense } from "react";
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
import { Separator } from "@/components/ui/separator";
import { flattenCategories } from "@/lib/categories";
import { formatCount, formatRelative, languageColor } from "@/lib/format";
import { SNOOZE_OPTIONS } from "@/lib/review";
import { ollamaAvailabilityLabel } from "@/lib/statusCopy";
import {
  getOllamaStatus,
  getRepo,
  listCategories,
  recategorizeRepo,
  setRepoCategory,
  setRepoReview,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";

const MarkdownExcerpt = lazy(
  () => import("@/components/library/MarkdownExcerpt"),
);

type Props = {
  repoId: number;
  variant?: "docked" | "overlay";
};

export function RepoDetailPanel({ repoId, variant = "docked" }: Props) {
  const queryClient = useQueryClient();
  const setSelectedRepoId = useUiStore((s) => s.setSelectedRepoId);
  const setTopic = useUiStore((s) => s.setTopic);
  const setLanguage = useUiStore((s) => s.setLanguage);

  const detail = useQuery({
    queryKey: ["repo", repoId],
    queryFn: () => getRepo(repoId),
  });

  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const ollama = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
    refetchInterval: 30_000,
  });

  const setCategory = useMutation({
    mutationFn: (categoryId: number) => setRepoCategory(repoId, categoryId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["repo", repoId] });
      void queryClient.invalidateQueries({ queryKey: ["categories"] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
    },
  });

  const recategorize = useMutation({
    mutationFn: () => recategorizeRepo(repoId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["repo", repoId] });
      void queryClient.invalidateQueries({ queryKey: ["categories"] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
    },
  });

  const reviewMutation = useMutation({
    mutationFn: setRepoReview,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["repo", repoId] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
      void queryClient.invalidateQueries({ queryKey: ["reviewCounts"] });
      void queryClient.invalidateQueries({ queryKey: ["facets"] });
    },
  });

  const repo = detail.data;
  const flatCats = flattenCategories(categories.data ?? []);
  const offline = ollama.data != null && !ollama.data.available;
  const isManual = repo?.categorySource === "manual";
  const overlay = variant === "overlay";

  const PanelTag = overlay ? "div" : "aside";
  const panel = (
    <PanelTag
      className={cn(
        "flex min-h-0 flex-col border-border bg-background",
        overlay
          ? "h-full w-full max-w-lg border-l shadow-none"
          : "h-full w-[min(380px,100%)] shrink-0 border-l",
      )}
      role={overlay ? "dialog" : "complementary"}
      aria-modal={overlay ? true : undefined}
      aria-label="Repository details"
    >
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <p className="text-sm font-medium">Details</p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0"
          onClick={() => setSelectedRepoId(null)}
          aria-label="Close detail"
          autoFocus={overlay}
        >
          <X className="size-4" />
        </Button>
      </div>

      <div className="flex-1 overflow-auto px-4 py-4">
        {detail.isLoading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : detail.isError ? (
          <p className="text-sm text-destructive">Failed to load repository.</p>
        ) : repo ? (
          <div className="flex flex-col gap-4">
            <div>
              <div className="flex items-start gap-3">
                <RepoAvatar fullName={repo.fullName} size={48} />
                <div className="min-w-0">
                  <h2 className="text-lg font-semibold tracking-tight">
                    {repo.fullName}
                  </h2>
                  {repo.description ? (
                    <p className="mt-1 text-sm text-muted-foreground">
                      {repo.description}
                    </p>
                  ) : null}
                </div>
              </div>
              <div className="mt-2 flex flex-wrap gap-2">
                {repo.archived ? (
                  <Badge variant="secondary">Archived</Badge>
                ) : null}
                {repo.fork ? <Badge variant="outline">Fork</Badge> : null}
                {repo.unstarred ? (
                  <Badge variant="destructive">Unstarred</Badge>
                ) : null}
                {repo.license ? (
                  <Badge variant="outline">{repo.license}</Badge>
                ) : null}
              </div>
            </div>

            <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
              <Stat label="Stars" value={formatCount(repo.starsCount)} />
              <Stat label="Forks" value={formatCount(repo.forksCount)} />
              <Stat label="Open issues" value={formatCount(repo.openIssues)} />
              <Stat
                label="Language"
                value={
                  repo.language ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 hover:underline"
                      onClick={() => setLanguage(repo.language)}
                    >
                      <span
                        className="size-2.5 rounded-full"
                        style={{
                          backgroundColor: languageColor(repo.language),
                        }}
                      />
                      {repo.language}
                    </button>
                  ) : (
                    "—"
                  )
                }
              />
              <Stat label="Starred" value={formatRelative(repo.starredAt)} />
              <Stat label="Pushed" value={formatRelative(repo.pushedAt)} />
              <Stat
                label="Created"
                value={formatRelative(repo.repoCreatedAt)}
              />
              <Stat label="Fetched" value={formatRelative(repo.fetchedAt)} />
            </div>

            {repo.topics.length > 0 ? (
              <div>
                <p className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Topics
                </p>
                <div className="flex flex-wrap gap-1.5">
                  {repo.topics.map((t) => (
                    <button key={t} type="button" onClick={() => setTopic(t)}>
                      <Badge
                        variant="outline"
                        className="cursor-pointer font-normal"
                      >
                        {t}
                      </Badge>
                    </button>
                  ))}
                </div>
              </div>
            ) : null}

            <div>
              <p className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Category
              </p>
              <div className="flex flex-col gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  {repo.categoryNames.length > 0 ? (
                    repo.categoryNames.map((c) => (
                      <Badge key={c} variant="secondary">
                        {c}
                      </Badge>
                    ))
                  ) : (
                    <span className="text-sm text-muted-foreground">
                      Uncategorized
                    </span>
                  )}
                  {repo.categorySource ? (
                    <Badge variant="outline">
                      {repo.categorySource === "manual" ? "Manual" : "LLM"}
                    </Badge>
                  ) : null}
                </div>
                {flatCats.length > 0 ? (
                  <Select
                    value={
                      repo.categoryId != null ? String(repo.categoryId) : ""
                    }
                    onValueChange={(v) => {
                      const id = Number(v);
                      if (Number.isFinite(id)) {
                        setCategory.mutate(id);
                      }
                    }}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="Set category manually…" />
                    </SelectTrigger>
                    <SelectContent>
                      {flatCats.map((c) => (
                        <SelectItem key={c.id} value={String(c.id)}>
                          {c.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Commit a taxonomy in Categories first.
                  </p>
                )}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-fit gap-1"
                  disabled={
                    offline ||
                    isManual ||
                    flatCats.length === 0 ||
                    recategorize.isPending
                  }
                  title={
                    isManual
                      ? "Manual override — clear via picker change only"
                      : offline
                        ? ollamaAvailabilityLabel(false)
                        : undefined
                  }
                  onClick={() => recategorize.mutate()}
                >
                  {recategorize.isPending ? (
                    <Loader2 className="size-3 animate-spin" />
                  ) : null}
                  Re-categorize with LLM
                </Button>
                {recategorize.isError ? (
                  <p className="text-xs text-destructive">
                    {(recategorize.error as { message?: string })?.message ??
                      "Re-categorize failed"}
                  </p>
                ) : null}
              </div>
            </div>

            <div>
              <p className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Review
              </p>
              <p className="mb-2 text-xs text-muted-foreground">
                Local only — never stars or unstars on GitHub.
              </p>
              <div className="flex flex-wrap items-center gap-2">
                {repo.reviewedAt ? (
                  <Badge variant="secondary">
                    Reviewed {formatRelative(repo.reviewedAt)}
                  </Badge>
                ) : null}
                {repo.snoozedUntil ? (
                  <Badge variant="outline">
                    Snoozed until {repo.snoozedUntil.slice(0, 10)}
                  </Badge>
                ) : null}
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                {repo.reviewedAt ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={reviewMutation.isPending}
                    onClick={() =>
                      reviewMutation.mutate({
                        repoId,
                        reviewed: false,
                        snoozeDays: null,
                      })
                    }
                  >
                    Undo reviewed
                  </Button>
                ) : (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={reviewMutation.isPending}
                    onClick={() =>
                      reviewMutation.mutate({
                        repoId,
                        reviewed: true,
                        snoozeDays: null,
                      })
                    }
                  >
                    Mark reviewed
                  </Button>
                )}
                {repo.snoozedUntil ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={reviewMutation.isPending}
                    onClick={() =>
                      reviewMutation.mutate({
                        repoId,
                        reviewed: null,
                        snoozeDays: 0,
                      })
                    }
                  >
                    Clear snooze
                  </Button>
                ) : (
                  <Select
                    onValueChange={(v) => {
                      const days = Number(v);
                      if (Number.isFinite(days)) {
                        reviewMutation.mutate({
                          repoId,
                          reviewed: null,
                          snoozeDays: days,
                        });
                      }
                    }}
                  >
                    <SelectTrigger
                      className="h-8 w-36"
                      aria-label="Snooze review"
                    >
                      <SelectValue placeholder="Snooze…" />
                    </SelectTrigger>
                    <SelectContent>
                      {SNOOZE_OPTIONS.map((opt) => (
                        <SelectItem key={opt.days} value={String(opt.days)}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              </div>
            </div>

            <Separator />

            <div>
              <p className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                README excerpt
              </p>
              {repo.readmeExcerpt == null ? (
                <p className="text-sm text-muted-foreground">
                  Not fetched yet — still in the README queue.
                </p>
              ) : repo.readmeExcerpt === "" ? (
                <p className="text-sm text-muted-foreground">
                  No README available for this repository.
                </p>
              ) : (
                <Suspense
                  fallback={
                    <p className="text-sm text-muted-foreground">
                      Loading README…
                    </p>
                  }
                >
                  <MarkdownExcerpt markdown={repo.readmeExcerpt} />
                </Suspense>
              )}
            </div>

            <div className="flex flex-col gap-2">
              <Button
                type="button"
                className="w-full gap-2"
                onClick={() => void openUrl(repo.htmlUrl)}
              >
                <ExternalLink className="size-4" />
                Open on GitHub
              </Button>
              {repo.homepage ? (
                <Button
                  type="button"
                  variant="outline"
                  className="w-full gap-2"
                  onClick={() => void openUrl(repo.homepage as string)}
                >
                  Homepage
                </Button>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>
    </PanelTag>
  );

  if (!overlay) {
    return panel;
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      <button
        type="button"
        className="absolute inset-0 bg-foreground/30"
        aria-label="Close detail"
        onClick={() => setSelectedRepoId(null)}
      />
      <div className="relative flex h-full w-full max-w-lg">{panel}</div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div>
      <p className="text-xs text-muted-foreground">{label}</p>
      <div className="font-medium">{value}</div>
    </div>
  );
}
