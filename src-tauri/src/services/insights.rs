//! Insights aggregations over `starred_at` (includes soft-deleted / unstarred rows).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime, UtcOffset};

use crate::error::{AppError, AppResult};
use crate::models::{
    FunFacts, HeatmapCell, InsightsDashboard, InsightsDateRange, InsightsMeta, InsightsRequest,
    InterestMetric, LibraryExport, SharePoint, StarringTimeline, TimelineBucket,
};

const SHORT_HISTORY_STARS: i64 = 20;
const SHORT_HISTORY_DAYS: i64 = 90; // ~3 months
const RISING_RATIO: f64 = 1.5;
const RECENT_MONTHS: f64 = 6.0;

#[derive(Debug, Clone)]
struct RangeBounds {
    start: Option<OffsetDateTime>,
    end: Option<OffsetDateTime>,
}

pub fn get_insights(conn: &Connection, req: InsightsRequest) -> AppResult<InsightsDashboard> {
    let offset = utc_offset(req.utc_offset_minutes);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let bounds = resolve_range(req.range.as_ref(), now)?;
    let starred = load_starred_rows(conn, &bounds)?;

    let meta = build_meta(&starred, &bounds);
    let timeline = build_timeline(&starred, offset);
    let heatmap = build_heatmap(&starred, offset);
    let interest_drift = build_interest_drift(conn, &starred, offset)?;
    let language_trend = build_language_trend(&starred, offset);
    let interest_metrics = build_interest_metrics(conn, &starred, now)?;
    let fun_facts = build_fun_facts(conn, &starred, offset)?;

    Ok(InsightsDashboard {
        meta,
        timeline,
        heatmap,
        interest_drift,
        language_trend,
        interest_metrics,
        fun_facts,
    })
}

pub fn interest_drift_drilldown(
    conn: &Connection,
    range: Option<InsightsDateRange>,
    category: &str,
    utc_offset_minutes: Option<i32>,
) -> AppResult<Vec<SharePoint>> {
    let offset = utc_offset(utc_offset_minutes);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let bounds = resolve_range(range.as_ref(), now)?;
    let starred = load_starred_rows(conn, &bounds)?;
    build_subcategory_drift(conn, &starred, category, offset)
}

pub fn export_library(conn: &Connection) -> AppResult<LibraryExport> {
    let markdown = export_markdown(conn)?;
    let json = export_json(conn)?;
    Ok(LibraryExport { markdown, json })
}

pub fn write_export_file(path: &str, content: &str) -> AppResult<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::new("io_error", format!("failed to create export directory: {e}"))
            })?;
        }
    }
    std::fs::write(p, content)
        .map_err(|e| AppError::new("io_error", format!("failed to write export: {e}")))
}

fn utc_offset(minutes: Option<i32>) -> UtcOffset {
    let mins = minutes.unwrap_or(0);
    UtcOffset::from_whole_seconds(mins.saturating_mul(60)).unwrap_or(UtcOffset::UTC)
}

fn resolve_range(
    range: Option<&InsightsDateRange>,
    now: OffsetDateTime,
) -> AppResult<RangeBounds> {
    let Some(range) = range else {
        return Ok(RangeBounds {
            start: None,
            end: None,
        });
    };
    let preset = range
        .preset
        .as_deref()
        .unwrap_or("all")
        .to_ascii_lowercase();
    match preset.as_str() {
        "all" | "" => Ok(RangeBounds {
            start: None,
            end: None,
        }),
        "1y" => Ok(RangeBounds {
            start: Some(now - Duration::days(365)),
            end: Some(now),
        }),
        "6m" => Ok(RangeBounds {
            start: Some(now - Duration::days(183)),
            end: Some(now),
        }),
        "custom" => {
            let start = range
                .start
                .as_deref()
                .map(parse_bound)
                .transpose()?;
            let end = range.end.as_deref().map(parse_bound).transpose()?;
            Ok(RangeBounds { start, end })
        }
        other => Err(AppError::new(
            "validation_error",
            format!("unknown date-range preset: {other}"),
        )),
    }
}

fn parse_bound(s: &str) -> AppResult<OffsetDateTime> {
    if let Ok(dt) = OffsetDateTime::parse(s, &Rfc3339) {
        return Ok(dt);
    }
    // Accept date-only YYYY-MM-DD as start-of-day UTC.
    if s.len() == 10 {
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .map_err(|e| AppError::new("validation_error", e.to_string()))?;
        let date = Date::parse(s, &format)
            .map_err(|e| AppError::new("validation_error", format!("invalid date '{s}': {e}")))?;
        return Ok(date
            .with_hms(0, 0, 0)
            .map_err(|e| AppError::new("validation_error", format!("invalid date '{s}': {e}")))?
            .assume_utc());
    }
    Err(AppError::new(
        "validation_error",
        format!("invalid datetime '{s}'"),
    ))
}

#[derive(Debug, Clone)]
struct StarredRow {
    id: i64,
    full_name: String,
    language: Option<String>,
    starred_at: OffsetDateTime,
    repo_created_at: Option<String>,
}

