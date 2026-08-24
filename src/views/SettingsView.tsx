import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { DataBackupCard } from "@/components/settings/DataBackupCard";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { formatRelative } from "@/lib/format";
import {
  checkDbIntegrity,
  connectGithub,
  disconnectGithub,
  getAuthStatus,
  getSettings,
  getSystemStatus,
  rebuildEmbeddingsTable,
  setOnboardingCompleted,
  updateSettings,
} from "@/lib/tauri";
import { usePrefersReducedMotion } from "@/lib/useMediaQuery";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type { AppError, CategoryNode, IntegrityReport } from "@/types";

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as AppError).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "Something went wrong";
}

function CategoryCountList({
  nodes,
  depth = 0,
}: {
  nodes: CategoryNode[];
  depth?: number;
}) {
  if (nodes.length === 0) {
    return <p className="text-sm text-muted-foreground">No categories yet.</p>;
  }
  return (
    <ul className="flex flex-col gap-1 text-sm">
      {nodes.map((node) => (
        <li key={node.id}>
          <div
            className="flex items-center justify-between gap-3"
            style={{ paddingLeft: depth * 12 }}
          >
            <span className="truncate">{node.name}</span>
            <span className="tabular-nums text-muted-foreground">
              {node.count}
            </span>
          </div>
          {node.children.length > 0 ? (
            <CategoryCountList nodes={node.children} depth={depth + 1} />
          ) : null}
        </li>
      ))}
    </ul>
  );
}

