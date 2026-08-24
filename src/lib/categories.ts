import type { CategoryNode } from "@/types";

export type FlatCategory = {
  id: number;
  name: string;
  label: string;
  parentId: number | null;
};

export function flattenCategories(
  nodes: CategoryNode[],
  prefix = "",
): FlatCategory[] {
  const out: FlatCategory[] = [];
  for (const node of nodes) {
    const label = prefix ? `${prefix} / ${node.name}` : node.name;
    out.push({
      id: node.id,
      name: node.name,
      label,
      parentId: node.parentId,
    });
    out.push(...flattenCategories(node.children, label));
  }
  return out;
}

export function findCategoryName(
  nodes: CategoryNode[],
  id: number | null,
): string | null {
  if (id == null) {
    return null;
  }
  for (const node of flattenCategories(nodes)) {
    if (node.id === id) {
      return node.label;
    }
  }
  return null;
}

export function resolveCategoryId(
  nodes: CategoryNode[],
  name: string,
): number | null {
  const needle = name.trim().toLowerCase();
  if (!needle) {
    return null;
  }
  const flat = flattenCategories(nodes);
  const exact = flat.find((n) => n.name.toLowerCase() === needle);
  if (exact) {
    return exact.id;
  }
  const labeled = flat.find((n) => n.label.toLowerCase() === needle);
  return labeled?.id ?? null;
}
