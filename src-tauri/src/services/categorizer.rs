use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{
    CategorizeProgress, TaxonomyDraft, TaxonomyEdit, TaxonomyNodeEdit,
};
use crate::services::github::truncate_utf8;
use crate::services::ollama::OllamaClient;
use crate::services::settings;
use crate::services::store::DbState;

/// Repos per assignment request to Ollama (HLD §5).
const ASSIGNMENT_BATCH_SIZE: i64 = 25;
/// Full library vs. stratified-sample threshold for taxonomy generation (HLD §5).
const TAXONOMY_SAMPLE_THRESHOLD: i64 = 500;
const TAXONOMY_SAMPLE_TARGET: usize = 400;
/// README excerpt length included per repo in assignment prompts (HLD §5).
const README_PROMPT_CHARS: usize = 400;

const TAXONOMY_SYSTEM_PROMPT: &str = "You are organizing a developer's GitHub starred \
repositories into a two-level taxonomy. Propose 8 to 14 top-level categories, each with \
2 to 6 subcategories, that together cover the given repositories well. Use short, clear \
names (e.g. \"AI/LLM\", \"Web Frameworks\"). Respond with JSON only, matching the schema.";

const ASSIGNMENT_SYSTEM_PROMPT: &str = "You are assigning GitHub repositories to an \
existing taxonomy. You MUST use category and subcategory names exactly as given — never \
invent new ones. If nothing fits well, use the closest existing top-level category. \
Respond with JSON only, matching the schema.";

pub struct CategorizeState {
    pub running: AtomicBool,
    pub last_error: Mutex<Option<String>>,
}

impl Default for CategorizeState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }
}

/// (id, name, parent_id) — a flattened view of the `categories` table.
pub type TaxonomyRow = (i64, String, Option<i64>);

