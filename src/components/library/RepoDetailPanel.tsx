import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, X } from "lucide-react";
import { MarkdownExcerpt } from "@/components/library/MarkdownExcerpt";
import { RepoAvatar } from "@/components/library/RepoAvatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { formatCount, formatRelative, languageColor } from "@/lib/format";
import { getRepo } from "@/lib/tauri";
import { useUiStore } from "@/store/ui";

type Props = {
  repoId: number;
};

export function RepoDetailPanel({ repoId }: Props) {
  const setSelectedRepoId = useUiStore((s) => s.setSelectedRepoId);
  const setTopic = useUiStore((s) => s.setTopic);
  const setLanguage = useUiStore((s) => s.setLanguage);

  const detail = useQuery({
    queryKey: ["repo", repoId],
    queryFn: () => getRepo(repoId),
  });

  const repo = detail.data;

  return (
    <aside className="flex h-full w-[380px] shrink-0 flex-col border-l border-border bg-background">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <p className="text-sm font-medium">Details</p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0"
          onClick={() => setSelectedRepoId(null)}
          aria-label="Close detail"
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

            <div className="grid grid-cols-2 gap-3 text-sm">
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

            {repo.categoryNames.length > 0 ? (
              <div>
                <p className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Categories
                </p>
                <div className="flex flex-wrap gap-1.5">
                  {repo.categoryNames.map((c) => (
                    <Badge key={c} variant="secondary">
                      {c}
                    </Badge>
                  ))}
                </div>
              </div>
            ) : null}

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
                <MarkdownExcerpt markdown={repo.readmeExcerpt} />
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
    </aside>
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
