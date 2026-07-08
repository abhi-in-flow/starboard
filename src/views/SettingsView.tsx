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
import {
  connectGithub,
  disconnectGithub,
  getAuthStatus,
  getSettings,
  updateSettings,
} from "@/lib/tauri";
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

export function SettingsView() {
  const queryClient = useQueryClient();
  const [pat, setPat] = useState("");
  const [replacingToken, setReplacingToken] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [chatModel, setChatModel] = useState("");
  const [embedModel, setEmbedModel] = useState("");
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
    mutationFn: () =>
      updateSettings({
        ollamaBaseUrl: baseUrl,
        ollamaChatModel: chatModel,
        ollamaEmbedModel: embedModel,
      }),
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      setSettingsSaved(true);
      window.setTimeout(() => setSettingsSaved(false), 2000);
    },
  });

  const auth = authQuery.data;
  const showPatForm = !auth?.connected || replacingToken;

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-6 p-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Connect GitHub and configure the local Ollama endpoint.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>GitHub</CardTitle>
          <CardDescription>
            Personal access token is stored in the OS credential store and never
            written to the database.
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
              <div className="flex items-center gap-3">
                <Button type="submit" disabled={saveSettingsMutation.isPending}>
                  {saveSettingsMutation.isPending ? "Saving…" : "Save settings"}
                </Button>
                {settingsSaved ? (
                  <span className="text-sm text-muted-foreground">Saved</span>
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
    </div>
  );
}
