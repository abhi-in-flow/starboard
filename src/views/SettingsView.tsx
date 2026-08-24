import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
  updateSettings,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
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

  const { embeddings: emb, categorization: cat } = data;
  const coveragePct = Math.round(emb.coverage * 100);

  return (
    <div className="flex flex-col gap-6">
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
            To (re)build vectors, use Build embeddings from the Library toolbar
            when Ollama is online.
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
  const [tab, setTab] = useState<"general" | "status">("general");
  const [pat, setPat] = useState("");
  const [replacingToken, setReplacingToken] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [chatModel, setChatModel] = useState("");
  const [embedModel, setEmbedModel] = useState("");
  const [embedDimension, setEmbedDimension] = useState("768");
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
      });
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      setEmbedDimension(String(settings.embedDimension));
      setSettingsSaved(true);
      window.setTimeout(() => setSettingsSaved(false), 2000);
      void queryClient.invalidateQueries({ queryKey: ["embedStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
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
    },
  });

  const auth = authQuery.data;
  const showPatForm = !auth?.connected || replacingToken;
  const needRebuild = settingsQuery.data?.embeddingsNeedRebuild === true;

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-6 p-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Connect GitHub and configure the local Ollama endpoint.
        </p>
      </div>

      <div className="flex w-fit items-center rounded-lg border border-border p-0.5">
        <Button
          type="button"
          size="sm"
          variant={tab === "general" ? "secondary" : "ghost"}
          className={cn("h-8 px-3 text-xs")}
          onClick={() => setTab("general")}
        >
          General
        </Button>
        <Button
          type="button"
          size="sm"
          variant={tab === "status" ? "secondary" : "ghost"}
          className={cn("h-8 px-3 text-xs")}
          onClick={() => {
            setTab("status");
            void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
          }}
        >
          Status
        </Button>
      </div>

      {tab === "status" ? (
        <StatusTab />
      ) : (
        <>
          <Card>
            <CardHeader>
              <CardTitle>GitHub</CardTitle>
              <CardDescription>
                Personal access token is stored in the OS credential store and
                never written to the database.
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

          <Card>
            <CardHeader>
              <CardTitle>Ollama</CardTitle>
              <CardDescription>
                Base URL may point at a machine on your LAN. Do not hardcode
                localhost in application code.
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
                    />
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
                      rebuilding the vector table.
                    </p>
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
        </>
      )}
    </div>
  );
}
