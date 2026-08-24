import { useMutation, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { formatRelative } from "@/lib/format";
import {
  checkDbIntegrity,
  createBackup,
  restoreBackup,
  validateBackup,
} from "@/lib/tauri";
import type {
  AppError,
  BackupResult,
  BackupValidation,
  DataPanel,
  IntegrityCheckResult,
  RestoreResult,
} from "@/types";

function backupFilters() {
  return [
    {
      name: "Starboard backup",
      extensions: ["db", "sqlite", "sqlite3", "starboard-backup"],
    },
  ];
}

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as AppError).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "Something went wrong";
}

function defaultBackupName() {
  return `starboard-${new Date().toISOString().slice(0, 10)}.db`;
}

type Props = {
  data: DataPanel;
};

export function DataBackupCard({ data }: Props) {
  const queryClient = useQueryClient();
  const [backupResult, setBackupResult] = useState<BackupResult | null>(null);
  const [validation, setValidation] = useState<BackupValidation | null>(null);
  const [integrity, setIntegrity] = useState<IntegrityCheckResult | null>(null);
  const [restorePreview, setRestorePreview] = useState<string | null>(null);
  const [restoreResult, setRestoreResult] = useState<RestoreResult | null>(
    null,
  );
  const [actionError, setActionError] = useState<string | null>(null);

  const backupMutation = useMutation({
    mutationFn: createBackup,
    onSuccess: (result) => {
      setBackupResult(result);
      setActionError(null);
      void queryClient.invalidateQueries({ queryKey: ["systemStatus"] });
    },
    onError: (err) => {
      setActionError(errorMessage(err));
    },
  });

  const validateMutation = useMutation({
    mutationFn: validateBackup,
    onSuccess: (result) => {
      setValidation(result);
      setActionError(null);
    },
    onError: (err) => {
      setValidation(null);
      setActionError(errorMessage(err));
    },
  });

  const integrityMutation = useMutation({
    mutationFn: checkDbIntegrity,
    onSuccess: (result) => {
      setIntegrity(result);
      setActionError(null);
    },
    onError: (err) => {
      setActionError(errorMessage(err));
    },
  });

  const restoreMutation = useMutation({
    mutationFn: restoreBackup,
    onSuccess: async (result) => {
      setRestoreResult(result);
      setRestorePreview(null);
      setActionError(null);
      await queryClient.resetQueries();
    },
    onError: (err) => {
      setActionError(errorMessage(err));
    },
  });

  async function handleCreateBackup() {
    setActionError(null);
    const path = await save({
      title: "Save Starboard backup",
      defaultPath: defaultBackupName(),
      filters: backupFilters(),
    });
    if (!path) return;
    backupMutation.mutate(path);
  }

  async function handleValidate() {
    setActionError(null);
    const path = await open({
      title: "Validate Starboard backup",
      multiple: false,
      directory: false,
      filters: backupFilters(),
    });
    if (!path || Array.isArray(path)) return;
    validateMutation.mutate(path);
  }

  async function handleChooseRestore() {
    setActionError(null);
    setRestoreResult(null);
    const path = await open({
      title: "Restore Starboard backup",
      multiple: false,
      directory: false,
      filters: backupFilters(),
    });
    if (!path || Array.isArray(path)) return;
    setRestorePreview(path);
  }

  const lastBackupLabel = data.lastBackupAt
    ? `${formatRelative(data.lastBackupAt)} (${data.lastBackupAt})`
    : "Never";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Data</CardTitle>
        <CardDescription>
          Consistent SQLite snapshots of this library. The GitHub token stays in
          the OS keyring and is never exported.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 text-sm">
        <div className="grid grid-cols-2 gap-x-4 gap-y-2">
          <span className="text-muted-foreground">Database</span>
          <span className="break-all font-medium">{data.dbPath}</span>
          <span className="text-muted-foreground">Last backup</span>
          <span className="font-medium">{lastBackupLabel}</span>
          {data.lastBackupPath ? (
            <>
              <span className="text-muted-foreground">Last backup file</span>
              <span className="break-all font-medium">
                {data.lastBackupPath}
              </span>
            </>
          ) : null}
          <span className="text-muted-foreground">Last backup result</span>
          <span className="font-medium">
            {data.lastBackupOk == null
              ? "—"
              : data.lastBackupOk
                ? "Succeeded"
                : "Failed"}
          </span>
          <span className="text-muted-foreground">Last restore</span>
          <span className="font-medium">
            {data.lastRestoreAt
              ? `${formatRelative(data.lastRestoreAt)} (${data.lastRestoreAt})`
              : "Never"}
          </span>
        </div>

        <p className="text-xs text-muted-foreground">
          Restore is blocked while sync, README fetch, embedding, or
          categorization is running. A safety copy of the current database is
          written first; if restore fails the original library is put back. No
          app restart is required.
        </p>

        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            disabled={backupMutation.isPending}
            onClick={() => void handleCreateBackup()}
          >
            {backupMutation.isPending ? "Creating backup…" : "Create backup"}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={validateMutation.isPending}
            onClick={() => void handleValidate()}
          >
            {validateMutation.isPending ? "Validating…" : "Validate backup"}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={integrityMutation.isPending}
            onClick={() => integrityMutation.mutate()}
          >
            {integrityMutation.isPending ? "Checking…" : "Check integrity"}
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={restoreMutation.isPending}
            onClick={() => void handleChooseRestore()}
          >
            Restore backup
          </Button>
        </div>

        {backupResult ? (
          <p className="text-sm">
            Backup saved ({backupResult.repoCount} repos, schema{" "}
            {backupResult.schemaVersion}).
          </p>
        ) : null}
        {validation ? <p className="text-sm">{validation.message}</p> : null}
        {integrity ? (
          <p className={integrity.ok ? "text-sm" : "text-sm text-destructive"}>
            Integrity: {integrity.message}
          </p>
        ) : null}
        {restoreResult ? (
          <p className="text-sm">
            Restored {restoreResult.repoCount} repos
            {restoreResult.migrated ? " (older backup was migrated)" : ""}.
            Safety copy: {restoreResult.preRestoreBackupPath}. GitHub token was
            not changed.
          </p>
        ) : null}

        {restorePreview ? (
          <div className="flex flex-col gap-3 rounded-md border border-destructive/40 bg-muted/40 p-3">
            <p>
              Replace the current library with{" "}
              <span className="break-all font-medium">{restorePreview}</span>?
              This overwrites local repos, categories, and settings. A safety
              copy is saved first. Your GitHub token is not part of the backup
              and will not change.
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="destructive"
                disabled={restoreMutation.isPending}
                onClick={() => restoreMutation.mutate(restorePreview)}
              >
                {restoreMutation.isPending ? "Restoring…" : "Replace library"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                disabled={restoreMutation.isPending}
                onClick={() => setRestorePreview(null)}
              >
                Cancel
              </Button>
            </div>
          </div>
        ) : null}

        {actionError ? (
          <p className="text-sm text-destructive" role="alert">
            {actionError}
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}