fn load_starred_rows(conn: &Connection, bounds: &RangeBounds) -> AppResult<Vec<StarredRow>> {
    // Include unstarred rows — history counts for insights.
    let mut sql = String::from(
        "SELECT id, full_name, language, starred_at, repo_created_at FROM repos WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();
    if let Some(start) = bounds.start {
        sql.push_str(" AND starred_at >= ?");
        binds.push(start.format(&Rfc3339).unwrap_or_default());
    }
    if let Some(end) = bounds.end {
        sql.push_str(" AND starred_at <= ?");
        binds.push(end.format(&Rfc3339).unwrap_or_default());
    }
    sql.push_str(" ORDER BY starred_at ASC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |row| {
        let starred_raw: String = row.get(3)?;
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            starred_raw,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id, full_name, language, starred_raw, repo_created_at) = row?;
        let Ok(starred_at) = OffsetDateTime::parse(&starred_raw, &Rfc3339) else {
            continue;
        };
        out.push(StarredRow {
            id,
            full_name,
            language,
            starred_at,
            repo_created_at,
        });
    }
    Ok(out)
}

fn build_meta(starred: &[StarredRow], bounds: &RangeBounds) -> InsightsMeta {
    let total_stars = starred.len() as i64;
    let span_days = if let (Some(first), Some(last)) = (starred.first(), starred.last()) {
        (last.starred_at - first.starred_at).whole_days().max(0)
    } else {
        0
    };
    let short_history = total_stars < SHORT_HISTORY_STARS || span_days < SHORT_HISTORY_DAYS;
    InsightsMeta {
        total_stars,
        span_days,
        short_history,
        range_start: bounds
            .start
            .and_then(|d| d.format(&Rfc3339).ok()),
        range_end: bounds.end.and_then(|d| d.format(&Rfc3339).ok()),
    }
}

fn build_timeline(starred: &[StarredRow], offset: UtcOffset) -> StarringTimeline {
    let mut weekly: BTreeMap<String, i64> = BTreeMap::new();
    let mut monthly: BTreeMap<String, i64> = BTreeMap::new();
    for row in starred {
        let local = row.starred_at.to_offset(offset);
        let week = iso_week_key(local.date());
        let month = format!("{:04}-{:02}", local.year(), local.month() as u8);
        *weekly.entry(week).or_insert(0) += 1;
        *monthly.entry(month).or_insert(0) += 1;
    }
    StarringTimeline {
        weekly: weekly
            .into_iter()
            .map(|(period, count)| TimelineBucket { period, count })
            .collect(),
        monthly: monthly
            .into_iter()
            .map(|(period, count)| TimelineBucket { period, count })
            .collect(),
    }
}

fn iso_week_key(date: Date) -> String {
    let (year, week, _) = date.to_iso_week_date();
    format!("{year}-W{week:02}")
}

fn build_heatmap(starred: &[StarredRow], offset: UtcOffset) -> Vec<HeatmapCell> {
    let mut grid = [[0i64; 24]; 7];
    for row in starred {
        let local = row.starred_at.to_offset(offset);
        let dow = match local.weekday() {
            time::Weekday::Monday => 0,
            time::Weekday::Tuesday => 1,
            time::Weekday::Wednesday => 2,
            time::Weekday::Thursday => 3,
            time::Weekday::Friday => 4,
            time::Weekday::Saturday => 5,
            time::Weekday::Sunday => 6,
        };
        let hour = local.hour() as usize;
        grid[dow][hour] += 1;
    }
    let mut cells = Vec::new();
    for (day, hours) in grid.iter().enumerate() {
        for (hour, count) in hours.iter().enumerate() {
            if *count > 0 {
                cells.push(HeatmapCell {
                    day_of_week: day as u8,
                    hour: hour as u8,
                    count: *count,
                });
            }
        }
    }
    cells
}

/// Resolve each repo's primary category to a top-level name (or Uncategorized).
fn primary_top_level_map(conn: &Connection) -> AppResult<HashMap<i64, String>> {
    let mut stmt = conn.prepare(
        "SELECT rc.repo_id,
                COALESCE(parent.name, c.name) AS top_name,
                rc.source,
                rc.confidence,
                c.parent_id
         FROM repo_categories rc
         JOIN categories c ON c.id = rc.category_id
         LEFT JOIN categories parent ON parent.id = c.parent_id
         ORDER BY rc.repo_id,
                  CASE WHEN rc.source = 'manual' THEN 0 ELSE 1 END,
                  rc.confidence DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<f64>>(3)?,
        ))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (repo_id, top_name, _source, _conf) = row?;
        map.entry(repo_id).or_insert(top_name);
    }
    Ok(map)
}

fn primary_category_detail(
    conn: &Connection,
) -> AppResult<HashMap<i64, (String, Option<String>)>> {
    // Returns (top_level, subcategory_name) for primary assignment.
    let mut stmt = conn.prepare(
        "SELECT rc.repo_id,
                CASE WHEN c.parent_id IS NULL THEN c.name ELSE parent.name END AS top_name,
                CASE WHEN c.parent_id IS NULL THEN NULL ELSE c.name END AS sub_name,
                rc.source,
                rc.confidence
         FROM repo_categories rc
         JOIN categories c ON c.id = rc.category_id
         LEFT JOIN categories parent ON parent.id = c.parent_id
         ORDER BY rc.repo_id,
                  CASE WHEN rc.source = 'manual' THEN 0 ELSE 1 END,
                  rc.confidence DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (repo_id, top, sub) = row?;
        map.entry(repo_id).or_insert((top, sub));
    }
    Ok(map)
}

