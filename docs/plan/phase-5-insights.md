# Task: Phase 5 — Insights Dashboard (v2b)

Read `CLAUDE.md` and `docs/HLD.md` §7 first. Depends on Phase 3 (Phase 4 not required).

## Goal
An Insights tab answering: when do I star, what am I into, and how has that changed.

## Build

All aggregations in `services/insights.rs` as SQL over `starred_at` (include `unstarred` rows — history counts). Convert UTC → local timezone in Rust before bucketing hour/day. Global date-range selector (all-time / 1y / 6m / custom) applies to every view.

1. **Starring timeline** — stars per week and per month; Recharts line/area with granularity toggle.
2. **Rhythm heatmap** — 24×7 hour-of-day × day-of-week matrix (custom grid of divs is fine; color scale by count). Tooltip with exact count.
3. **Interest drift** — stacked area: top-level category share per quarter. Repos without a category bucket as `Uncategorized`. Click a band → drill into that category's subcategory breakdown.
4. **Language trend** — same chart, by language, top 8 + "Other".
5. **Interest metrics table** — per top-level category: repo count, first/last starred, recent velocity (stars/month over trailing 6 months) vs lifetime velocity, badge: Rising (recent > 1.5× lifetime), Dormant (0 stars in 6 months), Steady otherwise.
6. **Fun facts panel** — longest daily starring streak, biggest single-day count + date, first-ever star, oldest repo starred.
7. **Export:** button → categorized library as Markdown (grouped by category → subcategory, linked entries) and JSON, via save-file dialog.

## Out of scope
Predictions/recommendations, cross-source analytics, unstar-specific analytics beyond inclusion in counts.

## Acceptance criteria
- For each chart, one raw-SQL spot check in the PR description showing chart value == query value.
- Heatmap hour buckets match local timezone (verify one known `starred_at` manually).
- Date-range filter provably applies to all six views.
- Library with <20 stars or <3 months of history shows graceful empty/short-history states, no NaN/divide-by-zero.
- Exported Markdown opens cleanly and matches current categories.
