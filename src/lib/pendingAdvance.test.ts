import { describe, expect, test } from "bun:test";
import {
  createPendingAdvance,
  resolvePendingAdvance,
  serializeListGeneration,
} from "./pendingAdvance";

const genA = serializeListGeneration({
  query: "",
  language: null,
  topic: null,
  categoryId: null,
  reviewPreset: null,
  hideUnstarred: true,
  hideArchived: true,
  sort: "starredAt",
  sortDesc: true,
  searchMode: "keyword",
});

const genB = serializeListGeneration({
  query: "rust",
  language: null,
  topic: null,
  categoryId: null,
  reviewPreset: null,
  hideUnstarred: true,
  hideArchived: true,
  sort: "starredAt",
  sortDesc: true,
  searchMode: "keyword",
});

describe("serializeListGeneration", () => {
  test("changes when query, filters, sort, review, or mode change", () => {
    expect(genA).not.toBe(genB);
    expect(
      serializeListGeneration({
        query: "",
        language: "Rust",
        topic: null,
        categoryId: null,
        reviewPreset: null,
        hideUnstarred: true,
        hideArchived: true,
        sort: "starredAt",
        sortDesc: true,
        searchMode: "keyword",
      }),
    ).not.toBe(genA);
    expect(
      serializeListGeneration({
        query: "",
        language: null,
        topic: null,
        categoryId: null,
        reviewPreset: "inactive",
        hideUnstarred: true,
        hideArchived: true,
        sort: "stale",
        sortDesc: false,
        searchMode: "hybrid",
      }),
    ).not.toBe(genA);
  });
});

describe("createPendingAdvance", () => {
  test("binds to generation and source id only when paging is possible", () => {
    expect(
      createPendingAdvance({
        generation: genA,
        sourceId: 12,
        hasNextPage: true,
        isFetchingNextPage: false,
        fetchFailed: false,
      }),
    ).toEqual({ generation: genA, sourceId: 12 });
    expect(
      createPendingAdvance({
        generation: genA,
        sourceId: 12,
        hasNextPage: false,
        isFetchingNextPage: false,
        fetchFailed: false,
      }),
    ).toBeNull();
    expect(
      createPendingAdvance({
        generation: genA,
        sourceId: 12,
        hasNextPage: true,
        isFetchingNextPage: true,
        fetchFailed: false,
      }),
    ).toBeNull();
  });
});

describe("resolvePendingAdvance", () => {
  const pending = { generation: genA, sourceId: 2 };

  test("advances only when the same generation returns and source id still matches", () => {
    expect(
      resolvePendingAdvance({
        pending,
        generation: genA,
        selectedId: 2,
        itemIds: [1, 2, 3],
        fetchFailed: false,
        fetchSettled: true,
      }),
    ).toEqual({ kind: "select", id: 3 });
  });

  test("clears on generation change, selection change, failed or empty page", () => {
    expect(
      resolvePendingAdvance({
        pending,
        generation: genB,
        selectedId: 2,
        itemIds: [1, 2, 3],
        fetchFailed: false,
        fetchSettled: true,
      }),
    ).toEqual({ kind: "clear" });
    expect(
      resolvePendingAdvance({
        pending,
        generation: genA,
        selectedId: 9,
        itemIds: [1, 2, 3],
        fetchFailed: false,
        fetchSettled: true,
      }),
    ).toEqual({ kind: "clear" });
    expect(
      resolvePendingAdvance({
        pending,
        generation: genA,
        selectedId: 2,
        itemIds: [1, 2],
        fetchFailed: true,
        fetchSettled: true,
      }),
    ).toEqual({ kind: "clear" });
    expect(
      resolvePendingAdvance({
        pending,
        generation: genA,
        selectedId: 2,
        itemIds: [1, 2],
        fetchFailed: false,
        fetchSettled: true,
      }),
    ).toEqual({ kind: "clear" });
  });

  test("waits while the same request is still in flight", () => {
    expect(
      resolvePendingAdvance({
        pending,
        generation: genA,
        selectedId: 2,
        itemIds: [1, 2],
        fetchFailed: false,
        fetchSettled: false,
      }),
    ).toEqual({ kind: "keep" });
  });
});
