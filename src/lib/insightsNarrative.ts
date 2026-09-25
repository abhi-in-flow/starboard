import type {
  HeatmapCell,
  InsightsDashboard,
  InterestMetric,
  ReviewPreset,
  SharePoint,
} from "@/types";

const DAY_LABELS = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];

export type InsightsPeakRhythm = {
  day: string;
  dayIndex: number;
  hour: number;
  count: number;
};

export type InsightsNarrative = {
  totalStars: number;
  shortHistory: boolean;
  peak: InsightsPeakRhythm | null;
  leading: string | null;
  rising: string[];
  dormantCategories: string[];
  sentences: string[];
};

export type InsightsDeepLink =
  | { kind: "category-name"; name: string }
  | { kind: "language"; name: string }
  | { kind: "review"; preset: ReviewPreset };

export function peakHeatmapCell(
  cells: HeatmapCell[],
): InsightsPeakRhythm | null {
  if (cells.length === 0) {
    return null;
  }
  let best = cells[0];
  for (const cell of cells) {
    if (cell.count > best.count) {
      best = cell;
    }
  }
  if (best.count <= 0) {
    return null;
  }
  return {
    day: DAY_LABELS[best.dayOfWeek] ?? `Day ${best.dayOfWeek}`,
    dayIndex: best.dayOfWeek,
    hour: best.hour,
    count: best.count,
  };
}

function latestLeadingShare(points: SharePoint[]): string | null {
  if (points.length === 0) {
    return null;
  }
  const lastPeriod = points.reduce(
    (max, p) => (p.period > max ? p.period : max),
    points[0].period,
  );
  const inPeriod = points.filter((p) => p.period === lastPeriod);
  if (inPeriod.length === 0) {
    return null;
  }
  const top = inPeriod.reduce((a, b) => (b.share > a.share ? b : a));
  return top.name || null;
}

function leadingFromMetrics(metrics: InterestMetric[]): string | null {
  if (metrics.length === 0) {
    return null;
  }
  const top = metrics.reduce((a, b) => (b.repoCount > a.repoCount ? b : a));
  return top.repoCount > 0 ? top.category : null;
}

export function deriveInsightsNarrative(
  dash: InsightsDashboard,
): InsightsNarrative {
  const peak = peakHeatmapCell(dash.heatmap);
  const leading =
    leadingFromMetrics(dash.interestMetrics) ??
    latestLeadingShare(dash.interestDrift);
  const rising = dash.interestMetrics
    .filter((m) => m.badge === "rising")
    .map((m) => m.category);
  const dormantCategories = dash.interestMetrics
    .filter((m) => m.badge === "dormant")
    .map((m) => m.category);

  const sentences: string[] = [];
  const total = dash.meta.totalStars;
  sentences.push(
    total === 1
      ? "1 star in this period."
      : `${total.toLocaleString()} stars in this period.`,
  );
  if (peak) {
    sentences.push(
      `Peak rhythm ${peak.day} at ${String(peak.hour).padStart(2, "0")}:00 (${peak.count} star${peak.count === 1 ? "" : "s"}).`,
    );
  }
  if (leading) {
    sentences.push(`Leading interest: ${leading}.`);
  }
  if (rising.length > 0) {
    sentences.push(`Rising: ${rising.join(", ")}.`);
  }
  if (dormantCategories.length > 0) {
    sentences.push(
      `Dormant (no new stars in 6 months, not repo push inactivity): ${dormantCategories.join(", ")}.`,
    );
  }

  return {
    totalStars: total,
    shortHistory: dash.meta.shortHistory,
    peak,
    leading,
    rising,
    dormantCategories,
    sentences,
  };
}

/** Rising/category chips may deep-link; category-velocity Dormant must not claim repo inactivity. */
export function insightsMetricLink(
  metric: InterestMetric,
): InsightsDeepLink | null {
  if (metric.badge === "dormant") {
    return null;
  }
  if (metric.category.trim().length === 0) {
    return null;
  }
  return { kind: "category-name", name: metric.category };
}

export function insightsLanguageLink(name: string): InsightsDeepLink | null {
  if (!name.trim() || name.toLowerCase() === "other") {
    return null;
  }
  return { kind: "language", name };
}

export function heatmapAriaLabel(
  dayLabel: string,
  hour: number,
  count: number,
): string {
  return `${dayLabel} ${String(hour).padStart(2, "0")}:00, ${count} star${count === 1 ? "" : "s"}`;
}

export function heatmapSummary(cells: HeatmapCell[]): string {
  const peak = peakHeatmapCell(cells);
  const total = cells.reduce((n, c) => n + c.count, 0);
  if (!peak) {
    return `Starring rhythm heatmap. ${total} stars across the week.`;
  }
  return `Starring rhythm heatmap. ${total} stars across the week. Peak ${peak.day} at ${String(peak.hour).padStart(2, "0")}:00 with ${peak.count}.`;
}