fn build_interest_drift(
    conn: &Connection,
    starred: &[StarredRow],
    offset: UtcOffset,
) -> AppResult<Vec<SharePoint>> {
    let cat_map = primary_top_level_map(conn)?;
    let mut by_period: BTreeMap<String, HashMap<String, i64>> = BTreeMap::new();
    for row in starred {
        let local = row.starred_at.to_offset(offset);
        let period = quarter_key(local);
        let name = cat_map
            .get(&row.id)
            .cloned()
            .unwrap_or_else(|| "Uncategorized".into());
        *by_period
            .entry(period)
            .or_default()
            .entry(name)
            .or_insert(0) += 1;
    }
    Ok(share_points_from_periods(by_period))
}

fn build_subcategory_drift(
    conn: &Connection,
    starred: &[StarredRow],
    category: &str,
    offset: UtcOffset,
) -> AppResult<Vec<SharePoint>> {
    let detail = primary_category_detail(conn)?;
    let mut by_period: BTreeMap<String, HashMap<String, i64>> = BTreeMap::new();
    for row in starred {
        let Some((top, sub)) = detail.get(&row.id) else {
            if category.eq_ignore_ascii_case("Uncategorized") {
                let local = row.starred_at.to_offset(offset);
                let period = quarter_key(local);
                *by_period
                    .entry(period)
                    .or_default()
                    .entry("(none)".into())
                    .or_insert(0) += 1;
            }
            continue;
        };
        if !top.eq_ignore_ascii_case(category) {
            continue;
        }
        let local = row.starred_at.to_offset(offset);
        let period = quarter_key(local);
        let name = sub.clone().unwrap_or_else(|| "(top-level)".into());
        *by_period
            .entry(period)
            .or_default()
            .entry(name)
            .or_insert(0) += 1;
    }
    Ok(share_points_from_periods(by_period))
}

fn build_language_trend(starred: &[StarredRow], offset: UtcOffset) -> Vec<SharePoint> {
    // First pass: total per language to pick top 8.
    let mut totals: HashMap<String, i64> = HashMap::new();
    for row in starred {
        let lang = row
            .language
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown")
            .to_string();
        *totals.entry(lang).or_insert(0) += 1;
    }
    let mut ranked: Vec<_> = totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top: HashSet<String> = ranked.into_iter().take(8).map(|(n, _)| n).collect();

    let mut by_period: BTreeMap<String, HashMap<String, i64>> = BTreeMap::new();
    for row in starred {
        let local = row.starred_at.to_offset(offset);
        let period = quarter_key(local);
        let lang = row
            .language
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown")
            .to_string();
        let name = if top.contains(&lang) {
            lang
        } else {
            "Other".into()
        };
        *by_period
            .entry(period)
            .or_default()
            .entry(name)
            .or_insert(0) += 1;
    }
    share_points_from_periods(by_period)
}

fn quarter_key(local: OffsetDateTime) -> String {
    let q = ((local.month() as u8 - 1) / 3) + 1;
    format!("{:04}-Q{q}", local.year())
}

fn share_points_from_periods(
    by_period: BTreeMap<String, HashMap<String, i64>>,
) -> Vec<SharePoint> {
    let mut out = Vec::new();
    for (period, counts) in by_period {
        let total: i64 = counts.values().sum();
        let mut names: Vec<_> = counts.into_iter().collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (name, count) in names {
            let share = if total > 0 {
                count as f64 / total as f64
            } else {
                0.0
            };
            out.push(SharePoint {
                period: period.clone(),
                name,
                count,
                share,
            });
        }
    }
    out
}

fn build_interest_metrics(
    conn: &Connection,
    starred: &[StarredRow],
    now: OffsetDateTime,
) -> AppResult<Vec<InterestMetric>> {
    let cat_map = primary_top_level_map(conn)?;
    let recent_cutoff = now - Duration::days(183);

    #[derive(Default)]
    struct Acc {
        count: i64,
        recent: i64,
        first: Option<OffsetDateTime>,
        last: Option<OffsetDateTime>,
    }

    let mut by_cat: BTreeMap<String, Acc> = BTreeMap::new();
    for row in starred {
        let name = cat_map
            .get(&row.id)
            .cloned()
            .unwrap_or_else(|| "Uncategorized".into());
        let acc = by_cat.entry(name).or_default();
        acc.count += 1;
        if row.starred_at >= recent_cutoff {
            acc.recent += 1;
        }
        match acc.first {
            Some(f) if row.starred_at < f => acc.first = Some(row.starred_at),
            None => acc.first = Some(row.starred_at),
            _ => {}
        }
        match acc.last {
            Some(l) if row.starred_at > l => acc.last = Some(row.starred_at),
            None => acc.last = Some(row.starred_at),
            _ => {}
        }
    }

    let mut metrics = Vec::new();
    for (category, acc) in by_cat {
        let lifetime_months = match (acc.first, acc.last) {
            (Some(first), Some(last)) => {
                let days = (last - first).whole_days().max(0) as f64;
                (days / 30.44).max(1.0)
            }
            _ => 1.0,
        };
        let lifetime_velocity = acc.count as f64 / lifetime_months;
        let recent_velocity = acc.recent as f64 / RECENT_MONTHS;
        let badge = classify_badge(recent_velocity, lifetime_velocity, acc.recent);
        metrics.push(InterestMetric {
            category,
            repo_count: acc.count,
            first_starred: acc.first.and_then(|d| d.format(&Rfc3339).ok()),
            last_starred: acc.last.and_then(|d| d.format(&Rfc3339).ok()),
            recent_velocity,
            lifetime_velocity,
            badge,
        });
    }
    metrics.sort_by_key(|m| std::cmp::Reverse(m.repo_count));
    Ok(metrics)
}

