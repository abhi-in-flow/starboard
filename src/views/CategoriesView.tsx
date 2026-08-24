import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { Loader2, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
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
import { isCancelledMessage } from "@/lib/jobStatus";
import {
  cancelAssignment,
  commitTaxonomy,
  generateTaxonomy,
  getCategorizeStatus,
  getOllamaStatus,
  getTaxonomyEdit,
  listCategories,
  startAssignment,
  updateTaxonomy,
} from "@/lib/tauri";
import { useUiStore } from "@/store/ui";
import type {
  AppError,
  CategorizeProgress,
  TaxonomyDraft,
  TaxonomyEdit,
  TaxonomyNodeEdit,
} from "@/types";

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as AppError).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  return "Something went wrong";
}

type DraftSub = { key: string; id: number | null; name: string };
type DraftCat = {
  key: string;
  id: number | null;
  name: string;
  subcategories: DraftSub[];
};

type EditorMode = "create" | "edit";

let draftKeySeq = 0;
function nextKey(prefix: string): string {
  draftKeySeq += 1;
  return `${prefix}-${draftKeySeq}`;
}

function emptyDraft(): DraftCat[] {
  return [
    {
      key: nextKey("cat"),
      id: null,
      name: "",
      subcategories: [{ key: nextKey("sub"), id: null, name: "" }],
    },
  ];
}

function fromTaxonomyDraft(draft: TaxonomyDraft): DraftCat[] {
  return draft.categories.map((c) => ({
    key: nextKey("cat"),
    id: null,
    name: c.name,
    subcategories:
      c.subcategories.length > 0
        ? c.subcategories.map((s) => ({
            key: nextKey("sub"),
            id: null,
            name: s,
          }))
        : [{ key: nextKey("sub"), id: null, name: "" }],
  }));
}

function fromTaxonomyEdit(edit: TaxonomyEdit): DraftCat[] {
  return edit.categories.map((c) => ({
    key: nextKey("cat"),
    id: c.id,
    name: c.name,
    subcategories:
      c.subcategories.length > 0
        ? c.subcategories.map((s) => ({
            key: nextKey("sub"),
            id: s.id,
            name: s.name,
          }))
        : [{ key: nextKey("sub"), id: null, name: "" }],
  }));
}

function toTaxonomyDraft(cats: DraftCat[]): TaxonomyDraft {
  return {
    categories: cats
      .map((c) => ({
        name: c.name.trim(),
        subcategories: c.subcategories
          .map((s) => s.name.trim())
          .filter((s) => s.length > 0),
      }))
      .filter((c) => c.name.length > 0),
  };
}

function toTaxonomyEdit(cats: DraftCat[]): TaxonomyEdit {
  return {
    categories: cats
      .map(
        (c): TaxonomyNodeEdit => ({
          id: c.id,
          name: c.name.trim(),
          subcategories: c.subcategories
            .map((s) => ({
              id: s.id,
              name: s.name.trim(),
              subcategories: [],
            }))
            .filter((s) => s.name.length > 0),
        }),
      )
      .filter((c) => c.name.length > 0),
  };
}