#[derive(Debug, Clone)]
pub struct TaxonomySampleRepo {
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AssignmentRepo {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub topics: Vec<String>,
    pub readme_excerpt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssignmentResponse {
    assignments: Vec<AssignmentItem>,
}

#[derive(Debug, Deserialize)]
struct AssignmentItem {
    repo_id: i64,
    category: String,
    #[serde(default)]
    subcategory: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
}

// ---------------------------------------------------------------------------
// Pure taxonomy mapping (unit-tested below)
// ---------------------------------------------------------------------------

pub fn normalize_name(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Map an LLM-proposed (category, subcategory) pair onto the committed taxonomy.
/// Matches the subcategory under the matching parent (case-insensitive); falls back to
/// the top-level category; falls back to `uncategorized_id` if nothing matches
/// (CLAUDE.md rule 8 — never create new taxonomy entries from the LLM).
pub fn map_assignment(
    category: &str,
    subcategory: Option<&str>,
    taxonomy: &[TaxonomyRow],
    uncategorized_id: i64,
) -> i64 {
    let norm_category = normalize_name(category);

    let top_level = taxonomy
        .iter()
        .find(|(_, name, parent_id)| parent_id.is_none() && normalize_name(name) == norm_category);

    let Some((top_id, _, _)) = top_level else {
        return uncategorized_id;
    };

    if let Some(sub) = subcategory.map(str::trim).filter(|s| !s.is_empty()) {
        let norm_sub = normalize_name(sub);
        if let Some((sub_id, _, _)) = taxonomy
            .iter()
            .find(|(_, name, parent_id)| *parent_id == Some(*top_id) && normalize_name(name) == norm_sub)
        {
            return *sub_id;
        }
    }

    *top_id
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

pub fn ensure_uncategorized(conn: &Connection) -> AppResult<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE parent_id IS NULL AND LOWER(name) = 'uncategorized'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO categories (name, parent_id) VALUES ('Uncategorized', NULL)",
        [],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_taxonomy_flat(conn: &Connection) -> AppResult<Vec<TaxonomyRow>> {
    let mut stmt = conn.prepare("SELECT id, name, parent_id FROM categories")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Load the committed taxonomy as an editable tree (ids preserved for in-place updates).
pub fn load_taxonomy_edit(conn: &Connection) -> AppResult<TaxonomyEdit> {
    let flat = list_taxonomy_flat(conn)?;
    let mut tops: Vec<TaxonomyNodeEdit> = Vec::new();
    let mut children: Vec<(i64, TaxonomyNodeEdit)> = Vec::new();

    for (id, name, parent_id) in flat {
        let node = TaxonomyNodeEdit {
            id: Some(id),
            name,
            subcategories: Vec::new(),
        };
        match parent_id {
            None => tops.push(node),
            Some(pid) => children.push((pid, node)),
        }
    }

    for (pid, child) in children {
        if let Some(parent) = tops.iter_mut().find(|n| n.id == Some(pid)) {
            parent.subcategories.push(child);
        } else {
            // Orphan subcategory — surface as top-level so it isn't lost in the editor.
            tops.push(child);
        }
    }

    Ok(TaxonomyEdit { categories: tops })
}

/// In-place taxonomy update: rename/add/remove while keeping stable ids so existing
/// `repo_categories` rows survive renames. Removed category ids drop their assignments only.
pub fn update_taxonomy(conn: &Connection, edit: &TaxonomyEdit) -> AppResult<()> {
    let existing = list_taxonomy_flat(conn)?;
    if existing.is_empty() {
        return Err(AppError::new(
            "taxonomy_missing",
            "no taxonomy to edit — commit a draft first",
        ));
    }

    let mut keep_ids: Vec<i64> = Vec::new();

    for category in &edit.categories {
        let name = category.name.trim();
        if name.is_empty() {
            continue;
        }
        let parent_id = upsert_category_node(conn, category.id, name, None)?;
        keep_ids.push(parent_id);
        for sub in &category.subcategories {
            let sub_name = sub.name.trim();
            if sub_name.is_empty() {
                continue;
            }
            let sub_id = upsert_category_node(conn, sub.id, sub_name, Some(parent_id))?;
            keep_ids.push(sub_id);
        }
    }

    if keep_ids.is_empty() {
        return Err(AppError::new(
            "taxonomy_empty",
            "taxonomy must contain at least one category",
        ));
    }

    let keep_set: std::collections::HashSet<i64> = keep_ids.into_iter().collect();
    let mut to_delete: Vec<(i64, Option<i64>)> = existing
        .iter()
        .filter(|(id, _, _)| !keep_set.contains(id))
        .map(|(id, _, parent_id)| (*id, *parent_id))
        .collect();
    // Children first so parent FK references don't block deletes.
    to_delete.sort_by_key(|(_, parent_id)| parent_id.is_none());

    for (id, _) in &to_delete {
        conn.execute("DELETE FROM repo_categories WHERE category_id = ?1", params![id])?;
    }
    for (id, _) in &to_delete {
        conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
    }

    Ok(())
}

fn upsert_category_node(
    conn: &Connection,
    id: Option<i64>,
    name: &str,
    parent_id: Option<i64>,
) -> AppResult<i64> {
    if let Some(existing_id) = id {
        let found: Option<i64> = conn
            .query_row(
                "SELECT id FROM categories WHERE id = ?1",
                params![existing_id],
                |row| row.get(0),
            )
            .optional()?;
        if found.is_some() {
            conn.execute(
                "UPDATE categories SET name = ?1, parent_id = ?2 WHERE id = ?3",
                params![name, parent_id, existing_id],
            )?;
            return Ok(existing_id);
        }
    }
    get_or_create_category(conn, name, parent_id)
}

/// Thinner-cut replace strategy: a taxonomy can only be (re)committed when `categories`
/// is empty, unless `force` is set. Forcing wipes ALL repo_categories (including manual
/// overrides) since remapping old assignments onto a brand-new tree isn't well-defined.
/// Callers must warn the user before passing `force = true`.
pub fn commit_taxonomy(conn: &Connection, draft: &TaxonomyDraft, force: bool) -> AppResult<()> {
    let existing_count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
    if existing_count > 0 && !force {
        return Err(AppError::new(
            "taxonomy_exists",
            "a taxonomy already exists — commit with force=true to replace it (this clears all category assignments, including manual ones)",
        ));
    }
    if existing_count > 0 {
        conn.execute("DELETE FROM repo_categories", [])?;
        conn.execute("DELETE FROM categories", [])?;
    }

    for category in &draft.categories {
        let name = category.name.trim();
        if name.is_empty() {
            continue;
        }
        let parent_id = get_or_create_category(conn, name, None)?;
        for sub in &category.subcategories {
            let sub_name = sub.trim();
            if sub_name.is_empty() {
                continue;
            }
            get_or_create_category(conn, sub_name, Some(parent_id))?;
        }
    }

    Ok(())
}

fn get_or_create_category(conn: &Connection, name: &str, parent_id: Option<i64>) -> AppResult<i64> {
    let existing: Option<i64> = match parent_id {
        Some(pid) => conn
            .query_row(
                "SELECT id FROM categories WHERE parent_id = ?1 AND LOWER(name) = LOWER(?2)",
                params![pid, name],
                |row| row.get(0),
            )
            .optional()?,
        None => conn
            .query_row(
                "SELECT id FROM categories WHERE parent_id IS NULL AND LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get(0),
            )
            .optional()?,
    };
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO categories (name, parent_id) VALUES (?1, ?2)",
        params![name, parent_id],
    )?;
    Ok(conn.last_insert_rowid())
}

fn parse_topics_json(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn map_taxonomy_sample_row(row: &Row<'_>) -> rusqlite::Result<TaxonomySampleRepo> {
    let topics_raw: Option<String> = row.get(3)?;
    Ok(TaxonomySampleRepo {
        full_name: row.get(0)?,
        description: row.get(1)?,
        language: row.get(2)?,
        topics: parse_topics_json(topics_raw),
    })
}

fn map_assignment_repo_row(row: &Row<'_>) -> rusqlite::Result<AssignmentRepo> {
    let topics_raw: Option<String> = row.get(4)?;
    let readme_raw: Option<String> = row.get(5)?;
    Ok(AssignmentRepo {
        id: row.get(0)?,
        full_name: row.get(1)?,
        description: row.get(2)?,
        language: row.get(3)?,
        topics: parse_topics_json(topics_raw),
        readme_excerpt: readme_raw
            .filter(|s| !s.is_empty())
            .map(|s| truncate_utf8(&s, README_PROMPT_CHARS)),
    })
}

fn load_all_taxonomy_sample(conn: &Connection) -> AppResult<Vec<TaxonomySampleRepo>> {
    let mut stmt = conn.prepare(
        "SELECT full_name, description, language, topics FROM repos WHERE unstarred = 0 ORDER BY id",
    )?;
    let rows = stmt.query_map([], map_taxonomy_sample_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Groups repos by language and round-robins across groups so the sample stays
/// representative even when one language dominates the library.
fn stratified_sample(all: Vec<TaxonomySampleRepo>, target: usize) -> Vec<TaxonomySampleRepo> {
    let mut buckets: std::collections::BTreeMap<String, Vec<TaxonomySampleRepo>> =
        std::collections::BTreeMap::new();
    for repo in all {
        let key = repo.language.clone().unwrap_or_else(|| "Unknown".to_string());
        buckets.entry(key).or_default().push(repo);
    }

    let mut sampled = Vec::with_capacity(target);
    let mut idx = 0usize;
    loop {
        if sampled.len() >= target {
            break;
        }
        let mut any_left = false;
        for bucket in buckets.values() {
            if sampled.len() >= target {
                break;
            }
            if let Some(repo) = bucket.get(idx) {
                sampled.push(repo.clone());
                any_left = true;
            } else if idx < bucket.len() {
                any_left = true;
            }
        }
        if !any_left {
            break;
        }
        idx += 1;
    }
    sampled
}

pub fn sample_repos_for_taxonomy(conn: &Connection) -> AppResult<Vec<TaxonomySampleRepo>> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
        [],
        |row| row.get(0),
    )?;

    let all = load_all_taxonomy_sample(conn)?;
    if total < TAXONOMY_SAMPLE_THRESHOLD {
        Ok(all)
    } else {
        Ok(stratified_sample(all, TAXONOMY_SAMPLE_TARGET))
    }
}

pub fn uncategorized_repos(conn: &Connection, limit: i64) -> AppResult<Vec<AssignmentRepo>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.full_name, r.description, r.language, r.topics, r.readme_excerpt
         FROM repos r
         WHERE r.unstarred = 0
           AND NOT EXISTS (SELECT 1 FROM repo_categories rc WHERE rc.repo_id = r.id)
         ORDER BY r.id
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], map_assignment_repo_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn count_uncategorized(conn: &Connection) -> AppResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos r
         WHERE r.unstarred = 0
           AND NOT EXISTS (SELECT 1 FROM repo_categories rc WHERE rc.repo_id = r.id)",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn has_manual_category(conn: &Connection, repo_id: i64) -> AppResult<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM repo_categories WHERE repo_id = ?1 AND source = 'manual')",
        [repo_id],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn load_assignment_repo(conn: &Connection, repo_id: i64) -> AppResult<AssignmentRepo> {
    conn.query_row(
        "SELECT id, full_name, description, language, topics, readme_excerpt
         FROM repos WHERE id = ?1",
        [repo_id],
        map_assignment_repo_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::new("not_found", format!("repo {repo_id} not found"))
        }
        other => AppError::from(other),
    })
}

/// One category per repo (locked decision): always replaces any prior assignment.
pub fn set_manual_category(conn: &Connection, repo_id: i64, category_id: i64) -> AppResult<()> {
    let category_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)",
        [category_id],
        |row| row.get(0),
    )?;
    if !category_exists {
        return Err(AppError::new(
            "not_found",
            format!("category {category_id} not found"),
        ));
    }
    let repo_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM repos WHERE id = ?1)",
        [repo_id],
        |row| row.get(0),
    )?;
    if !repo_exists {
        return Err(AppError::new("not_found", format!("repo {repo_id} not found")));
    }

    conn.execute("DELETE FROM repo_categories WHERE repo_id = ?1", [repo_id])?;
    conn.execute(
        "INSERT INTO repo_categories (repo_id, category_id, source, confidence) VALUES (?1, ?2, 'manual', 1.0)",
        params![repo_id, category_id],
    )?;
    Ok(())
}