pub fn classify_badge(recent_velocity: f64, lifetime_velocity: f64, recent_count: i64) -> String {
    if recent_count == 0 {
        return "dormant".into();
    }
    if lifetime_velocity > 0.0 && recent_velocity > RISING_RATIO * lifetime_velocity {
        return "rising".into();
    }
    "steady".into()
}

fn build_fun_facts(
    conn: &Connection,
    starred: &[StarredRow],
    offset: UtcOffset,
) -> AppResult<FunFacts> {
    if starred.is_empty() {
        return Ok(FunFacts {
            longest_streak_days: 0,
            biggest_day_count: 0,
            biggest_day_date: None,
            first_star_at: None,
            first_star_repo: None,
            oldest_repo_created_at: None,
            oldest_repo_name: None,
        });
    }

    let mut by_day: BTreeMap<Date, i64> = BTreeMap::new();
    for row in starred {
        let day = row.starred_at.to_offset(offset).date();
        *by_day.entry(day).or_insert(0) += 1;
    }

    let longest_streak_days = longest_streak(&by_day);
    let (biggest_day_date, biggest_day_count) = by_day
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)))
        .map(|(d, c)| {
            (
                Some(format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())),
                *c,
            )
        })
        .unwrap_or((None, 0));

    let first = &starred[0];
    let first_star_at = first.starred_at.format(&Rfc3339).ok();
    let first_star_repo = Some(first.full_name.clone());

    // Oldest repo by repo_created_at among starred (in range).
    let mut oldest_repo_created_at: Option<String> = None;
    let mut oldest_repo_name: Option<String> = None;
    for row in starred {
        let Some(created) = row.repo_created_at.as_deref() else {
            continue;
        };
        if created.is_empty() {
            continue;
        }
        match &oldest_repo_created_at {
            None => {
                oldest_repo_created_at = Some(created.to_string());
                oldest_repo_name = Some(row.full_name.clone());
            }
            Some(cur) if created < cur.as_str() => {
                oldest_repo_created_at = Some(created.to_string());
                oldest_repo_name = Some(row.full_name.clone());
            }
            _ => {}
        }
    }

    // If nothing in-memory had created_at, fall back to SQL over same ids.
    if oldest_repo_created_at.is_none() {
        let _ = conn; // already loaded from rows
    }

    Ok(FunFacts {
        longest_streak_days,
        biggest_day_count,
        biggest_day_date,
        first_star_at,
        first_star_repo,
        oldest_repo_created_at,
        oldest_repo_name,
    })
}

fn longest_streak(by_day: &BTreeMap<Date, i64>) -> i64 {
    let mut best = 0i64;
    let mut current = 0i64;
    let mut prev: Option<Date> = None;
    for day in by_day.keys() {
        match prev {
            Some(p) if *day == p + Duration::days(1) => current += 1,
            _ => current = 1,
        }
        best = best.max(current);
        prev = Some(*day);
    }
    best
}

fn export_markdown(conn: &Connection) -> AppResult<String> {
    let tree = load_export_tree(conn)?;
    let mut md = String::from("# Starboard Library\n\n");
    for cat in &tree.categories {
        md.push_str(&format!("## {}\n\n", cat.name));
        for repo in &cat.repos {
            push_md_repo(&mut md, repo);
        }
        for sub in &cat.subcategories {
            md.push_str(&format!("### {}\n\n", sub.name));
            for repo in &sub.repos {
                push_md_repo(&mut md, repo);
            }
        }
        if cat.repos.is_empty() && cat.subcategories.iter().all(|s| s.repos.is_empty()) {
            md.push_str("_No repos_\n\n");
        }
    }
    if !tree.uncategorized.is_empty() {
        md.push_str("## Uncategorized\n\n");
        for repo in &tree.uncategorized {
            push_md_repo(&mut md, repo);
        }
    }
    Ok(md)
}

fn push_md_repo(md: &mut String, repo: &ExportRepo) {
    let desc = repo
        .description
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" — {s}"))
        .unwrap_or_default();
    md.push_str(&format!(
        "- [{}]({}){}\n",
        repo.full_name, repo.html_url, desc
    ));
    md.push('\n');
}

