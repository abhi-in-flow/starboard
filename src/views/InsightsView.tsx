import { useQuery } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { Download } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { resolveCategoryId } from "@/lib/categories";
import {
  deriveInsightsNarrative,
  heatmapAriaLabel,
  heatmapSummary,
  insightsLanguageLink,
} from "@/lib/insightsNarrative";
import { REVIEW_PRESETS } from "@/lib/review";
import {
  getInsights,
  getInterestDriftDrilldown,
  getReviewCounts,
  listCategories,
  writeLibraryExport,
} from "@/lib/tauri";
import { usePrefersReducedMotion } from "@/lib/useMediaQuery";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";
import type {
  HeatmapCell,
  InsightsDateRange,
  InterestMetric,
  ReviewCounts,
  ReviewPreset,
  SharePoint,
} from "@/types";

const CHART_COLORS = [
  "#1d4ed8",
  "#0f766e",
  "#b45309",
  "#be123c",
  "#4338ca",
  "#047857",
  "#a16207",
  "#0e7490",
  "#7c2d12",
  "#334155",
];

const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
const HOURS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
  22, 23,
] as const;

type RangePreset = "all" | "1y" | "6m" | "custom";
type TimelineGrain = "weekly" | "monthly";

function utcOffsetMinutes() {
  return -new Date().getTimezoneOffset();
}

function pivotSharePoints(points: SharePoint[]) {
  const series = new Set<string>();
  const byPeriod = new Map<string, Record<string, string | number>>();
  for (const p of points) {
    series.add(p.name);
    const row = byPeriod.get(p.period) ?? { period: p.period };
    row[p.name] = p.share;
    byPeriod.set(p.period, row);
  }
  const keys = [...series].sort();
  const data = [...byPeriod.values()].sort((a, b) =>
    String(a.period).localeCompare(String(b.period)),
  );
  return { data, keys };
}

function heatmapColor(count: number, max: number) {
  if (count <= 0 || max <= 0) return "var(--muted)";
  const t = Math.min(1, count / max);
  const alpha = 0.18 + t * 0.82;
  return `color-mix(in oklab, #0f766e ${Math.round(alpha * 100)}%, var(--muted))`;
}

function badgeVariant(badge: string) {
  if (badge === "rising") return "default" as const;
  if (badge === "dormant") return "secondary" as const;
  return "outline" as const;
}

function formatVelocity(v: number) {
  if (!Number.isFinite(v)) return "—";
  return v.toFixed(2);
}

function EmptyHint({ children }: { children: ReactNode }) {
  return (
    <p className="py-8 text-center text-sm text-muted-foreground">{children}</p>
  );
}

function SectionCard({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <Card className="gap-4 py-4 shadow-none">
      <CardHeader className="px-4 [.border-b]:pb-4">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="space-y-1">
            <CardTitle className="text-base">{title}</CardTitle>
            {description ? (
              <CardDescription>{description}</CardDescription>
            ) : null}
          </div>
          {action}
        </div>
      </CardHeader>
      <CardContent className="px-4">{children}</CardContent>
    </Card>
  );
}