/// Writes LLM assignments for exactly `batch_ids`. Repos that the model didn't return an
/// assignment for still get a row (Uncategorized) so the batch always makes forward
/// progress and the "no description/README" edge case never stalls the queue.
fn apply_assignments(
    conn: &Connection,
    assignments: &[AssignmentItem],
    taxonomy: &[TaxonomyRow],
    uncategorized_id: i64,
    batch_ids: &[i64],
) -> AppResult<()> {
    let mut by_id: HashMap<i64, &AssignmentItem> = HashMap::new();
    for item in assignments {
        by_id.insert(item.repo_id, item);
    }

    for &repo_id in batch_ids {
        let item = by_id.get(&repo_id);
        let category_id = match item {
            Some(item) => map_assignment(&item.category, item.subcategory.as_deref(), taxonomy, uncategorized_id),
            None => uncategorized_id,
        };
        let confidence = item.and_then(|i| i.confidence);

        conn.execute("DELETE FROM repo_categories WHERE repo_id = ?1", [repo_id])?;
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence) VALUES (?1, ?2, 'llm', ?3)",
            params![repo_id, category_id, confidence],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Prompts + schemas
// ---------------------------------------------------------------------------

fn build_taxonomy_prompt(sample: &[TaxonomySampleRepo]) -> String {
    let mut out = String::from("Repositories:\n");
    for repo in sample {
        let desc = repo.description.as_deref().unwrap_or("");
        let lang = repo.language.as_deref().unwrap_or("unknown");
        let topics = repo.topics.join(", ");
        out.push_str(&format!(
            "- {} | lang: {} | topics: {} | desc: {}\n",
            repo.full_name, lang, topics, desc
        ));
    }
    out
}

fn build_assignment_prompt(taxonomy: &[TaxonomyRow], batch: &[AssignmentRepo]) -> String {
    let mut out = String::from("Taxonomy (use these names exactly, do not invent new ones):\n");
    for (id, name, parent_id) in taxonomy {
        if parent_id.is_some() {
            continue;
        }
        out.push_str(&format!("- {name}\n"));
        for (_, sub_name, sub_parent) in taxonomy {
            if *sub_parent == Some(*id) {
                out.push_str(&format!("  - {sub_name}\n"));
            }
        }
    }

    out.push_str("\nRepositories to categorize:\n");
    for repo in batch {
        let desc = repo.description.as_deref().unwrap_or("");
        let lang = repo.language.as_deref().unwrap_or("unknown");
        let topics = repo.topics.join(", ");
        let readme = repo.readme_excerpt.as_deref().unwrap_or("");
        out.push_str(&format!(
            "- repo_id: {} | {} | lang: {} | topics: {} | desc: {} | readme: {}\n",
            repo.id, repo.full_name, lang, topics, desc, readme
        ));
    }
    out
}

fn taxonomy_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "categories": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "subcategories": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["name", "subcategories"]
                }
            }
        },
        "required": ["categories"]
    })
}

