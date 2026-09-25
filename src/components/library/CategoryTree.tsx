import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { listCategories, setRepoCategory } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type { CategoryNode } from "@/types";

const REPO_DND_TYPE = "application/x-starboard-repo-id";

function CategoryItem({ node, depth }: { node: CategoryNode; depth: number }) {
  const queryClient = useQueryClient();
  const categoryId = useUiStore((s) => s.categoryId);
  const setCategoryId = useUiStore((s) => s.setCategoryId);
  const active = categoryId === node.id;
  const [dragOver, setDragOver] = useState(false);

  const assign = useMutation({
    mutationFn: ({ repoId, catId }: { repoId: number; catId: number }) =>
      setRepoCategory(repoId, catId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["categories"] });
      void queryClient.invalidateQueries({ queryKey: ["repos"] });
      void queryClient.invalidateQueries({ queryKey: ["repo"] });
    },
  });

  return (
    <div>
      <button
        type="button"
        onClick={() => setCategoryId(active ? null : node.id)}
        onDragOver={(e) => {
          if (![...e.dataTransfer.types].includes(REPO_DND_TYPE)) {
            return;
          }
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          const raw = e.dataTransfer.getData(REPO_DND_TYPE);
          const repoId = Number(raw);
          if (!Number.isFinite(repoId) || repoId <= 0) {
            return;
          }
          assign.mutate({ repoId, catId: node.id });
        }}
        aria-pressed={active}
        aria-current={active ? "true" : undefined}
        className={cn(
          "flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-sm transition-colors",
          active
            ? "bg-sidebar-accent text-sidebar-accent-foreground"
            : "hover:bg-sidebar-accent/60",
          dragOver && "ring-2 ring-primary ring-offset-1",
        )}
        style={{ paddingLeft: 8 + depth * 12 }}
      >
        <span className="truncate">{node.name}</span>
        <span className="ml-2 shrink-0 text-xs text-muted-foreground">
          {node.count}
        </span>
      </button>
      {node.children.map((child) => (
        <CategoryItem key={child.id} node={child} depth={depth + 1} />
      ))}
    </div>
  );
}

export function CategoryTree() {
  const query = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const nodes = query.data ?? [];

  return (
    <div className="flex h-full flex-col border-r border-sidebar-border bg-sidebar/40">
      <div className="border-b border-sidebar-border px-3 py-3">
        <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Categories
        </p>
        <p className="mt-1 text-[11px] text-foreground/70">
          Drop a repo here to set a manual category. On smaller screens, assign
          from the detail picker.
        </p>
      </div>
      <div className="flex-1 overflow-auto p-2">
        {query.isLoading ? (
          <p className="px-2 text-xs text-muted-foreground">Loading…</p>
        ) : nodes.length === 0 ? (
          <p className="px-2 text-xs leading-relaxed text-muted-foreground">
            No categories yet — open Categories to generate a taxonomy.
          </p>
        ) : (
          nodes.map((node) => (
            <CategoryItem key={node.id} node={node} depth={0} />
          ))
        )}
      </div>
    </div>
  );
}

export { REPO_DND_TYPE };