export function CategoriesView() {
  const queryClient = useQueryClient();
  const categoriesSection = useUiStore((s) => s.categoriesSection);
  const setCategoriesSection = useUiStore((s) => s.setCategoriesSection);
  const [draft, setDraft] = useState<DraftCat[] | null>(null);
  const [editorMode, setEditorMode] = useState<EditorMode>("create");
  const [progress, setProgress] = useState<CategorizeProgress | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);

  const ollama = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
    refetchInterval: 15_000,
  });

  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const categorizeStatus = useQuery({
    queryKey: ["categorizeStatus"],
    queryFn: getCategorizeStatus,
    refetchInterval: (q) => (q.state.data?.running ? 1000 : 5000),
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<CategorizeProgress>("categorize://progress", (event) => {
      setProgress(event.payload);
      void queryClient.invalidateQueries({ queryKey: ["categorizeStatus"] });
      if (event.payload.kind === "done" || event.payload.kind === "error") {
        void queryClient.invalidateQueries({ queryKey: ["categories"] });
        void queryClient.invalidateQueries({ queryKey: ["repos"] });
        void queryClient.invalidateQueries({ queryKey: ["repo"] });
        void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [queryClient]);

  useEffect(() => {
    if (!categoriesSection) {
      return;
    }
    const id =
      categoriesSection === "taxonomy"
        ? "categories-taxonomy"
        : "categories-assign";
    document
      .getElementById(id)
      ?.scrollIntoView({ behavior: "smooth", block: "start" });
    setCategoriesSection(null);
  }, [categoriesSection, setCategoriesSection]);

  const generateMutation = useMutation({
    mutationFn: generateTaxonomy,
    onSuccess: (result) => {
      setActionError(null);
      setEditorMode("create");
      setDraft(fromTaxonomyDraft(result));
    },
    onError: (err) => setActionError(errorMessage(err)),
  });

  const editMutation = useMutation({
    mutationFn: getTaxonomyEdit,
    onSuccess: (result) => {
      setActionError(null);
      setEditorMode("edit");
      setDraft(fromTaxonomyEdit(result));
    },
    onError: (err) => setActionError(errorMessage(err)),
  });

  const commitMutation = useMutation({
    mutationFn: async ({
      next,
      force,
    }: {
      next: TaxonomyDraft;
      force: boolean;
    }) => commitTaxonomy(next, force),
    onSuccess: () => {
      setActionError(null);
      setDraft(null);
      setEditorMode("create");
      void queryClient.invalidateQueries({ queryKey: ["categories"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => setActionError(errorMessage(err)),
  });

  const saveEditMutation = useMutation({
    mutationFn: (edit: TaxonomyEdit) => updateTaxonomy(edit),
    onSuccess: () => {
      setActionError(null);
      setDraft(null);
      setEditorMode("create");
      void queryClient.invalidateQueries({ queryKey: ["categories"] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
      void queryClient.invalidateQueries({ queryKey: ["repo"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => setActionError(errorMessage(err)),
  });

  const assignMutation = useMutation({
    mutationFn: startAssignment,
    onSuccess: () => {
      setActionError(null);
      setActionNotice(null);
      setProgress({
        kind: "starting",
        current: 0,
        total: 0,
        message: "Starting assignment…",
      });
      void queryClient.invalidateQueries({ queryKey: ["categorizeStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["setupStatus"] });
    },
    onError: (err) => {
      const message = errorMessage(err);
      if (isCancelledMessage(message)) {
        setActionNotice(message);
        setActionError(null);
      } else {
        setActionError(message);
      }
    },
  });

  const cancelAssignMutation = useMutation({
    mutationFn: cancelAssignment,
    onSuccess: () => {
      setActionNotice("Cancelled");
      setActionError(null);
      void queryClient.invalidateQueries({ queryKey: ["categorizeStatus"] });
    },
    onError: (err) => setActionError(errorMessage(err)),
  });

  const offline = ollama.data != null && !ollama.data.available;
  const offlineHint =
    ollama.data?.message ||
    "Ollama offline — set base URL + chat model in Settings.";
  const hasCommitted = (categories.data?.length ?? 0) > 0;
  const running = categorizeStatus.data?.running === true;
  const saving = commitMutation.isPending || saveEditMutation.isPending;

  function updateCategoryName(key: string, name: string) {
    if (!draft) return;
    setDraft(draft.map((c) => (c.key === key ? { ...c, name } : c)));
  }

  function updateSubcategory(catKey: string, subKey: string, name: string) {
    if (!draft) return;
    setDraft(
      draft.map((c) => {
        if (c.key !== catKey) return c;
        return {
          ...c,
          subcategories: c.subcategories.map((s) =>
            s.key === subKey ? { ...s, name } : s,
          ),
        };
      }),
    );
  }

  function addCategory() {
    if (!draft) return;
    setDraft([
      ...draft,
      {
        key: nextKey("cat"),
        id: null,
        name: "",
        subcategories: [{ key: nextKey("sub"), id: null, name: "" }],
      },
    ]);
  }

  function removeCategory(key: string) {
    if (!draft) return;
    setDraft(draft.filter((c) => c.key !== key));
  }

  function addSubcategory(catKey: string) {
    if (!draft) return;
    setDraft(
      draft.map((c) =>
        c.key === catKey
          ? {
              ...c,
              subcategories: [
                ...c.subcategories,
                { key: nextKey("sub"), id: null, name: "" },
              ],
            }
          : c,
      ),
    );
  }

  function removeSubcategory(catKey: string, subKey: string) {
    if (!draft) return;
    setDraft(
      draft.map((c) => {
        if (c.key !== catKey) return c;
        return {
          ...c,
          subcategories: c.subcategories.filter((s) => s.key !== subKey),
        };
      }),
    );
  }

  function onSave() {
    if (!draft) return;
    if (editorMode === "edit") {
      const next = toTaxonomyEdit(draft);
      if (next.categories.length === 0) {
        setActionError("Add at least one category before saving.");
        return;
      }
      saveEditMutation.mutate(next);
      return;
    }

    const next = toTaxonomyDraft(draft);
    if (next.categories.length === 0) {
      setActionError("Add at least one category before committing.");
      return;
    }
    commitMutation.mutate({ next, force: false });
  }

  function onForceReplace() {
    if (!draft) return;
    const next = toTaxonomyDraft(draft);
    if (next.categories.length === 0) {
      setActionError("Add at least one category before committing.");
      return;
    }
    const ok = window.confirm(
      "Force replace wipes the taxonomy and clears all repo category assignments (including manual). Prefer Edit taxonomy to rename without losing assignments. Continue?",
    );
    if (!ok) return;
    commitMutation.mutate({ next, force: true });
  }

  function cancelEditor() {
    setDraft(null);
    setEditorMode("create");
    setActionError(null);
  }

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Categories</h1>
          <p className="text-sm text-muted-foreground">
            Generate a taxonomy with Ollama, review it, then assign repos.
          </p>
        </div>
        {ollama.data ? (
          <Badge variant={ollama.data.available ? "default" : "secondary"}>
            {ollama.data.available ? "Ollama online" : "Ollama offline"}
          </Badge>
        ) : null}
      </div>

      {offline ? (
        <p className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
          {offlineHint}
        </p>
      ) : null}

      {actionNotice ? (
        <p className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
          {actionNotice}
        </p>
      ) : null}
      {actionError ? (
        <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {actionError}
        </p>
      ) : null}

      <Card id="categories-taxonomy">
        <CardHeader>
          <CardTitle className="text-base">1. Taxonomy</CardTitle>
          <CardDescription>
            Propose categories from your library, edit the draft, then commit.
            Use Edit taxonomy to rename/add/remove without wiping assignments.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              disabled={offline || generateMutation.isPending || running}
              onClick={() => generateMutation.mutate()}
            >
              {generateMutation.isPending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : null}
              Generate with Ollama
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={!hasCommitted || editMutation.isPending || running}
              onClick={() => editMutation.mutate()}
            >
              {editMutation.isPending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : null}
              Edit taxonomy
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setEditorMode("create");
                setDraft(emptyDraft());
              }}
            >
              Start blank draft
            </Button>
            {draft ? (
              <>
                <Button type="button" disabled={saving} onClick={onSave}>
                  {saving ? <Loader2 className="size-4 animate-spin" /> : null}
                  {editorMode === "edit" ? "Save changes" : "Commit taxonomy"}
                </Button>
                {editorMode === "create" && hasCommitted ? (
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={saving}
                    onClick={onForceReplace}
                  >
                    Force replace
                  </Button>
                ) : null}
                <Button
                  type="button"
                  variant="ghost"
                  disabled={saving}
                  onClick={cancelEditor}
                >
                  Cancel
                </Button>
              </>
            ) : null}
          </div>

          {draft ? (
            <div className="flex flex-col gap-4 rounded-lg border border-border p-3">
              {editorMode === "edit" ? (
                <p className="text-xs text-muted-foreground">
                  Editing committed taxonomy — renames keep assignments; deleted
                  categories drop only their own assignments.
                </p>
              ) : null}
              {draft.map((cat) => (
                <div
                  key={cat.key}
                  className="flex flex-col gap-2 rounded-md bg-muted/30 p-3"
                >
                  <div className="flex items-center gap-2">
                    <Label className="w-24 shrink-0 text-xs">Category</Label>
                    <Input
                      value={cat.name}
                      onChange={(e) =>
                        updateCategoryName(cat.key, e.target.value)
                      }
                      placeholder="e.g. AI / LLM"
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-8 w-8 p-0"
                      onClick={() => removeCategory(cat.key)}
                      aria-label="Remove category"
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  </div>
                  {cat.subcategories.map((sub) => (
                    <div key={sub.key} className="flex items-center gap-2 pl-6">
                      <Label className="w-24 shrink-0 text-xs">
                        Subcategory
                      </Label>
                      <Input
                        value={sub.name}
                        onChange={(e) =>
                          updateSubcategory(cat.key, sub.key, e.target.value)
                        }
                        placeholder="e.g. Agent frameworks"
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0"
                        onClick={() => removeSubcategory(cat.key, sub.key)}
                        aria-label="Remove subcategory"
                      >
                        <Trash2 className="size-4" />
                      </Button>
                    </div>
                  ))}
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="w-fit gap-1"
                    onClick={() => addSubcategory(cat.key)}
                  >
                    <Plus className="size-3" />
                    Add subcategory
                  </Button>
                </div>
              ))}
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="w-fit gap-1"
                onClick={addCategory}
              >
                <Plus className="size-3" />
                Add category
              </Button>
            </div>
          ) : null}

          {hasCommitted && !draft ? (
            <div className="text-sm text-muted-foreground">
              Committed taxonomy ({categories.data?.length ?? 0} top-level). Use
              Edit taxonomy to change names, or Generate again for a new draft.
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card id="categories-assign">
        <CardHeader>
          <CardTitle className="text-base">2. Assign repos</CardTitle>
          <CardDescription>
            Batches uncategorized repos against the committed taxonomy. Manual
            overrides are never overwritten. When auto-categorize is on in
            Settings, new uncategorized repos are assigned after sync once
            READMEs finish.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <Button
            type="button"
            className="w-fit"
            disabled={
              offline || !hasCommitted || running || assignMutation.isPending
            }
            onClick={() => assignMutation.mutate()}
          >
            {running ? <Loader2 className="size-4 animate-spin" /> : null}
            {running ? "Assigning…" : "Start assignment"}
          </Button>
          {running ? (
            <Button
              type="button"
              variant="outline"
              className="w-fit"
              disabled={cancelAssignMutation.isPending}
              onClick={() => cancelAssignMutation.mutate()}
            >
              Cancel
            </Button>
          ) : null}
          {progress ? (
            <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
              <p>
                {progress.message}
                {progress.total > 0
                  ? ` (${progress.current}/${progress.total})`
                  : ""}
              </p>
              {progress.error ? (
                <p
                  className={
                    isCancelledMessage(progress.error)
                      ? "mt-1 text-muted-foreground"
                      : "mt-1 text-destructive"
                  }
                >
                  {progress.error}
                </p>
              ) : null}
            </div>
          ) : null}
          {actionNotice ? (
            <p className="text-sm text-muted-foreground">{actionNotice}</p>
          ) : null}
          {categorizeStatus.data?.lastError &&
          !isCancelledMessage(categorizeStatus.data.lastError) ? (
            <p className="text-sm text-destructive">
              Last error: {categorizeStatus.data.lastError}
            </p>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