fn assignment_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "assignments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "repo_id": { "type": "integer" },
                        "category": { "type": "string" },
                        "subcategory": { "type": "string" },
                        "confidence": { "type": "number" }
                    },
                    "required": ["repo_id", "category"]
                }
            }
        },
        "required": ["assignments"]
    })
}

// ---------------------------------------------------------------------------
// Pipeline (async, DB + Ollama)
// ---------------------------------------------------------------------------

pub async fn generate_taxonomy(app: &AppHandle) -> AppResult<TaxonomyDraft> {
    let settings = with_db(app, settings::get_settings)?;
    let client = OllamaClient::from_settings(&settings)?;

    let status = client.health_check().await?;
    if !status.available {
        return Err(AppError::ollama(format!("Ollama is offline: {}", status.message)));
    }

    let sample = with_db(app, sample_repos_for_taxonomy)?;
    if sample.is_empty() {
        return Err(AppError::ollama("no repos available to build a taxonomy from"));
    }

    let user_prompt = build_taxonomy_prompt(&sample);
    client
        .chat_json::<TaxonomyDraft>(TAXONOMY_SYSTEM_PROMPT, &user_prompt, taxonomy_schema())
        .await
}

async fn assign_batch(
    client: &OllamaClient,
    taxonomy: &[TaxonomyRow],
    batch: &[AssignmentRepo],
) -> AppResult<Vec<AssignmentItem>> {
    let user_prompt = build_assignment_prompt(taxonomy, batch);
    let response: AssignmentResponse = client
        .chat_json(ASSIGNMENT_SYSTEM_PROMPT, &user_prompt, assignment_schema())
        .await?;
    Ok(response.assignments)
}

