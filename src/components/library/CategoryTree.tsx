import { useQuery } from "@tanstack/react-query";
import { listCategories } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type { CategoryNode } from "@/types";

function CategoryItem({ node, depth }: { node: CategoryNode; depth: number }) {
  const categoryId = useUiStore((s) => s.categoryId);
  const setCategoryId = useUiStore((s) => s.setCategoryId);
  const active = categoryId === node.id;

  return (
    <div>
      <button
        type="button"
        onClick={() => setCategoryId(active ? null : node.id)}
        className={cn(
          "flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-sm transition-colors",
          active
            ? "bg-sidebar-accent text-sidebar-accent-foreground"
            : "hover:bg-sidebar-accent/60",
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
      </div>
      <div className="flex-1 overflow-auto p-2">
        {query.isLoading ? (
          <p className="px-2 text-xs text-muted-foreground">Loading…</p>
        ) : nodes.length === 0 ? (
          <p className="px-2 text-xs leading-relaxed text-muted-foreground">
            No categories yet — run categorization in Phase 3.
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
