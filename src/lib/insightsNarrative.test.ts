import { describe, expect, test } from "bun:test";
import type { InsightsDashboard, InterestMetric } from "../types";
import {
  deriveInsightsNarrative,
  insightsLanguageLink,
  insightsMetricLink,
  peakHeatmapCell,
} from "./insightsNarrative";

function dash(partial: Partial<InsightsDashboard> = {}): InsightsDashboard {
  return {
    meta: {
      totalStars: 42,
      spanDays: 400,
      shortHistory: false,
      rangeStart: null,
      rangeEnd: null,
    },
    timeline: { weekly: [], monthly: [] },
    heatmap: [
      { dayOfWeek: 2, hour: 21, count: 8 },
      { dayOfWeek: 0, hour: 9, count: 2 },
    ],
    interestDrift: [
      { period: "2025-Q1", name: "Developer Tools", count: 10, share: 0.4 },
      { period: "2025-Q1", name: "AI / LLM", count: 5, share: 0.2 },
    ],
    languageTrend: [],
    interestMetrics: [
      {
        category: "Developer Tools",
        repoCount: 20,
        firstStarred: "2024-01-01",
        lastStarred: "2026-01-01",
        recentVelocity: 2,
        lifetimeVelocity: 1,
        badge: "rising",
      },
      {
        category: "Games",
        repoCount: 3,
        firstStarred: "2023-01-01",
        lastStarred: "2024-06-01",
        recentVelocity: 0,
        lifetimeVelocity: 0.4,
        badge: "dormant",
      },
    ],
    funFacts: {
      longestStreakDays: 3,
      biggestDayCount: 4,
      biggestDayDate: "2025-01-01",
      firstStarAt: null,
      firstStarRepo: null,
      oldestRepoCreatedAt: null,
      oldestRepoName: null,
    },
    ...partial,
  };
}

describe("deriveInsightsNarrative", () => {
  test("derives total, peak rhythm, leading/rising, and dormant cue from the payload", () => {
    const narrative = deriveInsightsNarrative(dash());
    expect(narrative.totalStars).toBe(42);
    expect(narrative.peak).toEqual({
      day: "Wednesday",
      dayIndex: 2,
      hour: 21,
      count: 8,
    });
    expect(narrative.leading).toBe("Developer Tools");
    expect(narrative.rising).toEqual(["Developer Tools"]);
    expect(narrative.dormantCategories).toEqual(["Games"]);
    expect(narrative.sentences.some((s) => s.includes("42 stars"))).toBe(true);
    expect(
      narrative.sentences.some((s) => s.includes("no new stars in 6 months")),
    ).toBe(true);
    expect(narrative.sentences.some((s) => /push inactivity/i.test(s))).toBe(
      true,
    );
  });

  test("does not invent a peak or leading category when data is empty", () => {
    const narrative = deriveInsightsNarrative(
      dash({
        heatmap: [],
        interestMetrics: [],
        interestDrift: [],
        meta: {
          totalStars: 0,
          spanDays: 0,
          shortHistory: true,
          rangeStart: null,
          rangeEnd: null,
        },
      }),
    );
    expect(narrative.peak).toBeNull();
    expect(narrative.leading).toBeNull();
    expect(narrative.rising).toEqual([]);
    expect(narrative.sentences).toEqual(["0 stars in this period."]);
  });
});

describe("insights deep links", () => {
  test("Rising is a category deep link; Dormant is not Inactive/push inactivity", () => {
    const rising: InterestMetric = {
      category: "Rust",
      repoCount: 4,
      firstStarred: null,
      lastStarred: null,
      recentVelocity: 2,
      lifetimeVelocity: 1,
      badge: "rising",
    };
    const dormant: InterestMetric = { ...rising, badge: "dormant" };
    expect(insightsMetricLink(rising)).toEqual({
      kind: "category-name",
      name: "Rust",
    });
    expect(insightsMetricLink(dormant)).toBeNull();
    expect(insightsLanguageLink("TypeScript")).toEqual({
      kind: "language",
      name: "TypeScript",
    });
    expect(insightsLanguageLink("Other")).toBeNull();
  });

  test("peakHeatmapCell ignores zero-count cells", () => {
    expect(peakHeatmapCell([{ dayOfWeek: 1, hour: 3, count: 0 }])).toBeNull();
  });
});