/// Runs the full assignment pass over all uncategorized repos, in batches of 25,
/// emitting `categorize://progress` events. Resumable: "uncategorized" always means
/// "no repo_categories row", so re-running after a partial failure picks up where it
/// left off without re-processing manually or already-LLM-categorized repos.
pub async fn run_assignment(app: AppHandle) -> AppResult<()> {
    let state = app.state::<CategorizeState>();
    if state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::ollama("categorization is already running"));
    }
    if let Ok(mut last_error) = state.last_error.lock() {
        *last_error = None;
    }

    let result = run_assignment_inner(&app).await;

    if let Err(e) = &result {
        if let Ok(mut last_error) = state.last_error.lock() {
            *last_error = Some(e.message.clone());
        }
    }
    state.running.store(false, Ordering::SeqCst);
    result
}

async fn run_assignment_inner(app: &AppHandle) -> AppResult<()> {
    let settings = with_db(app, settings::get_settings)?;
    let client = OllamaClient::from_settings(&settings)?;

    let status = client.health_check().await?;
    if !status.available {
        return Err(AppError::ollama(format!("Ollama is offline: {}", status.message)));
    }

    let taxonomy = with_db(app, list_taxonomy_flat)?;
    if taxonomy.is_empty() {
        return Err(AppError::ollama("commit a taxonomy before running assignment"));
    }
    let uncategorized_id = with_db(app, ensure_uncategorized)?;

    let total = with_db(app, count_uncategorized)?.max(0) as u32;
    let mut processed = 0u32;
    emit_progress(app, progress("assign", 0, total, "Starting categorization…", None));

    loop {
        let batch = with_db(app, |conn| uncategorized_repos(conn, ASSIGNMENT_BATCH_SIZE))?;
        if batch.is_empty() {
            break;
        }
        let batch_ids: Vec<i64> = batch.iter().map(|r| r.id).collect();

        let assignments = assign_batch(&client, &taxonomy, &batch).await?;

        with_db(app, |conn| {
            apply_assignments(conn, &assignments, &taxonomy, uncategorized_id, &batch_ids)
        })?;

        processed += batch_ids.len() as u32;
        let display_total = total.max(processed);
        emit_progress(
            app,
            progress(
                "assign",
                processed,
                display_total,
                format!("Categorized {processed}/{display_total}"),
                None,
            ),
        );
    }

    emit_progress(
        app,
        progress("assign", processed, processed, "Categorization complete", None),
    );
    Ok(())
}