function StatusTab() {
  const [integrity, setIntegrity] = useState<IntegrityReport | null>(null);
  const integrityMutation = useMutation({
    mutationFn: checkDbIntegrity,
    onSuccess: setIntegrity,
  });

  const statusQuery = useQuery({
    queryKey: ["systemStatus"],
    queryFn: getSystemStatus,
  });

  if (statusQuery.isLoading) {
    return <p className="text-sm text-muted-foreground">Loading status…</p>;
  }
  if (statusQuery.isError) {
    return (
      <p className="text-sm text-destructive" role="alert">
        Failed to load status: {errorMessage(statusQuery.error)}
      </p>
    );
  }

  const data = statusQuery.data;
  if (!data) {
    return null;
  }

  const { embeddings: emb, categorization: cat, data: dataPanel } = data;
  const coveragePct = Math.round(emb.coverage * 100);

  return (
    <div className="flex flex-col gap-6">
      <DataBackupCard data={dataPanel} />
      <Card>
        <CardHeader>
          <CardTitle>Embeddings</CardTitle>
          <CardDescription>
            Vector coverage for Semantic and Hybrid search.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-sm">
          <div className="grid grid-cols-2 gap-x-4 gap-y-2">
            <span className="text-muted-foreground">Model</span>
            <span className="font-medium">{emb.model || "—"}</span>
            <span className="text-muted-foreground">Dimension</span>
            <span className="font-medium tabular-nums">{emb.dimension}</span>
            <span className="text-muted-foreground">Coverage</span>
            <span className="font-medium tabular-nums">{coveragePct}%</span>
            <span className="text-muted-foreground">Embedded</span>
            <span className="font-medium tabular-nums">
              {emb.embeddedRepos} / {emb.totalRepos}
            </span>
            <span className="text-muted-foreground">Stale</span>
            <span className="font-medium tabular-nums">{emb.staleRepos}</span>
            <span className="text-muted-foreground">Missing</span>
            <span className="font-medium tabular-nums">{emb.missingRepos}</span>
            <span className="text-muted-foreground">Last embed run</span>
            <span className="font-medium">
              {emb.lastEmbedAt
                ? `${formatRelative(emb.lastEmbedAt)} (${emb.lastEmbedAt})`
                : "Never"}
            </span>
          </div>
          {emb.needRebuild ? (
            <p className="rounded-md border border-border bg-muted/40 p-3 text-sm">
              Embeddings table needs rebuild — change dimension is pending. Use
              Rebuild embeddings table on the General tab.
            </p>
          ) : null}
          <p className="text-xs text-muted-foreground">
            To rebuild search embeddings, use Build embeddings from the Library
            toolbar when Ollama is reachable.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Categorization</CardTitle>
          <CardDescription>
            Active (starred) repos and how they were assigned.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 text-sm">
          <div className="grid grid-cols-2 gap-x-4 gap-y-2">
            <span className="text-muted-foreground">Categorized</span>
            <span className="font-medium tabular-nums">
              {cat.categorizedRepos} / {cat.totalRepos}
            </span>
            <span className="text-muted-foreground">Uncategorized</span>
            <span className="font-medium tabular-nums">
              {cat.uncategorizedRepos}
            </span>
            <span className="text-muted-foreground">LLM assignments</span>
            <span className="font-medium tabular-nums">
              {cat.llmAssignments}
            </span>
            <span className="text-muted-foreground">Manual assignments</span>
            <span className="font-medium tabular-nums">
              {cat.manualAssignments}
            </span>
            <span className="text-muted-foreground">
              Auto-categorize after sync
            </span>
            <span className="font-medium">
              {cat.autoCategorizeAfterSync ? "On" : "Off"}
            </span>
          </div>
          <div>
            <p className="mb-2 text-xs font-medium text-muted-foreground">
              Per category
            </p>
            <CategoryCountList nodes={cat.categories} />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Database integrity</CardTitle>
          <CardDescription>
            SQLite integrity_check and foreign-key check. This is not a
            backup/restore.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-sm">
          <Button
            type="button"
            variant="outline"
            className="w-fit"
            disabled={integrityMutation.isPending}
            onClick={() => integrityMutation.mutate()}
          >
            {integrityMutation.isPending ? "Checking…" : "Check database"}
          </Button>
          {integrityMutation.isError ? (
            <p className="text-destructive" role="alert">
              {errorMessage(integrityMutation.error)}
            </p>
          ) : null}
          {integrity ? (
            <div className="grid grid-cols-2 gap-x-4 gap-y-2">
              <span className="text-muted-foreground">Result</span>
              <span className="font-medium">
                {integrity.ok ? "OK" : "Issues found"}
              </span>
              <span className="text-muted-foreground">Integrity</span>
              <span className="font-medium">{integrity.integrity}</span>
              <span className="text-muted-foreground">FK violations</span>
              <span className="font-medium tabular-nums">
                {integrity.foreignKeyViolations}
              </span>
              <span className="text-muted-foreground">Checked</span>
              <span className="font-medium">{integrity.checkedAt}</span>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

export function SettingsView() {
  const queryClient = useQueryClient();
  const tab = useUiStore((s) => s.settingsTab);
  const setTab = useUiStore((s) => s.setSettingsTab);
  const settingsSection = useUiStore((s) => s.settingsSection);
  const setSettingsSection = useUiStore((s) => s.setSettingsSection);
  const setView = useUiStore((s) => s.setView);
  const reduceMotion = usePrefersReducedMotion();
  const [pat, setPat] = useState("");
  const [replacingToken, setReplacingToken] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [chatModel, setChatModel] = useState("");
  const [embedModel, setEmbedModel] = useState("");
  const [embedDimension, setEmbedDimension] = useState("768");
  const [autoCategorize, setAutoCategorize] = useState(true);
  const [settingsSaved, setSettingsSaved] = useState(false);

  const authQuery = useQuery({
    queryKey: ["authStatus"],
    queryFn: getAuthStatus,
  });

  const settingsQuery = useQuery({
    queryKey: ["settings"],
    queryFn: async () => {
      const settings = await getSettings();
      setBaseUrl(settings.ollamaBaseUrl);
      setChatModel(settings.ollamaChatModel);
      setEmbedModel(settings.ollamaEmbedModel);
      setEmbedDimension(String(settings.embedDimension));
      setAutoCategorize(settings.autoCategorizeAfterSync);
      return settings;
    },
  });

  const connectMutation = useMutation({
    mutationFn: connectGithub,
    onSuccess: (status) => {
      setPat("");
      setReplacingToken(false);
      setAuthError(null);
      queryClient.setQueryData(["authStatus"], status);
      void queryClient.invalidateQueries({ queryKey: ["settings"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => {
      setAuthError(errorMessage(err));
    },
  });

  const disconnectMutation = useMutation({
    mutationFn: disconnectGithub,
    onSuccess: (status) => {
      setAuthError(null);
      setReplacingToken(false);
      queryClient.setQueryData(["authStatus"], status);
      void queryClient.invalidateQueries({ queryKey: ["settings"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => {
      setAuthError(errorMessage(err));
    },
  });

  const saveSettingsMutation = useMutation({
    mutationFn: () => {
      const parsed = Number.parseInt(embedDimension, 10);
      return updateSettings({
        ollamaBaseUrl: baseUrl,
        ollamaChatModel: chatModel,
        ollamaEmbedModel: embedModel,
        embedDimension: Number.isFinite(parsed) ? parsed : undefined,
        autoCategorizeAfterSync: autoCategorize,
      });
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      setEmbedDimension(String(settings.embedDimension));
      setSettingsSaved(true);
      window.setTimeout(() => setSettingsSaved(false), 2000);
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
  });

  const rebuildMutation = useMutation({
    mutationFn: () => {
      const parsed = Number.parseInt(embedDimension, 10);
      return rebuildEmbeddingsTable(
        Number.isFinite(parsed) ? parsed : undefined,
      );
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
  });

  const reopenGuideMutation = useMutation({
    mutationFn: () => setOnboardingCompleted(false),
    onSuccess: (status) => {
      queryClient.setQueryData(["setupStatus"], status);
      setView("library");
    },
  });

  useEffect(() => {
    if (!settingsSection) {
      return;
    }
    const id =
      settingsSection === "github" ? "settings-github" : "settings-ollama";
    const section = document.getElementById(id);
    section?.scrollIntoView({
      behavior: reduceMotion ? "auto" : "smooth",
      block: "start",
    });
    if (settingsSection === "github") {
      window.setTimeout(() => {
        document.getElementById("pat")?.focus();
      }, 80);
    } else {
      window.setTimeout(() => {
        document.getElementById("ollama-url")?.focus();
      }, 80);
    }
    setSettingsSection(null);
  }, [settingsSection, setSettingsSection, reduceMotion]);

  const auth = authQuery.data;
  const showPatForm = !auth?.connected || replacingToken;
  const needRebuild = settingsQuery.data?.embeddingsNeedRebuild === true;

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-6 p-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Connect GitHub, choose an Ollama host, and review library health.
        </p>
      </div>

      <fieldset className="m-0 flex w-fit items-center rounded-lg border border-border p-0.5">
        <legend className="sr-only">Settings sections</legend>
        <Button
          type="button"
          size="sm"
          aria-pressed={tab === "general"}
          variant={tab === "general" ? "secondary" : "ghost"}
          className={cn("h-8 px-3 text-xs")}
          onClick={() => setTab("general")}
        >
          General
        </Button>
        <Button
          type="button"
          size="sm"
          aria-pressed={tab === "status"}
          variant={tab === "status" ? "secondary" : "ghost"}
          className={cn("h-8 px-3 text-xs")}
          onClick={() => {
            setTab("status");
            void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
          }}
        >
          Status
        </Button>
      </fieldset>

      {tab === "status" ? (
        <StatusTab />
      ) : (
        <>
          <Card id="settings-github" tabIndex={-1} className="outline-none">
            <CardHeader>
              <CardTitle>GitHub</CardTitle>
              <CardDescription>
                Your personal access token stays in the operating system
                credential store. It is never written to the library database,
                backups, or logs.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              {auth?.connected && auth.username ? (
                <div className="flex flex-col gap-3">
                  <p className="text-sm">
                    Connected as{" "}
                    <span className="font-medium">{auth.username}</span>
                  </p>
                  {!replacingToken ? (
                    <div className="flex gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => {
                          setReplacingToken(true);
                          setAuthError(null);
                          setPat("");
                        }}
                      >
                        Replace token
                      </Button>
                      <Button
                        type="button"
                        variant="destructive"
                        disabled={disconnectMutation.isPending}
                        onClick={() => disconnectMutation.mutate()}
                      >
                        Disconnect
                      </Button>
                    </div>
                  ) : null}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">Not connected</p>
              )}

              {showPatForm ? (
                <form
                  className="flex flex-col gap-3"
                  onSubmit={(e) => {
                    e.preventDefault();
                    setAuthError(null);
                    connectMutation.mutate(pat.trim());
                  }}
                >
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="pat">
                      {auth?.connected
                        ? "New personal access token"
                        : "Personal access token"}
                    </Label>
                    <Input
                      id="pat"
                      type="password"
                      autoComplete="off"
                      placeholder="ghp_…"
                      value={pat}
                      onChange={(e) => setPat(e.target.value)}
                    />
                  </div>
                  {authError ? (
                    <p className="text-sm text-destructive" role="alert">
                      {authError}
                    </p>
                  ) : null}
                  <div className="flex gap-2">
                    <Button
                      type="submit"
                      disabled={
                        connectMutation.isPending || pat.trim().length === 0
                      }
                    >
                      {connectMutation.isPending ? "Validating…" : "Connect"}
                    </Button>
                    {auth?.connected ? (
                      <Button
                        type="button"
                        variant="ghost"
                        onClick={() => {
                          setReplacingToken(false);
                          setPat("");
                          setAuthError(null);
                        }}
                      >
                        Cancel
                      </Button>
                    ) : null}
                  </div>
                </form>
              ) : null}
            </CardContent>
          </Card>

          <Card id="settings-ollama" tabIndex={-1} className="outline-none">
            <CardHeader>
              <CardTitle>Ollama</CardTitle>
              <CardDescription>
                Connect to Ollama on this computer or another machine on your
                local network. Do not expose Ollama on the public internet — it
                is typically unauthenticated.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {settingsQuery.isLoading ? (
                <p className="text-sm text-muted-foreground">Loading…</p>
              ) : (
                <form
                  className="flex flex-col gap-4"
                  onSubmit={(e) => {
                    e.preventDefault();
                    saveSettingsMutation.mutate();
                  }}
                >
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="ollama-url">Base URL</Label>
                    <Input
                      id="ollama-url"
                      value={baseUrl}
                      onChange={(e) => setBaseUrl(e.target.value)}
                      placeholder="http://127.0.0.1:11434"
                      aria-describedby="ollama-url-help"
                    />
                    <p
                      id="ollama-url-help"
                      className="text-xs text-muted-foreground"
                    >
                      A LAN address is supported. Only use hosts you trust —
                      Starboard sends repository text to this URL for
                      categorization and embeddings.
                    </p>
                  </div>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="chat-model">Chat model</Label>
                    <Input
                      id="chat-model"
                      value={chatModel}
                      onChange={(e) => setChatModel(e.target.value)}
                      placeholder="qwen3:14b"
                    />
                  </div>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="embed-model">Embed model</Label>
                    <Input
                      id="embed-model"
                      value={embedModel}
                      onChange={(e) => setEmbedModel(e.target.value)}
                      placeholder="nomic-embed-text"
                    />
                  </div>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="embed-dimension">Embed dimension</Label>
                    <Input
                      id="embed-dimension"
                      type="number"
                      min={1}
                      max={8192}
                      value={embedDimension}
                      onChange={(e) => setEmbedDimension(e.target.value)}
                      placeholder="768"
                    />
                    <p className="text-xs text-muted-foreground">
                      Must match the embedding model output size
                      (nomic-embed-text = 768). Changing this requires
                      rebuilding the search index on the Status or General tabs,
                      then building embeddings from the library.
                    </p>
                  </div>
                  <div className="flex items-start gap-2">
                    <Checkbox
                      id="auto-categorize"
                      checked={autoCategorize}
                      onCheckedChange={(v) => setAutoCategorize(v === true)}
                      className="mt-0.5"
                    />
                    <div className="flex flex-col gap-1">
                      <Label htmlFor="auto-categorize">
                        Auto-categorize after sync
                      </Label>
                      <p className="text-xs text-muted-foreground">
                        On by default. After a successful sync and README fetch,
                        assign only new uncategorized repos with your committed
                        taxonomy. Manual overrides are never changed. Silent
                        no-op if Ollama is offline or no taxonomy is committed.
                      </p>
                    </div>
                  </div>
                  {needRebuild ? (
                    <div className="rounded-md border border-border bg-muted/40 p-3 text-sm">
                      <p className="mb-2">
                        Embed dimension changed. Rebuild the embeddings table
                        (this clears existing vectors), then run Build
                        embeddings from the library.
                      </p>
                      <Button
                        type="button"
                        variant="secondary"
                        disabled={rebuildMutation.isPending}
                        onClick={() => rebuildMutation.mutate()}
                      >
                        {rebuildMutation.isPending
                          ? "Rebuilding…"
                          : "Rebuild embeddings table"}
                      </Button>
                      {rebuildMutation.isError ? (
                        <p className="mt-2 text-sm text-destructive">
                          {errorMessage(rebuildMutation.error)}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="flex items-center gap-3">
                    <Button
                      type="submit"
                      disabled={saveSettingsMutation.isPending}
                    >
                      {saveSettingsMutation.isPending
                        ? "Saving…"
                        : "Save settings"}
                    </Button>
                    {settingsSaved ? (
                      <span className="text-sm text-muted-foreground">
                        Saved
                      </span>
                    ) : null}
                    {saveSettingsMutation.isError ? (
                      <span className="text-sm text-destructive">
                        {errorMessage(saveSettingsMutation.error)}
                      </span>
                    ) : null}
                  </div>
                </form>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Setup guide</CardTitle>
              <CardDescription>
                Reopen the first-run checklist. Progress comes from your current
                GitHub, sync, category, and embedding status.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Button
                type="button"
                variant="outline"
                disabled={reopenGuideMutation.isPending}
                onClick={() => reopenGuideMutation.mutate()}
              >
                Run setup guide again
              </Button>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
