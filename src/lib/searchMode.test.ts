import { describe, expect, test } from "bun:test";
import { searchModeFallback, searchSortIsRelevance } from "./searchMode";

describe("searchModeFallback", () => {
  test("labels Hybrid → Keyword fallback", () => {
    const result = searchModeFallback("hybrid", "keyword");
    expect(result.fallback).toBe(true);
    expect(result.badge).toBe("Using Keyword");
    expect(result.title).toContain("Hybrid was requested");
    expect(result.usedLabel).toBe("Keyword");
  });

  test("no badge when the requested mode ran", () => {
    expect(searchModeFallback("hybrid", "hybrid").badge).toBeNull();
    expect(searchModeFallback("keyword", undefined).fallback).toBe(false);
  });
});

describe("searchSortIsRelevance", () => {
  test("uses the mode that actually ran, not the requested mode", () => {
    expect(searchSortIsRelevance("tokio", "hybrid", "hybrid")).toBe(true);
    expect(searchSortIsRelevance("tokio", "hybrid", "keyword")).toBe(false);
    expect(searchSortIsRelevance("", "semantic", "semantic")).toBe(false);
  });
});
