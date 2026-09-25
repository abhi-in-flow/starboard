import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, Check, Circle, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  canDismissOnboarding,
  type DerivedSetupStep,
  deriveSetupSteps,
  nextActionableStep,
  requiredStepsComplete,
  type SetupSurface,
  setupTruthFromStatus,
} from "@/lib/onboarding";
import {
  getEmbedStatus,
  getOllamaStatus,
  getSetupStatus,
  getSyncStatus,
  setOnboardingCompleted,
  startEmbedding,
  startSync,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type { AppError } from "@/types";

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as AppError).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "Something went wrong";
}

type Props = {
  variant: Exclude<SetupSurface, "hidden">;
};

export function OnboardingGuide({ variant }: Props) {
  const queryClient = useQueryClient();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const openSettings = useUiStore((s) => s.openSettings);
  const openCategories = useUiStore((s) => s.openCategories);

  const setupQuery = useQuery({
    queryKey: ["setupStatus"],
    queryFn: getSetupStatus,
  });

  const ollamaQuery = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
    refetchInterval: 15_000,
  });

  const syncQuery = useQuery({
    queryKey: ["syncStatus"],
    queryFn: getSyncStatus,
    refetchInterval: (query) => (query.state.data?.running ? 1000 : false),
  });

  const embedQuery = useQuery({
    queryKey: ["embedStatus"],
    queryFn: getEmbedStatus,
    refetchInterval: (query) => (query.state.data?.running ? 1000 : false),
  });

  useEffect(() => {
    headingRef.current?.focus();
  }, []);

  const dismissMutation = useMutation({
    mutationFn: () => setOnboardingCompleted(true),
    onSuccess: (status) => {
      queryClient.setQueryData(["setupStatus"], status);
    },
  });

  const syncMutation = useMutation({
    mutationFn: () => startSync(false),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["syncStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
    },
  });

  const embedMutation = useMutation({
    mutationFn: startEmbedding,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
  });

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Escape") {
        return;
      }
      const target = e.target as HTMLElement | null;
      const inside = target?.closest("[data-onboarding-guide]");
      if (variant === "full" || inside) {
        e.preventDefault();
        e.stopPropagation();
        dismissMutation.mutate();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [dismissMutation, variant]);

  const status = setupQuery.data;
  const steps = useMemo(() => {
    if (!status) {
      return [];
    }
    return deriveSetupSteps(setupTruthFromStatus(status, ollamaQuery.data));
  }, [status, ollamaQuery.data]);

  const next = nextActionableStep(steps);
  const requiredDone = requiredStepsComplete(steps);
  const syncing = syncMutation.isPending || syncQuery.data?.running === true;
  const embedding =
    embedMutation.isPending || embedQuery.data?.running === true;
  const ollamaOffline = ollamaQuery.data != null && !ollamaQuery.data.available;

  function runStep(step: DerivedSetupStep) {
    switch (step.id) {
      case "github":
        openSettings("github");
        return;
      case "sync":
        if (step.state === "blocked") {
          openSettings("github");
          return;
        }
        syncMutation.mutate();
        return;
      case "ollama":
        openSettings("ollama");
        return;
      case "taxonomy":
        openCategories("taxonomy");
        return;
      case "assign":
        openCategories("assign");
        return;
      case "embeddings":
        if (step.state === "blocked") {
          return;
        }
        if (ollamaOffline) {
          openSettings("ollama");
          return;
        }
        if (embedQuery.data?.needRebuild) {
          openSettings("ollama");
          return;
        }
        embedMutation.mutate();
        return;
    }
  }

  function actionLabel(step: DerivedSetupStep): string {
    switch (step.id) {
      case "github":
        return step.state === "complete" ? "Manage token" : "Connect";
      case "sync":
        if (step.state === "blocked") {
          return "Connect first";
        }
        if (syncing) {
          return "Syncing…";
        }
        return step.state === "complete" ? "Sync again" : "Sync now";
      case "ollama":
        return "Open Settings";
      case "taxonomy":
        return "Open Categories";
      case "assign":
        return "Open Categories";
      case "embeddings":
        if (step.state === "blocked") {
          return "Sync first";
        }
        if (embedding) {
          return "Embedding…";
        }
        if (ollamaOffline || embedQuery.data?.needRebuild) {
          return "Check Ollama";
        }
        return step.state === "complete" ? "Rebuild" : "Build embeddings";
    }
  }

  function actionDisabled(step: DerivedSetupStep): boolean {
    if (step.id === "sync") {
      return step.state !== "blocked" && syncing;
    }
    if (step.id === "embeddings") {
      return step.state === "blocked" || embedding;
    }
    return false;
  }

  const title =
    variant === "full" ? "Set up Starboard" : "Finish setup when you are ready";
  const description =
    variant === "full"
      ? "Connect GitHub and sync your stars. Ollama, categories, and embeddings are optional — Keyword search works without them."
      : "Optional AI steps stay available from Settings, Categories, and the library toolbar.";

  if (setupQuery.isLoading) {
    return (
      <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
        Loading setup…
      </div>
    );
  }

  if (setupQuery.isError || !status) {
    return (
      <div className="flex flex-col items-start gap-3 p-6">
        <p className="text-sm text-destructive" role="alert">
          Could not load setup status
          {setupQuery.error ? `: ${errorMessage(setupQuery.error)}` : "."}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            onClick={() => void setupQuery.refetch()}
          >
            Retry
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => openSettings("github")}
          >
            Settings
          </Button>
        </div>
      </div>
    );
  }

  return (
    <section
      data-onboarding-guide
      aria-labelledby="onboarding-heading"
      className={cn(
        variant === "full"
          ? "mx-auto flex w-full max-w-2xl flex-col p-8"
          : "p-4",
      )}
    >
      <Card>
        <CardHeader className="gap-3">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <CardTitle>
                <h1
                  id="onboarding-heading"
                  ref={headingRef}
                  tabIndex={-1}
                  className="text-xl font-semibold tracking-tight outline-none"
                >
                  {title}
                </h1>
              </CardTitle>
              <CardDescription className="mt-1">{description}</CardDescription>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => dismissMutation.mutate()}
                disabled={!canDismissOnboarding() || dismissMutation.isPending}
              >
                Skip for now
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Close setup guide"
                onClick={() => dismissMutation.mutate()}
                disabled={!canDismissOnboarding() || dismissMutation.isPending}
              >
                <X className="size-4" />
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <ol className="flex flex-col gap-2">
            {steps.map((step) => {
              const current = next?.id === step.id;
              return (
                <li
                  key={step.id}
                  aria-current={current ? "step" : undefined}
                  className={cn(
                    "flex flex-col gap-2 rounded-lg border border-border px-3 py-3 sm:flex-row sm:items-center sm:justify-between",
                    current && "border-primary/40 bg-muted/40",
                    step.state === "blocked" && "opacity-70",
                  )}
                >
                  <div className="flex min-w-0 items-start gap-3">
                    <StepIcon state={step.state} />
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="text-sm font-medium">{step.title}</p>
                        {step.optional ? (
                          <Badge variant="secondary" className="font-normal">
                            Optional
                          </Badge>
                        ) : null}
                        <span className="text-xs text-muted-foreground">
                          {step.state === "complete"
                            ? "Done"
                            : step.state === "blocked"
                              ? "Waiting"
                              : "To do"}
                        </span>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {step.detail}
                      </p>
                    </div>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant={
                      current && step.state !== "complete"
                        ? "default"
                        : "outline"
                    }
                    className="shrink-0"
                    disabled={actionDisabled(step)}
                    onClick={() => runStep(step)}
                  >
                    {actionLabel(step)}
                  </Button>
                </li>
              );
            })}
          </ol>

          {syncMutation.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {errorMessage(syncMutation.error)}
            </p>
          ) : null}
          {embedMutation.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {errorMessage(embedMutation.error)}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
            <p className="text-xs text-muted-foreground">
              Keyword search works without Ollama. You can reopen this guide
              from Settings → General.
            </p>
            <Button
              type="button"
              size="sm"
              onClick={() => dismissMutation.mutate()}
              disabled={!canDismissOnboarding() || dismissMutation.isPending}
            >
              {requiredDone ? "Finish setup" : "Skip remaining"}
            </Button>
          </div>
        </CardContent>
      </Card>
    </section>
  );
}

function StepIcon({ state }: { state: DerivedSetupStep["state"] }) {
  if (state === "complete") {
    return (
      <Check
        className="mt-0.5 size-4 shrink-0 text-primary"
        aria-hidden="true"
      />
    );
  }
  if (state === "blocked") {
    return (
      <Ban
        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
    );
  }
  return (
    <Circle
      className="mt-0.5 size-4 shrink-0 text-muted-foreground"
      aria-hidden="true"
    />
  );
}
