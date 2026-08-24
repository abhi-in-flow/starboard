import { describe, expect, test } from "bun:test";
import type { CategoryNode } from "../types";
import { resolveCategoryId } from "./categories";
import { deriveHealthItems, healthTriggerLabel } from "./health";
import { nextEscapeAction } from "./keyboard";

describe("deriveHealthItems", () => {
  test("routes GitHub/Ollama to Settings General and coverage to Status", () => {
    const items = deriveHealthItems({
      githubConnected: true,
      githubUsername: "octocat",
      lastSyncedAt: "2026-01-01T00:00:00Z",
      pendingReadmes: 4,
      ollamaAvailable: false,
      ollamaConfigured: true,
      embeddingCoverage: 0.4,
      assignmentCoverage: 0.5,
      categoryCount: 8,
      repoCount: 20,
    });
    const byId = Object.fromEntries(items.map((i) => [i.id, i]));
    expect(byId.github.target).toEqual({
      kind: "settings",
      tab: "general",
      section: "github",
    });
    expect(byId.ollama.target).toEqual({
      kind: "settings",
      tab: "general",
      section: "ollama",
    });
    expect(byId.embeddings.target).toEqual({
      kind: "settings",
      tab: "status",
    });
    expect(byId.readme.value).toContain("4 pending");
    expect(byId.readme.target).toEqual({ kind: "library" });
    expect(byId.sync.target).toEqual({ kind: "library" });
  });

  test("missing taxonomy routes to Categories, not a new Admin destination", () => {
    const items = deriveHealthItems({ categoryCount: 0, repoCount: 0 });
    expect(items.find((i) => i.id === "categories")?.target).toEqual({
      kind: "categories",
      section: "taxonomy",
    });
    expect(items).toHaveLength(6);
  });

  test("health trigger announces aggregate warning and error state", () => {
    const troubled = deriveHealthItems({
      githubConnected: false,
      pendingReadmes: 3,
      ollamaAvailable: false,
      ollamaConfigured: true,
    });
    expect(healthTriggerLabel(troubled)).toMatch(/error/i);
    expect(healthTriggerLabel(troubled)).toContain("GitHub");
    const clear = deriveHealthItems({
      githubConnected: true,
      lastSyncedAt: "2026-01-01T00:00:00Z",
      ollamaAvailable: true,
      embeddingCoverage: 1,
      assignmentCoverage: 1,
      categoryCount: 2,
    });
    expect(healthTriggerLabel(clear)).toBe("Library health: all clear");
  });
});

describe("navigation helpers", () => {
  test("Escape order is detail → query → filters/review", () => {
    expect(
      nextEscapeAction({
        detailOpen: true,
        hasQuery: true,
        hasFiltersOrReview: true,
      }),
    ).toBe("close-detail");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: true,
        hasFiltersOrReview: true,
      }),
    ).toBe("clear-query");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: false,
        hasFiltersOrReview: true,
      }),
    ).toBe("clear-filters");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: false,
        hasFiltersOrReview: false,
      }),
    ).toBe("none");
  });

  test("category deep-link resolves names case-insensitively", () => {
    const nodes: CategoryNode[] = [
      {
        id: 1,
        name: "Developer Tools",
        parentId: null,
        count: 3,
        children: [
          { id: 2, name: "CLIs", parentId: 1, count: 1, children: [] },
        ],
      },
    ];
    expect(resolveCategoryId(nodes, "clis")).toBe(2);
    expect(resolveCategoryId(nodes, "Developer Tools / CLIs")).toBe(2);
    expect(resolveCategoryId(nodes, "missing")).toBeNull();
  });
});
