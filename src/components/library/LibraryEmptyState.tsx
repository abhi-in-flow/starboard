import { Button } from "@/components/ui/button";
import {
  type EmptyStateAction,
  type EmptyStateKind,
  emptyStateCopy,
} from "@/lib/libraryEmpty";

type Props = {
  kind: EmptyStateKind;
  reviewLabel?: string;
  errorDetail?: string | null;
  onClearFilters: () => void;
  onSync: () => void;
  onRetry: () => void;
  onSettings: () => void;
  syncDisabled?: boolean;
};

export function LibraryEmptyState({
  kind,
  reviewLabel,
  errorDetail,
  onClearFilters,
  onSync,
  onRetry,
  onSettings,
  syncDisabled,
}: Props) {
  const copy = emptyStateCopy(kind, reviewLabel);
  if (!copy.title) {
    return null;
  }

  function run(action: EmptyStateAction) {
    if (action === "clear-filters") {
      onClearFilters();
      return;
    }
    if (action === "sync") {
      onSync();
      return;
    }
    if (action === "retry") {
      onRetry();
      return;
    }
    onSettings();
  }

  function label(action: EmptyStateAction): string {
    if (action === "clear-filters") {
      return "Clear filters";
    }
    if (action === "sync") {
      return "Sync";
    }
    if (action === "retry") {
      return "Retry";
    }
    return "Settings";
  }

  return (
    <div className="flex h-full min-h-40 flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="max-w-md space-y-1">
        <p className="text-sm font-medium text-foreground">{copy.title}</p>
        <p className="text-sm text-muted-foreground">{copy.body}</p>
        {errorDetail ? (
          <p className="text-xs text-destructive" role="alert">
            {errorDetail}
          </p>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center justify-center gap-2">
        {copy.primary ? (
          <Button
            type="button"
            size="sm"
            disabled={copy.primary === "sync" && syncDisabled}
            onClick={() => run(copy.primary as EmptyStateAction)}
          >
            {label(copy.primary)}
          </Button>
        ) : null}
        {copy.secondary ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => run(copy.secondary as EmptyStateAction)}
          >
            {label(copy.secondary)}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