/// Single-repo re-categorization. No-ops (returns an error) for manual overrides —
/// CLAUDE.md rule 7 forbids overwriting them, even on an explicit user action here.
pub async fn recategorize_single_repo(app: &AppHandle, repo_id: i64) -> AppResult<()> {
    if with_db(app, |conn| has_manual_category(conn, repo_id))? {
        return Err(AppError::new(
            "manual_override",
            "this repo has a manual category override and will not be re-categorized",
        ));
    }

    let settings = with_db(app, settings::get_settings)?;
    let client = OllamaClient::from_settings(&settings)?;

    let status = client.health_check().await?;
    if !status.available {
        return Err(AppError::ollama(format!("Ollama is offline: {}", status.message)));
    }

    let taxonomy = with_db(app, list_taxonomy_flat)?;
    if taxonomy.is_empty() {
        return Err(AppError::ollama("commit a taxonomy before categorizing"));
    }
    let uncategorized_id = with_db(app, ensure_uncategorized)?;
    let repo = with_db(app, |conn| load_assignment_repo(conn, repo_id))?;

    let assignments = assign_batch(&client, &taxonomy, std::slice::from_ref(&repo)).await?;
    with_db(app, |conn| {
        apply_assignments(conn, &assignments, &taxonomy, uncategorized_id, &[repo_id])
    })
}

fn progress(
    kind: impl Into<String>,
    current: u32,
    total: u32,
    message: impl Into<String>,
    error: Option<String>,
) -> CategorizeProgress {
    CategorizeProgress {
        kind: kind.into(),
        current,
        total,
        message: message.into(),
        error,
    }
}

fn emit_progress(app: &AppHandle, progress: CategorizeProgress) {
    let _ = app.emit("categorize://progress", progress);
}