fn export_json(conn: &Connection) -> AppResult<String> {
    let tree = load_export_tree(conn)?;
    serde_json::to_string_pretty(&tree)
        .map_err(|e| AppError::new("serialize_error", e.to_string()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportTree {
    exported_at: String,
    categories: Vec<ExportCategory>,
    uncategorized: Vec<ExportRepo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportCategory {
    name: String,
    repos: Vec<ExportRepo>,
    subcategories: Vec<ExportSubcategory>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportSubcategory {
    name: String,
    repos: Vec<ExportRepo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportRepo {
    id: i64,
    full_name: String,
    html_url: String,
    description: Option<String>,
    language: Option<String>,
    starred_at: String,
}

fn load_export_tree(conn: &Connection) -> AppResult<ExportTree> {
    // Active library only (exclude unstarred) for export.
    let mut cats_stmt = conn.prepare(
        "SELECT id, name, parent_id FROM categories
         ORDER BY parent_id IS NOT NULL, name COLLATE NOCASE",
    )?;
    let cat_rows: Vec<(i64, String, Option<i64>)> = cats_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut top_order: Vec<(i64, String)> = Vec::new();
    let mut subs: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for (id, name, parent_id) in &cat_rows {
        match parent_id {
            None => top_order.push((*id, name.clone())),
            Some(pid) => subs.entry(*pid).or_default().push((*id, name.clone())),
        }
    }

    let mut repos_stmt = conn.prepare(
        "SELECT r.id, r.full_name, r.html_url, r.description, r.language, r.starred_at,
                (
                  SELECT rc.category_id FROM repo_categories rc
                  WHERE rc.repo_id = r.id
                  ORDER BY CASE WHEN rc.source = 'manual' THEN 0 ELSE 1 END,
                           rc.confidence DESC
                  LIMIT 1
                ) AS category_id
         FROM repos r
         WHERE r.unstarred = 0
         ORDER BY r.full_name COLLATE NOCASE",
    )?;
    let repo_rows = repos_stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;

    let cat_parent: HashMap<i64, Option<i64>> = cat_rows
        .iter()
        .map(|(id, _, pid)| (*id, *pid))
        .collect();
    let cat_name: HashMap<i64, String> = cat_rows
        .iter()
        .map(|(id, name, _)| (*id, name.clone()))
        .collect();

    let mut top_direct: HashMap<i64, Vec<ExportRepo>> = HashMap::new();
    let mut sub_repos: HashMap<i64, Vec<ExportRepo>> = HashMap::new();
    let mut uncategorized: Vec<ExportRepo> = Vec::new();

    for row in repo_rows {
        let (id, full_name, html_url, description, language, starred_at, category_id) = row?;
        let repo = ExportRepo {
            id,
            full_name,
            html_url,
            description,
            language,
            starred_at,
        };
        let Some(cid) = category_id else {
            uncategorized.push(repo);
            continue;
        };
        match cat_parent.get(&cid).copied().flatten() {
            Some(_parent) => {
                sub_repos.entry(cid).or_default().push(repo);
            }
            None => {
                // Assigned directly to top-level (or unknown id → uncategorized)
                if cat_name.contains_key(&cid) {
                    top_direct.entry(cid).or_default().push(repo);
                } else {
                    uncategorized.push(repo);
                }
            }
        }
    }

    let exported_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".into());

    let categories = top_order
        .into_iter()
        .map(|(id, name)| {
            let subcategories = subs
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|(sid, sname)| ExportSubcategory {
                    name: sname,
                    repos: sub_repos.remove(&sid).unwrap_or_default(),
                })
                .collect();
            ExportCategory {
                name,
                repos: top_direct.remove(&id).unwrap_or_default(),
                subcategories,
            }
        })
        .collect();

    Ok(ExportTree {
        exported_at,
        categories,
        uncategorized,
    })
}

use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use rusqlite::params;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_insights_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn insert_repo(
        conn: &Connection,
        id: i64,
        name: &str,
        starred_at: &str,
        language: Option<&str>,
        unstarred: bool,
        created_at: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, language, starred_at,
                                repo_created_at, fetched_at, unstarred)
             VALUES (?1, ?2, 'owner', ?3, ?4, ?5, ?6, ?7, '2024-01-01T00:00:00Z', ?8)",
            params![
                id,
                format!("owner/{name}"),
                name,
                format!("https://github.com/owner/{name}"),
                language,
                starred_at,
                created_at,
                if unstarred { 1 } else { 0 },
            ],
        )
        .expect("insert repo");
    }

    fn seed_categories(conn: &Connection) {
        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES
             (1, 'AI/LLM', NULL),
             (2, 'Agents', 1),
             (3, 'DevTools', NULL),
             (4, 'Editors', 3),
             (5, 'Uncategorized', NULL)",
            [],
        )
        .expect("cats");
    }

    fn assign(conn: &Connection, repo_id: i64, category_id: i64, source: &str) {
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence)
             VALUES (?1, ?2, ?3, 0.9)",
            params![repo_id, category_id, source],
        )
        .expect("assign");
    }

    fn seed_rich(conn: &Connection) {
        seed_categories(conn);
        // Spread across quarters / days / hours (UTC). Include one unstarred.
        insert_repo(
            conn,
            1,
            "a",
            "2024-01-15T15:00:00Z",
            Some("Rust"),
            false,
            Some("2018-01-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            2,
            "b",
            "2024-01-15T16:00:00Z",
            Some("Rust"),
            false,
            Some("2019-06-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            3,
            "c",
            "2024-01-16T15:00:00Z",
            Some("Go"),
            false,
            Some("2020-01-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            4,
            "d",
            "2024-04-10T10:00:00Z",
            Some("Python"),
            false,
            Some("2015-01-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            5,
            "e",
            "2024-07-20T08:00:00Z",
            Some("TypeScript"),
            false,
            Some("2021-01-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            6,
            "f",
            "2024-07-21T08:00:00Z",
            Some("TypeScript"),
            true, // unstarred — still counts in insights
            Some("2022-01-01T00:00:00Z"),
        );
        insert_repo(
            conn,
            7,
            "g",
            "2024-10-05T20:00:00Z",
            Some("Rust"),
            false,
            Some("2010-05-05T00:00:00Z"),
        );
        insert_repo(
            conn,
            8,
            "h",
            "2025-01-02T12:00:00Z",
            Some("Zig"),
            false,
            Some("2023-01-01T00:00:00Z"),
        );

        assign(conn, 1, 2, "llm"); // AI/LLM → Agents
        assign(conn, 2, 1, "llm"); // AI/LLM top-level
        assign(conn, 3, 4, "manual"); // DevTools → Editors
        assign(conn, 4, 3, "llm"); // DevTools
        assign(conn, 5, 2, "llm");
        // 6, 7, 8 uncategorized
    }

    fn all_time_utc() -> InsightsRequest {
        InsightsRequest {
            range: Some(InsightsDateRange {
                preset: Some("all".into()),
                start: None,
                end: None,
            }),
            utc_offset_minutes: Some(0),
        }
    }

    #[test]
    fn timeline_spot_check_matches_sql() {
        let conn = test_conn();
        seed_rich(&conn);
        let dash = get_insights(&conn, all_time_utc()).expect("insights");

        let jan_week: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM repos
                 WHERE starred_at >= '2024-01-15T00:00:00Z'
                   AND starred_at < '2024-01-22T00:00:00Z'",
                [],
                |r| r.get(0),
            )
            .expect("sql");
        let week_bucket = dash
            .timeline
            .weekly
            .iter()
            .find(|b| b.period == "2024-W03")
            .expect("week bucket");
        assert_eq!(week_bucket.count, jan_week);
        assert_eq!(week_bucket.count, 3);

        let july: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM repos WHERE starred_at LIKE '2024-07%'",
                [],
                |r| r.get(0),
            )
            .expect("sql");
        let month = dash
            .timeline
            .monthly
            .iter()
            .find(|b| b.period == "2024-07")
            .expect("month");
        assert_eq!(month.count, july);
        assert_eq!(month.count, 2); // includes unstarred
    }

    #[test]
    fn heatmap_timezone_shifts_hour() {
        let conn = test_conn();
        // Wednesday 2024-01-17 01:00 UTC → Tuesday 17:00 in UTC-8
        insert_repo(
            &conn,
            1,
            "tz",
            "2024-01-17T01:00:00Z",
            Some("Rust"),
            false,
            None,
        );

        let utc = get_insights(
            &conn,
            InsightsRequest {
                range: None,
                utc_offset_minutes: Some(0),
            },
        )
        .expect("utc");
        let cell = utc
            .heatmap
            .iter()
            .find(|c| c.count > 0)
            .expect("utc cell");
        assert_eq!(cell.day_of_week, 2); // Wednesday
        assert_eq!(cell.hour, 1);

        let pst = get_insights(
            &conn,
            InsightsRequest {
                range: None,
                utc_offset_minutes: Some(-8 * 60),
            },
        )
        .expect("pst");
        let cell = pst
            .heatmap
            .iter()
            .find(|c| c.count > 0)
            .expect("pst cell");
        assert_eq!(cell.day_of_week, 1); // Tuesday
        assert_eq!(cell.hour, 17);
    }

    #[test]
    fn interest_drift_spot_check() {
        let conn = test_conn();
        seed_rich(&conn);
        let dash = get_insights(&conn, all_time_utc()).expect("insights");

        let q1_ai: i64 = dash
            .interest_drift
            .iter()
            .filter(|p| p.period == "2024-Q1" && p.name == "AI/LLM")
            .map(|p| p.count)
            .sum();
        // repos 1,2 in Q1 under AI/LLM
        assert_eq!(q1_ai, 2);

        let q1_total: i64 = dash
            .interest_drift
            .iter()
            .filter(|p| p.period == "2024-Q1")
            .map(|p| p.count)
            .sum();
        assert_eq!(q1_total, 3); // a,b,c
    }

    #[test]
    fn language_trend_top8_plus_other() {
        let conn = test_conn();
        seed_rich(&conn);
        // Add many unique languages so Zig becomes Other when we have >8? We have
        // Rust, Go, Python, TypeScript, Zig = 5 — all appear. Spot-check Rust Q1.
        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        let rust_q1 = dash
            .language_trend
            .iter()
            .find(|p| p.period == "2024-Q1" && p.name == "Rust")
            .expect("rust");
        assert_eq!(rust_q1.count, 2);
    }

    #[test]
    fn language_other_bucket() {
        let conn = test_conn();
        for i in 1..=10 {
            insert_repo(
                &conn,
                i,
                &format!("lang{i}"),
                "2024-06-01T12:00:00Z",
                Some(&format!("Lang{i}")),
                false,
                None,
            );
        }
        // 2 more of Lang1 so top languages are clear; Lang9+Lang10 → candidates for Other
        insert_repo(
            &conn,
            11,
            "extra1",
            "2024-06-02T12:00:00Z",
            Some("Lang1"),
            false,
            None,
        );
        insert_repo(
            &conn,
            12,
            "extra2",
            "2024-06-03T12:00:00Z",
            Some("Lang2"),
            false,
            None,
        );
        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        let names: HashSet<_> = dash
            .language_trend
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert!(names.contains("Other"));
    }

    #[test]
    fn badge_classification_boundaries() {
        assert_eq!(classify_badge(0.0, 1.0, 0), "dormant");
        assert_eq!(classify_badge(3.1, 2.0, 5), "rising"); // > 1.5 * 2 = 3
        assert_eq!(classify_badge(3.0, 2.0, 5), "steady"); // equal to 1.5x → steady
        assert_eq!(classify_badge(1.0, 2.0, 3), "steady");
    }

    #[test]
    fn interest_metrics_dormant_and_rising() {
        let conn = test_conn();
        seed_categories(&conn);
        // Old DevTools stars only — dormant relative to "now"
        insert_repo(
            &conn,
            1,
            "old1",
            "2020-01-01T00:00:00Z",
            Some("Rust"),
            false,
            None,
        );
        insert_repo(
            &conn,
            2,
            "old2",
            "2020-02-01T00:00:00Z",
            Some("Rust"),
            false,
            None,
        );
        assign(&conn, 1, 3, "llm");
        assign(&conn, 2, 3, "llm");

        // Sparse old AI history + recent burst → rising (recent > 1.5× lifetime).
        insert_repo(
            &conn,
            3,
            "ai_old",
            "2022-01-01T00:00:00Z",
            Some("Rust"),
            false,
            None,
        );
        assign(&conn, 3, 1, "llm");
        let recent = OffsetDateTime::now_utc() - Duration::days(20);
        for (i, days) in [0i64, 2, 4, 6, 8].into_iter().enumerate() {
            let id = 10 + i as i64;
            let at = (recent + Duration::days(days))
                .format(&Rfc3339)
                .expect("fmt");
            insert_repo(&conn, id, &format!("ai_new{i}"), &at, Some("Rust"), false, None);
            assign(&conn, id, 1, "llm");
        }

        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        let tools = dash
            .interest_metrics
            .iter()
            .find(|m| m.category == "DevTools")
            .expect("devtools");
        assert_eq!(tools.badge, "dormant");

        let ai = dash
            .interest_metrics
            .iter()
            .find(|m| m.category == "AI/LLM")
            .expect("ai");
        assert!(
            ai.recent_velocity > 1.5 * ai.lifetime_velocity,
            "expected rising: recent={} lifetime={}",
            ai.recent_velocity,
            ai.lifetime_velocity
        );
        assert_eq!(ai.badge, "rising");
    }

    #[test]
    fn date_range_applies_to_all_six_views() {
        let conn = test_conn();
        seed_rich(&conn);
        let filtered = get_insights(
            &conn,
            InsightsRequest {
                range: Some(InsightsDateRange {
                    preset: Some("custom".into()),
                    start: Some("2024-07-01T00:00:00Z".into()),
                    end: Some("2024-07-31T23:59:59Z".into()),
                }),
                utc_offset_minutes: Some(0),
            },
        )
        .expect("filtered");

        assert_eq!(filtered.meta.total_stars, 2);
        assert!(filtered
            .timeline
            .monthly
            .iter()
            .all(|b| b.period == "2024-07"));
        let heat_total: i64 = filtered.heatmap.iter().map(|c| c.count).sum();
        assert_eq!(heat_total, 2);
        let drift_total: i64 = filtered.interest_drift.iter().map(|p| p.count).sum();
        assert_eq!(drift_total, 2);
        let lang_total: i64 = filtered.language_trend.iter().map(|p| p.count).sum();
        assert_eq!(lang_total, 2);
        let metrics_total: i64 = filtered.interest_metrics.iter().map(|m| m.repo_count).sum();
        assert_eq!(metrics_total, 2);
        assert_eq!(filtered.fun_facts.biggest_day_count, 1);
        assert!(filtered
            .fun_facts
            .first_star_repo
            .as_deref()
            .is_some_and(|n| n == "owner/e" || n == "owner/f"));
    }

    #[test]
    fn short_history_empty_states() {
        let conn = test_conn();
        insert_repo(
            &conn,
            1,
            "solo",
            "2024-06-01T12:00:00Z",
            Some("Rust"),
            false,
            None,
        );
        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        assert!(dash.meta.short_history);
        assert_eq!(dash.meta.total_stars, 1);
        assert_eq!(dash.fun_facts.longest_streak_days, 1);
        assert!(dash.interest_metrics.iter().all(|m| m.lifetime_velocity.is_finite()));
        assert!(!dash.timeline.weekly.is_empty());
    }

    #[test]
    fn empty_library_no_crash() {
        let conn = test_conn();
        let dash = get_insights(&conn, all_time_utc()).expect("empty");
        assert_eq!(dash.meta.total_stars, 0);
        assert!(dash.meta.short_history);
        assert!(dash.heatmap.is_empty());
        assert_eq!(dash.fun_facts.longest_streak_days, 0);
        assert!(dash.fun_facts.first_star_repo.is_none());
    }

    #[test]
    fn unstarred_included_in_counts() {
        let conn = test_conn();
        insert_repo(
            &conn,
            1,
            "gone",
            "2024-03-01T00:00:00Z",
            Some("Rust"),
            true,
            None,
        );
        insert_repo(
            &conn,
            2,
            "kept",
            "2024-03-02T00:00:00Z",
            Some("Rust"),
            false,
            None,
        );
        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        assert_eq!(dash.meta.total_stars, 2);
    }

    #[test]
    fn fun_facts_streak_and_oldest() {
        let conn = test_conn();
        insert_repo(
            &conn,
            1,
            "a",
            "2024-01-01T12:00:00Z",
            None,
            false,
            Some("2010-01-01T00:00:00Z"),
        );
        insert_repo(
            &conn,
            2,
            "b",
            "2024-01-02T12:00:00Z",
            None,
            false,
            Some("2012-01-01T00:00:00Z"),
        );
        insert_repo(
            &conn,
            3,
            "c",
            "2024-01-03T12:00:00Z",
            None,
            false,
            Some("2011-01-01T00:00:00Z"),
        );
        insert_repo(
            &conn,
            4,
            "d",
            "2024-01-10T12:00:00Z",
            None,
            false,
            Some("2008-06-15T00:00:00Z"),
        );
        // binge day
        insert_repo(
            &conn,
            5,
            "e",
            "2024-01-10T13:00:00Z",
            None,
            false,
            Some("2015-01-01T00:00:00Z"),
        );
        insert_repo(
            &conn,
            6,
            "f",
            "2024-01-10T14:00:00Z",
            None,
            false,
            Some("2016-01-01T00:00:00Z"),
        );

        let dash = get_insights(&conn, all_time_utc()).expect("insights");
        assert_eq!(dash.fun_facts.longest_streak_days, 3);
        assert_eq!(dash.fun_facts.biggest_day_count, 3);
        assert_eq!(
            dash.fun_facts.biggest_day_date.as_deref(),
            Some("2024-01-10")
        );
        assert_eq!(dash.fun_facts.first_star_repo.as_deref(), Some("owner/a"));
        assert_eq!(
            dash.fun_facts.oldest_repo_name.as_deref(),
            Some("owner/d")
        );
    }

    #[test]
    fn export_matches_categories() {
        let conn = test_conn();
        seed_rich(&conn);
        let export = export_library(&conn).expect("export");
        assert!(export.markdown.contains("## AI/LLM"));
        assert!(export.markdown.contains("### Agents"));
        assert!(export.markdown.contains("[owner/a]("));
        assert!(export.markdown.contains("## DevTools"));
        // Unstarred repo f must not appear in library export
        assert!(!export.markdown.contains("owner/f"));
        // Uncategorized active repos present
        assert!(export.markdown.contains("## Uncategorized"));
        assert!(export.markdown.contains("owner/g"));

        let parsed: serde_json::Value = serde_json::from_str(&export.json).expect("json");
        assert!(parsed["categories"].is_array());
        let cats = parsed["categories"].as_array().unwrap();
        let ai = cats
            .iter()
            .find(|c| c["name"] == "AI/LLM")
            .expect("ai cat");
        assert!(ai["subcategories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "Agents"));
    }

    #[test]
    fn write_export_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "starboard_export_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("out.md");
        write_export_file(path.to_str().unwrap(), "# hi\n").expect("write");
        let got = std::fs::read_to_string(&path).expect("read");
        assert_eq!(got, "# hi\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn drilldown_subcategories() {
        let conn = test_conn();
        seed_rich(&conn);
        let points = interest_drift_drilldown(
            &conn,
            Some(InsightsDateRange {
                preset: Some("all".into()),
                ..Default::default()
            }),
            "AI/LLM",
            Some(0),
        )
        .expect("drill");
        let agents: i64 = points
            .iter()
            .filter(|p| p.name == "Agents")
            .map(|p| p.count)
            .sum();
        assert!(agents >= 2);
    }

    #[test]
    fn custom_range_rejects_bad_preset() {
        let conn = test_conn();
        let err = get_insights(
            &conn,
            InsightsRequest {
                range: Some(InsightsDateRange {
                    preset: Some("nope".into()),
                    start: None,
                    end: None,
                }),
                utc_offset_minutes: Some(0),
            },
        )
        .expect_err("bad preset");
        assert_eq!(err.code, "validation_error");
    }
}
