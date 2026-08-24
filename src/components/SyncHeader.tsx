import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  cancelReadmeQueue,
  cancelSync,
  getAuthStatus,
  getSyncStatus,
  resumeReadmeQueue,
  startSync,
} from "@/lib/tauri";
import type { AppError, SyncProgress } from "@/types";

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as AppError).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "Sync failed";
}

function formatRelative(iso: string | null | undefined): string {
  if (!iso) {
    return "Never synced";
  }
  const then = Date.parse(iso);
  if (Number.isNaN(then)) {
    return "Never synced";
  }
  const seconds = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (seconds < 60) {
    return "Last synced just now";
  }
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    return `Last synced ${m}m ago`;
  }
  if (seconds < 86400) {
    const h = Math.floor(seconds / 3600);
    return `Last synced ${h}h ago`;
  }
  const d = Math.floor(seconds / 86400);
  return `Last synced ${d}d ago`;
}

export function SyncHeader() {
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);

  const authQuery = useQuery({
    queryKey: ["authStatus"],
    queryFn: getAuthStatus,
  });

  const syncQuery = useQuery({
    queryKey: ["syncStatus"],
    queryFn: getSyncStatus,
    refetchInterval: (query) =>
      query.state.data?.running || query.state.data?.readmeRunning
        ? 1000
        : 30_000,
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<SyncProgress>("sync://progress", (event) => {
      const payload = event.payload;
      setProgress(payload);
      if (payload.error) {
        setSyncError(payload.error);
      } else if (
        payload.kind === "readme" &&
        !payload.message.includes("paused")
      ) {
        setSyncError(null);
      }
      if (
        payload.kind === "readme" &&
        (payload.message.startsWith("README queue complete") ||
          payload.error != null)
      ) {
        void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [queryClient]);

  const syncMutation = useMutation({
    mutationFn: (full: boolean) => startSync(full),
    onMutate: () => {
      setSyncError(null);
      setProgress({
        kind: "incremental",
        current: 0,
        total: 0,
        message: "Starting sync…",
        error: null,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
    },
    onError: (err) => {
      setProgress(null);
      setSyncError(errorMessage(err));
      void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
    },
  });

  const resumeMutation = useMutation({
    mutationFn: resumeReadmeQueue,
    onMutate: () => {
      setSyncError(null);
      setProgress({
        kind: "readme",
        current: 0,
        total: syncQuery.data?.pendingReadmes ?? 0,
        message: "Resuming README queue…",
        error: null,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
    },
    onError: (err) => {
      setSyncError(errorMessage(err));
      void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
    },
  });

  const connected = authQuery.data?.connected === true;
  const listSyncing =
    syncMutation.isPending || syncQuery.data?.running === true;
  const readmeRunning =
    syncQuery.data?.readmeRunning === true ||
    (progress?.kind === "readme" &&
      !progress.error &&
      !progress.message.startsWith("README queue complete"));
  const pendingReadmes = syncQuery.data?.pendingReadmes ?? 0;
  const showResume =
    connected && !listSyncing && !readmeRunning && pendingReadmes > 0;
  const showProgress =
    (listSyncing || readmeRunning) &&
    !(
      progress?.kind === "readme" &&
      progress.message.startsWith("README queue complete")
    );
  const pct =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.current / progress.total) * 100))
      : null;

  const statusLine = (() => {
    if (progress?.message) {
      return progress.message;
    }
    if (readmeRunning) {
      return "Fetching README excerpts in background…";
    }
    if (pendingReadmes > 0) {
      return `${pendingReadmes} README excerpts pending`;
    }
    return null;
  })();

  return (
    <header className="flex flex-col gap-2 border-b border-border px-6 py-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="text-sm text-muted-foreground">
            {formatRelative(syncQuery.data?.lastSyncedAt)}
          </p>
          {statusLine ? (
            <p
              className="truncate text-xs text-muted-foreground"
              title={statusLine}
            >
              {statusLine}
              {pct !== null ? ` (${pct}%)` : ""}
            </p>
          ) : null}
          {syncQuery.data?.unstarPolicy ? (
            <p
              className="truncate text-xs text-muted-foreground"
              title={syncQuery.data.unstarPolicy}
            >
              {syncQuery.data.reconcileDue
                ? "Next Sync will run a full reconcile to detect unstars."
                : "Incremental Sync does not detect unstars — use Full sync, or wait for the automatic 7-day reconcile."}
            </p>
          ) : null}
          {syncError ? (
            <p
              className="text-xs text-destructive"
              role="alert"
              title={syncError}
            >
              {syncError}
            </p>
          ) : null}
        </div>
        <div className="flex shrink-0 gap-2">
          {listSyncing || readmeRunning ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                if (listSyncing) {
                  void cancelSync().then(() =>
                    queryClient.invalidateQueries({ queryKey: ["syncStatus"] }),
                  );
                } else {
                  void cancelReadmeQueue().then(() =>
                    queryClient.invalidateQueries({ queryKey: ["syncStatus"] }),
                  );
                }
              }}
            >
              Cancel
            </Button>
          ) : null}
          {showResume ? (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={resumeMutation.isPending}
              onClick={() => resumeMutation.mutate()}
            >
              Resume READMEs ({pendingReadmes})
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!connected || listSyncing}
            onClick={() => syncMutation.mutate(true)}
          >
            Full sync
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!connected || listSyncing}
            onClick={() => syncMutation.mutate(false)}
          >
            {listSyncing ? "Syncing…" : "Sync"}
          </Button>
        </div>
      </div>
      {showProgress ? (
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            className="h-full bg-primary transition-all duration-300"
            style={{
              width: pct === null ? "35%" : `${Math.max(pct, 2)}%`,
              ...(pct === null
                ? { animation: "pulse 1.2s ease-in-out infinite" }
                : {}),
            }}
          />
        </div>
      ) : null}
      {!connected ? (
        <p className="text-xs text-muted-foreground">
          Connect a GitHub token in Settings to sync starred repos.
        </p>
      ) : null}
    </header>
  );
}