function RhythmHeatmap({ cells }: { cells: HeatmapCell[] }) {
  const max = cells.reduce((m, c) => Math.max(m, c.count), 0);
  const lookup = new Map(
    cells.map((c) => [`${c.dayOfWeek}-${c.hour}`, c.count]),
  );

  if (cells.length === 0) {
    return <EmptyHint>No starring activity in this range.</EmptyHint>;
  }

  const summary = heatmapSummary(cells);
  return (
    <div
      className="overflow-x-auto"
      role="img"
      aria-labelledby="insights-heatmap-summary"
    >
      <p id="insights-heatmap-summary" className="sr-only">
        {summary}
      </p>
      <div
        className="inline-grid min-w-full grid-cols-[auto_repeat(24,minmax(0.75rem,1fr))] gap-0.5"
        aria-hidden
      >
        <div />
        {HOURS.map((hour) => (
          <div
            key={`h-${hour}`}
            className="text-center text-[10px] text-muted-foreground"
          >
            {hour % 3 === 0 ? hour : ""}
          </div>
        ))}
        {DAY_LABELS.map((label, dayIndex) => (
          <div key={label} className="contents">
            <div className="pr-2 text-right text-xs text-muted-foreground">
              {label}
            </div>
            {HOURS.map((hour) => {
              const count = lookup.get(`${dayIndex}-${hour}`) ?? 0;
              const name = heatmapAriaLabel(label, hour, count);
              return (
                <div
                  key={`${label}-${hour}`}
                  title={name}
                  aria-hidden
                  className="aspect-square rounded-[2px] border border-transparent"
                  style={{ background: heatmapColor(count, max) }}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function StackedShareChart({
  points,
  onBandClick,
}: {
  points: SharePoint[];
  onBandClick?: (name: string) => void;
}) {
  const reduceMotion = usePrefersReducedMotion();
  const { data, keys } = useMemo(() => pivotSharePoints(points), [points]);
  if (data.length === 0) {
    return <EmptyHint>Not enough categorized history to chart.</EmptyHint>;
  }
  return (
    <div className="h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart
          data={data}
          margin={{ top: 8, right: 8, left: 0, bottom: 0 }}
        >
          <CartesianGrid strokeDasharray="3 3" className="stroke-border" />
          <XAxis dataKey="period" tick={{ fontSize: 11 }} />
          <YAxis
            tick={{ fontSize: 11 }}
            tickFormatter={(v) => `${Math.round(Number(v) * 100)}%`}
            domain={[0, 1]}
          />
          <Tooltip
            formatter={(value, name) => [
              `${Math.round(Number(value) * 100)}%`,
              String(name),
            ]}
          />
          <Legend
            wrapperStyle={{ fontSize: 12 }}
            onClick={(e) => {
              if (onBandClick && typeof e.dataKey === "string") {
                onBandClick(e.dataKey);
              }
            }}
          />
          {keys.map((key, i) => (
            <Area
              key={key}
              type="monotone"
              dataKey={key}
              stackId="1"
              stroke={CHART_COLORS[i % CHART_COLORS.length]}
              fill={CHART_COLORS[i % CHART_COLORS.length]}
              fillOpacity={0.75}
              isAnimationActive={!reduceMotion}
              style={{ cursor: onBandClick ? "pointer" : undefined }}
              onClick={() => onBandClick?.(key)}
            />
          ))}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

function ReviewQueuesCard({
  counts,
  onOpen,
}: {
  counts: ReviewCounts | undefined;
  onOpen: (preset: ReviewPreset) => void;
}) {
  const links: { preset: ReviewPreset; hint: string }[] = [
    {
      preset: "inactive",
      hint: "No push in 12+ months. This is repository inactivity, not category-velocity Dormant.",
    },
    {
      preset: "archived",
      hint: "Archived on GitHub, still starred.",
    },
    {
      preset: "uncategorized",
      hint: "Active stars without a real category.",
    },
  ];
  return (
    <SectionCard
      title="Review queues"
      description="Jump to Library review presets. Counts exclude reviewed and snoozed repos. Unstarred history is listed separately and is not an active count."
    >
      <div className="grid gap-2 sm:grid-cols-3">
        {links.map(({ preset, hint }) => {
          const meta = REVIEW_PRESETS.find((p) => p.id === preset);
          const n = counts?.[preset];
          return (
            <button
              key={preset}
              type="button"
              onClick={() => onOpen(preset)}
              className="rounded-md border bg-muted/30 px-3 py-2 text-left transition-colors hover:bg-muted/60"
            >
              <p className="text-sm font-medium">{meta?.label ?? preset}</p>
              <p className="mt-0.5 text-xs text-muted-foreground">{hint}</p>
              <p className="mt-1 text-sm tabular-nums">
                {n == null ? "—" : `${n.toLocaleString()} in queue`}
              </p>
            </button>
          );
        })}
      </div>
      {counts && counts.activeStars === 0 ? (
        <p className="mt-3 text-xs text-muted-foreground">
          Sync stars first. Review is local-only and never writes back to
          GitHub.
        </p>
      ) : null}
    </SectionCard>
  );
}

function MetricsTable({
  rows,
  onCategory,
}: {
  rows: InterestMetric[];
  onCategory?: (name: string) => void;
}) {
  if (rows.length === 0) {
    return <EmptyHint>No category metrics for this range.</EmptyHint>;
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[36rem] text-left text-sm">
        <thead className="border-b text-xs text-muted-foreground">
          <tr>
            <th className="py-2 pr-3 font-medium">Category</th>
            <th className="py-2 pr-3 font-medium">Repos</th>
            <th className="py-2 pr-3 font-medium">First</th>
            <th className="py-2 pr-3 font-medium">Last</th>
            <th className="py-2 pr-3 font-medium">Recent / mo</th>
            <th className="py-2 pr-3 font-medium">Lifetime / mo</th>
            <th className="py-2 font-medium">Trend</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.category} className="border-b border-border/60">
              <td className="py-2 pr-3 font-medium">
                {onCategory ? (
                  <button
                    type="button"
                    className="hover:underline"
                    onClick={() => onCategory(row.category)}
                  >
                    {row.category}
                  </button>
                ) : (
                  row.category
                )}
              </td>
              <td className="py-2 pr-3 tabular-nums">{row.repoCount}</td>
              <td className="py-2 pr-3 text-muted-foreground">
                {row.firstStarred?.slice(0, 10) ?? "—"}
              </td>
              <td className="py-2 pr-3 text-muted-foreground">
                {row.lastStarred?.slice(0, 10) ?? "—"}
              </td>
              <td className="py-2 pr-3 tabular-nums">
                {formatVelocity(row.recentVelocity)}
              </td>
              <td className="py-2 pr-3 tabular-nums">
                {formatVelocity(row.lifetimeVelocity)}
              </td>
              <td className="py-2">
                {row.badge === "rising" && onCategory ? (
                  <button
                    type="button"
                    onClick={() => onCategory(row.category)}
                    title="Open this category in the library"
                  >
                    <Badge
                      variant={badgeVariant(row.badge)}
                      className="capitalize"
                    >
                      {row.badge}
                    </Badge>
                  </button>
                ) : (
                  <Badge
                    variant={badgeVariant(row.badge)}
                    className="capitalize"
                    title={
                      row.badge === "dormant"
                        ? "No new stars in this category for 6 months (starring velocity, not repo push activity)"
                        : undefined
                    }
                  >
                    {row.badge}
                  </Badge>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function InsightsView() {
  const [preset, setPreset] = useState<RangePreset>("all");
  const [customStart, setCustomStart] = useState("");
  const [customEnd, setCustomEnd] = useState("");
  const [grain, setGrain] = useState<TimelineGrain>("weekly");
  const [drillCategory, setDrillCategory] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const openLibraryReview = useUiStore((s) => s.openLibraryReview);
  const openLibraryCategory = useUiStore((s) => s.openLibraryCategory);
  const openLibraryLanguage = useUiStore((s) => s.openLibraryLanguage);
  const reduceMotion = usePrefersReducedMotion();

  const range: InsightsDateRange = useMemo(() => {
    if (preset === "custom") {
      return {
        preset: "custom",
        start: customStart ? `${customStart}T00:00:00Z` : null,
        end: customEnd ? `${customEnd}T23:59:59Z` : null,
      };
    }
    return { preset, start: null, end: null };
  }, [preset, customStart, customEnd]);

  const offset = utcOffsetMinutes();

  const insightsQuery = useQuery({
    queryKey: ["insights", range, offset],
    queryFn: () =>
      getInsights({
        range,
        utcOffsetMinutes: offset,
      }),
    enabled:
      preset !== "custom" ||
      (Boolean(customStart) && Boolean(customEnd) && customStart <= customEnd),
  });

  const drillQuery = useQuery({
    queryKey: ["insights-drill", drillCategory, range, offset],
    queryFn: () =>
      getInterestDriftDrilldown(drillCategory ?? "", range, offset),
    enabled: Boolean(drillCategory),
  });

  const reviewCounts = useQuery({
    queryKey: ["reviewCounts"],
    queryFn: getReviewCounts,
  });

  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const dash = insightsQuery.data;
  const narrative = useMemo(
    () => (dash ? deriveInsightsNarrative(dash) : null),
    [dash],
  );

  function openCategoryName(name: string) {
    const id = resolveCategoryId(categories.data ?? [], name);
    if (id != null) {
      openLibraryCategory(id);
    }
  }

  function openLanguageName(name: string) {
    if (insightsLanguageLink(name)) {
      openLibraryLanguage(name);
    }
  }
  const timeline =
    grain === "weekly" ? dash?.timeline.weekly : dash?.timeline.monthly;

  async function handleExport(format: "markdown" | "json") {
    setExportError(null);
    setExporting(true);
    try {
      const path = await save({
        defaultPath:
          format === "markdown"
            ? "starboard-library.md"
            : "starboard-library.json",
        filters: [
          format === "markdown"
            ? { name: "Markdown", extensions: ["md"] }
            : { name: "JSON", extensions: ["json"] },
        ],
      });
      if (!path) return;
      await writeLibraryExport(path, format);
    } catch (err) {
      const message =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: string }).message)
          : "Export failed";
      setExportError(message);
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Insights</h1>
          <p className="text-sm text-muted-foreground">
            When you star, what you are into, and how that has shifted.
          </p>
          <p className="text-xs text-muted-foreground">
            Export writes to the path you pick in the save dialog. Extension and
            path checks are validation hygiene, not a filesystem sandbox.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={exporting}
            onClick={() => void handleExport("markdown")}
          >
            <Download className="size-4" />
            Export MD
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={exporting}
            onClick={() => void handleExport("json")}
          >
            <Download className="size-4" />
            Export JSON
          </Button>
        </div>
      </div>

      {exportError ? (
        <p className="text-sm text-destructive" role="alert">
          {exportError}
        </p>
      ) : null}

      <div className="flex flex-wrap items-end gap-3 rounded-lg border bg-card px-3 py-3">
        <div className="space-y-1">
          <p className="text-xs font-medium text-muted-foreground">
            Date range
          </p>
          <Select
            value={preset}
            onValueChange={(v) => setPreset(v as RangePreset)}
          >
            <SelectTrigger className="w-40" aria-label="Insights date range">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All time</SelectItem>
              <SelectItem value="1y">Last year</SelectItem>
              <SelectItem value="6m">Last 6 months</SelectItem>
              <SelectItem value="custom">Custom</SelectItem>
            </SelectContent>
          </Select>
        </div>
        {preset === "custom" ? (
          <>
            <div className="space-y-1">
              <label
                htmlFor="insights-from"
                className="text-xs font-medium text-muted-foreground"
              >
                From
              </label>
              <Input
                id="insights-from"
                type="date"
                value={customStart}
                onChange={(e) => setCustomStart(e.target.value)}
                className="w-40"
              />
            </div>
            <div className="space-y-1">
              <label
                htmlFor="insights-to"
                className="text-xs font-medium text-muted-foreground"
              >
                To
              </label>
              <Input
                id="insights-to"
                type="date"
                value={customEnd}
                onChange={(e) => setCustomEnd(e.target.value)}
                className="w-40"
              />
            </div>
          </>
        ) : null}
        {dash ? (
          <p className="ml-auto text-xs text-muted-foreground">
            {dash.meta.totalStars} stars
            {dash.meta.spanDays > 0
              ? ` · ${dash.meta.spanDays} day span`
              : null}
          </p>
        ) : null}
      </div>

      {insightsQuery.isLoading ? (
        <p className="text-sm text-muted-foreground">Loading insights…</p>
      ) : null}
      {insightsQuery.isError ? (
        <p className="text-sm text-destructive">
          Failed to load insights.{" "}
          {insightsQuery.error instanceof Error
            ? insightsQuery.error.message
            : null}
        </p>
      ) : null}

      {narrative && dash && dash.meta.totalStars > 0 ? (
        <section className="rounded-lg border bg-card px-4 py-3">
          <h2 className="text-sm font-semibold">This period</h2>
          <p className="mt-1 text-sm text-foreground/80">
            {narrative.sentences.join(" ")}
          </p>
          {narrative.rising.length > 0 ||
          narrative.dormantCategories.length > 0 ? (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {narrative.rising.map((name) => (
                <button
                  key={`rise-${name}`}
                  type="button"
                  onClick={() => openCategoryName(name)}
                >
                  <Badge className="font-normal">Rising: {name}</Badge>
                </button>
              ))}
              {narrative.dormantCategories.map((name) => (
                <Badge
                  key={`dorm-${name}`}
                  variant="secondary"
                  className="font-normal"
                  title="No new stars in this category for 6 months — starring velocity, not repository push activity"
                >
                  Dormant: {name}
                </Badge>
              ))}
            </div>
          ) : null}
        </section>
      ) : null}

      {dash?.meta.shortHistory ? (
        <div className="rounded-md border border-dashed bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
          Short history (&lt;20 stars or &lt;3 months). Charts still render from
          what you have — treat trends as provisional, not forecasts.
        </div>
      ) : null}

      {dash && dash.meta.totalStars === 0 ? (
        <EmptyHint>
          No starred repos in this range yet. Sync your library to populate
          insights.
        </EmptyHint>
      ) : null}

      <ReviewQueuesCard
        counts={reviewCounts.data}
        onOpen={(preset) => openLibraryReview({ preset })}
      />

      {dash && dash.meta.totalStars > 0 ? (
        <div className="grid gap-4">
          <SectionCard
            title="Starring timeline"
            description="Stars per week or month (includes later-unstarred history)."
            action={
              <div className="flex gap-1">
                {(["weekly", "monthly"] as const).map((g) => (
                  <Button
                    key={g}
                    size="xs"
                    variant={grain === g ? "default" : "outline"}
                    className="capitalize"
                    aria-pressed={grain === g}
                    onClick={() => setGrain(g)}
                  >
                    {g}
                  </Button>
                ))}
              </div>
            }
          >
            {!timeline || timeline.length === 0 ? (
              <EmptyHint>No timeline points.</EmptyHint>
            ) : (
              <div className="h-64 w-full">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart
                    data={timeline}
                    margin={{ top: 8, right: 8, left: 0, bottom: 0 }}
                  >
                    <CartesianGrid
                      strokeDasharray="3 3"
                      className="stroke-border"
                    />
                    <XAxis dataKey="period" tick={{ fontSize: 11 }} />
                    <YAxis allowDecimals={false} tick={{ fontSize: 11 }} />
                    <Tooltip />
                    <Line
                      type="monotone"
                      dataKey="count"
                      stroke="#0f766e"
                      strokeWidth={2}
                      dot={false}
                      isAnimationActive={!reduceMotion}
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            )}
          </SectionCard>

          <SectionCard
            title="Rhythm heatmap"
            description="Local-time hour × day-of-week. Hover a cell for the exact count."
          >
            <RhythmHeatmap cells={dash.heatmap} />
          </SectionCard>

          <SectionCard
            title="Interest drift"
            description="Top-level category share by quarter. Click a band or legend entry to drill into subcategories."
            action={
              drillCategory ? (
                <Button
                  size="xs"
                  variant="ghost"
                  onClick={() => setDrillCategory(null)}
                >
                  Back to top-level
                </Button>
              ) : null
            }
          >
            {drillCategory ? (
              <div className="space-y-2">
                <p className="text-sm text-muted-foreground">
                  Subcategories of{" "}
                  <span className="font-medium text-foreground">
                    {drillCategory}
                  </span>
                </p>
                {drillQuery.isLoading ? (
                  <EmptyHint>Loading breakdown…</EmptyHint>
                ) : (
                  <StackedShareChart points={drillQuery.data ?? []} />
                )}
              </div>
            ) : (
              <StackedShareChart
                points={dash.interestDrift}
                onBandClick={setDrillCategory}
              />
            )}
          </SectionCard>

          <SectionCard
            title="Language trend"
            description="Language share by quarter (top 8 + Other)."
          >
            <StackedShareChart
              points={dash.languageTrend}
              onBandClick={openLanguageName}
            />
          </SectionCard>

          <SectionCard
            title="Interest metrics"
            description="Rising = recent velocity &gt; 1.5× lifetime; Dormant = no stars in 6 months."
          >
            <p className="mb-3 text-xs text-muted-foreground">
              Dormant here means no new stars in that category for 6 months, not
              that the repositories stopped receiving pushes. For repos with no
              push in 12+ months, use the Inactive review queue above.
            </p>
            <MetricsTable
              rows={dash.interestMetrics}
              onCategory={openCategoryName}
            />
          </SectionCard>

          <SectionCard title="Fun facts">
            <dl className="grid gap-3 sm:grid-cols-2">
              <Fact
                label="Longest daily streak"
                value={`${dash.funFacts.longestStreakDays} day${dash.funFacts.longestStreakDays === 1 ? "" : "s"}`}
              />
              <Fact
                label="Biggest single day"
                value={
                  dash.funFacts.biggestDayDate
                    ? `${dash.funFacts.biggestDayCount} on ${dash.funFacts.biggestDayDate}`
                    : "—"
                }
              />
              <Fact
                label="First-ever star"
                value={
                  dash.funFacts.firstStarRepo
                    ? `${dash.funFacts.firstStarRepo}${dash.funFacts.firstStarAt ? ` · ${dash.funFacts.firstStarAt.slice(0, 10)}` : ""}`
                    : "—"
                }
              />
              <Fact
                label="Oldest repo starred"
                value={
                  dash.funFacts.oldestRepoName
                    ? `${dash.funFacts.oldestRepoName}${dash.funFacts.oldestRepoCreatedAt ? ` · created ${dash.funFacts.oldestRepoCreatedAt.slice(0, 10)}` : ""}`
                    : "—"
                }
              />
            </dl>
          </SectionCard>
        </div>
      ) : null}
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className={cn("rounded-md border bg-muted/30 px-3 py-2")}>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-1 text-sm font-medium break-all">{value}</dd>
    </div>
  );
}
