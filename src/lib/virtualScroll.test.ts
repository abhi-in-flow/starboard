import { describe, expect, test } from "bun:test";
import { shouldFetchNextPage, shouldScrollSelectedRow } from "./virtualScroll";

describe("shouldScrollSelectedRow", () => {
  test("does not scroll when only items identity would have changed", () => {
    const key = { selectedId: 9, columns: 1 };
    expect(shouldScrollSelectedRow(key, key)).toBe(false);
    expect(
      shouldScrollSelectedRow(
        { selectedId: 9, columns: 1 },
        { selectedId: 9, columns: 1 },
      ),
    ).toBe(false);
  });

  test("scrolls when selected id or column count changes", () => {
    expect(
      shouldScrollSelectedRow(
        { selectedId: 1, columns: 1 },
        { selectedId: 2, columns: 1 },
      ),
    ).toBe(true);
    expect(
      shouldScrollSelectedRow(
        { selectedId: 1, columns: 1 },
        { selectedId: 1, columns: 3 },
      ),
    ).toBe(true);
  });

  test("does not scroll when selection is cleared", () => {
    expect(
      shouldScrollSelectedRow(
        { selectedId: 4, columns: 2 },
        { selectedId: null, columns: 2 },
      ),
    ).toBe(false);
  });
});

describe("shouldFetchNextPage", () => {
  const nearEnd = {
    lastIndex: 8,
    rowCount: 10,
    lastTriggered: null,
    hasNextPage: true,
    isFetching: false,
    fetchFailed: false,
  };

  test("fetches near the end once per index/rowCount pair", () => {
    expect(shouldFetchNextPage(nearEnd)).toBe(true);
    expect(
      shouldFetchNextPage({
        ...nearEnd,
        lastTriggered: { index: 8, rowCount: 10 },
      }),
    ).toBe(false);
    expect(
      shouldFetchNextPage({
        ...nearEnd,
        lastIndex: 11,
        rowCount: 12,
        lastTriggered: { index: 8, rowCount: 10 },
      }),
    ).toBe(true);
  });

  test("does not retry a failed page or refetch while in flight", () => {
    expect(shouldFetchNextPage({ ...nearEnd, fetchFailed: true })).toBe(false);
    expect(shouldFetchNextPage({ ...nearEnd, isFetching: true })).toBe(false);
    expect(shouldFetchNextPage({ ...nearEnd, hasNextPage: false })).toBe(false);
    expect(shouldFetchNextPage({ ...nearEnd, lastIndex: 3 })).toBe(false);
  });
});