fn with_db<T, F>(app: &AppHandle, f: F) -> AppResult<T>
where
    F: FnOnce(&Connection) -> AppResult<T>,
{
    let state = app.state::<DbState>();
    let conn = state
        .0
        .lock()
        .map_err(|_| AppError::db("database lock poisoned"))?;
    f(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_categorizer_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn taxonomy() -> Vec<TaxonomyRow> {
        vec![
            (1, "AI/LLM".to_string(), None),
            (2, "Agent Frameworks".to_string(), Some(1)),
            (3, "Web Frameworks".to_string(), None),
            (4, "Backend".to_string(), Some(3)),
            (99, "Uncategorized".to_string(), None),
        ]
    }

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(normalize_name("  AI/LLM  "), "ai/llm");
    }

    #[test]
    fn maps_exact_subcategory_case_insensitive() {
        let id = map_assignment("ai/llm", Some("AGENT FRAMEWORKS"), &taxonomy(), 99);
        assert_eq!(id, 2);
    }

    #[test]
    fn maps_to_top_level_when_subcategory_unknown() {
        let id = map_assignment("AI/LLM", Some("Nonexistent Sub"), &taxonomy(), 99);
        assert_eq!(id, 1);
    }

    #[test]
    fn maps_to_top_level_when_subcategory_missing() {
        let id = map_assignment("Web Frameworks", None, &taxonomy(), 99);
        assert_eq!(id, 3);
    }

    #[test]
    fn falls_back_to_uncategorized_for_off_taxonomy_category() {
        let id = map_assignment("Quantum Computing", Some("Qubits"), &taxonomy(), 99);
        assert_eq!(id, 99);
    }

    #[test]
    fn subcategory_under_wrong_parent_falls_back_to_top_level() {
        // "Backend" exists but under "Web Frameworks", not "AI/LLM" — must not cross-match.
        let id = map_assignment("AI/LLM", Some("Backend"), &taxonomy(), 99);
        assert_eq!(id, 1);
    }

    #[test]
    fn ensure_uncategorized_is_idempotent() {
        let conn = test_conn();
        let first = ensure_uncategorized(&conn).expect("ensure");
        let second = ensure_uncategorized(&conn).expect("ensure again");
        assert_eq!(first, second);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE LOWER(name) = 'uncategorized'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn commit_taxonomy_rejects_recommit_without_force() {
        let conn = test_conn();
        let draft = TaxonomyDraft {
            categories: vec![crate::models::TaxonomyCategoryDraft {
                name: "AI/LLM".into(),
                subcategories: vec!["Agent Frameworks".into()],
            }],
        };
        commit_taxonomy(&conn, &draft, false).expect("first commit");
        let err = commit_taxonomy(&conn, &draft, false).unwrap_err();
        assert_eq!(err.code, "taxonomy_exists");

        commit_taxonomy(&conn, &draft, true).expect("forced recommit");
        let flat = list_taxonomy_flat(&conn).expect("flat");
        assert_eq!(flat.len(), 2);
    }

    #[test]
    fn commit_taxonomy_force_clears_all_repo_categories_including_manual() {
        let conn = test_conn();
        let draft = TaxonomyDraft {
            categories: vec![crate::models::TaxonomyCategoryDraft {
                name: "AI/LLM".into(),
                subcategories: vec![],
            }],
        };
        commit_taxonomy(&conn, &draft, false).expect("commit");
        let cat_id: i64 = conn
            .query_row("SELECT id FROM categories LIMIT 1", [], |r| r.get(0))
            .expect("cat id");

        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
             VALUES (1, 'o/r', 'o', 'r', 'https://x', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed repo");
        set_manual_category(&conn, 1, cat_id).expect("manual assign");

        commit_taxonomy(&conn, &draft, true).expect("forced recommit");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM repo_categories", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn set_manual_category_replaces_prior_assignment() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES (1, 'A', NULL), (2, 'B', NULL)",
            [],
        )
        .expect("seed categories");
        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
             VALUES (1, 'o/r', 'o', 'r', 'https://x', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed repo");

        set_manual_category(&conn, 1, 1).expect("assign A");
        set_manual_category(&conn, 1, 2).expect("assign B");

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM repo_categories WHERE repo_id = 1", [], |r| {
                r.get(0)
            })
            .expect("count");
        assert_eq!(rows, 1);
        let (category_id, source): (i64, String) = conn
            .query_row(
                "SELECT category_id, source FROM repo_categories WHERE repo_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("row");
        assert_eq!(category_id, 2);
        assert_eq!(source, "manual");
    }

    #[test]
    fn apply_assignments_falls_back_to_uncategorized_for_missing_repo() {
        let conn = test_conn();
        let taxonomy = taxonomy();
        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES (99, 'Uncategorized', NULL)",
            [],
        )
        .expect("seed uncategorized");
        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
             VALUES (5, 'o/r', 'o', 'r', 'https://x', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("seed repo");

        // Model returned nothing for repo 5 (e.g. it had no description/README).
        apply_assignments(&conn, &[], &taxonomy, 99, &[5]).expect("apply");

        let category_id: i64 = conn
            .query_row(
                "SELECT category_id FROM repo_categories WHERE repo_id = 5",
                [],
                |r| r.get(0),
            )
            .expect("row");
        assert_eq!(category_id, 99);
    }

    #[test]
    fn stratified_sample_caps_at_target_and_covers_languages() {
        let mut repos = Vec::new();
        for i in 0..300 {
            repos.push(TaxonomySampleRepo {
                full_name: format!("owner/rust-{i}"),
                description: None,
                language: Some("Rust".to_string()),
                topics: vec![],
            });
        }
        for i in 0..300 {
            repos.push(TaxonomySampleRepo {
                full_name: format!("owner/go-{i}"),
                description: None,
                language: Some("Go".to_string()),
                topics: vec![],
            });
        }

        let sampled = stratified_sample(repos, 400);
        assert_eq!(sampled.len(), 400);
        let rust_count = sampled.iter().filter(|r| r.language.as_deref() == Some("Rust")).count();
        let go_count = sampled.iter().filter(|r| r.language.as_deref() == Some("Go")).count();
        assert_eq!(rust_count, 200);
        assert_eq!(go_count, 200);
    }

    #[test]
    fn load_and_update_taxonomy_preserves_ids_and_assignments() {
        let conn = test_conn();
        let draft = TaxonomyDraft {
            categories: vec![crate::models::TaxonomyCategoryDraft {
                name: "AI/LLM".into(),
                subcategories: vec!["Agents".into()],
            }],
        };
        commit_taxonomy(&conn, &draft, false).expect("commit");
        let edit = load_taxonomy_edit(&conn).expect("load");
        assert_eq!(edit.categories.len(), 1);
        let top_id = edit.categories[0].id.expect("top id");
        let sub_id = edit.categories[0].subcategories[0].id.expect("sub id");

        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
             VALUES (1, 'o/r', 'o', 'r', 'https://github.com/o/r', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("repo");
        set_manual_category(&conn, 1, sub_id).expect("assign");

        let mut next = edit;
        next.categories[0].name = "Artificial Intelligence".into();
        next.categories[0].subcategories[0].name = "Agent Frameworks".into();
        next.categories[0]
            .subcategories
            .push(crate::models::TaxonomyNodeEdit {
                id: None,
                name: "RAG".into(),
                subcategories: vec![],
            });
        update_taxonomy(&conn, &next).expect("update");

        let reloaded = load_taxonomy_edit(&conn).expect("reload");
        assert_eq!(reloaded.categories[0].id, Some(top_id));
        assert_eq!(reloaded.categories[0].name, "Artificial Intelligence");
        assert_eq!(reloaded.categories[0].subcategories[0].id, Some(sub_id));
        assert_eq!(
            reloaded.categories[0].subcategories[0].name,
            "Agent Frameworks"
        );
        assert!(
            reloaded.categories[0]
                .subcategories
                .iter()
                .any(|s| s.name == "RAG")
        );

        let assigned: i64 = conn
            .query_row(
                "SELECT category_id FROM repo_categories WHERE repo_id = 1",
                [],
                |r| r.get(0),
            )
            .expect("still assigned");
        assert_eq!(assigned, sub_id);
    }

    #[test]
    fn update_taxonomy_deletes_removed_nodes_and_their_assignments() {
        let conn = test_conn();
        let draft = TaxonomyDraft {
            categories: vec![crate::models::TaxonomyCategoryDraft {
                name: "Tools".into(),
                subcategories: vec!["CLI".into(), "Editors".into()],
            }],
        };
        commit_taxonomy(&conn, &draft, false).expect("commit");
        let edit = load_taxonomy_edit(&conn).expect("load");
        let cli_id = edit.categories[0].subcategories[0].id.expect("cli");
        let editors_id = edit.categories[0].subcategories[1].id.expect("editors");

        conn.execute(
            "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
             VALUES (1, 'o/r', 'o', 'r', 'https://github.com/o/r', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("repo");
        set_manual_category(&conn, 1, editors_id).expect("assign");

        let mut next = edit;
        next.categories[0].subcategories.retain(|s| s.id == Some(cli_id));
        update_taxonomy(&conn, &next).expect("update");

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .expect("count");
        assert_eq!(remaining, 2);
        let assigns: i64 = conn
            .query_row("SELECT COUNT(*) FROM repo_categories", [], |r| r.get(0))
            .expect("assigns");
        assert_eq!(assigns, 0);
    }
}
