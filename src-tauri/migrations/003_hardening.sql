-- Migration 003: production hardening
-- Browse/filter indexes, persisted document hashes, README retry metadata.
-- ALTER TABLE is allowed only inside numbered migrations (never ad hoc).

-- Active-library browse + sort paths (unstarred filter is the common case).
CREATE INDEX IF NOT EXISTS idx_repos_starred_at_active
  ON repos(starred_at DESC) WHERE unstarred = 0;
CREATE INDEX IF NOT EXISTS idx_repos_stars_active
  ON repos(stars_count DESC) WHERE unstarred = 0;
CREATE INDEX IF NOT EXISTS idx_repos_pushed_at_active
  ON repos(pushed_at DESC) WHERE unstarred = 0;
CREATE INDEX IF NOT EXISTS idx_repos_full_name_active
  ON repos(full_name COLLATE NOCASE) WHERE unstarred = 0;
CREATE INDEX IF NOT EXISTS idx_repos_language_active
  ON repos(language) WHERE unstarred = 0 AND language IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_repos_archived_active
  ON repos(archived) WHERE unstarred = 0;

-- Category filter / assignment lookups.
CREATE INDEX IF NOT EXISTS idx_repo_categories_category
  ON repo_categories(category_id);
CREATE INDEX IF NOT EXISTS idx_repo_categories_source
  ON repo_categories(source);

-- Crash-recovery scan of leftover running jobs.
CREATE INDEX IF NOT EXISTS idx_sync_log_status
  ON sync_log(status);

-- Cheap embed-status join: persisted hash vs repo_embedding_meta.content_hash.
ALTER TABLE repos ADD COLUMN document_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_repos_document_hash
  ON repos(document_hash) WHERE unstarred = 0;

-- README retry metadata.
-- readme_status: NULL = never attempted; 'ok' | 'missing' (confirmed 404) | 'retryable'
ALTER TABLE repos ADD COLUMN readme_status TEXT;
ALTER TABLE repos ADD COLUMN readme_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repos ADD COLUMN readme_last_error TEXT;

CREATE INDEX IF NOT EXISTS idx_repos_readme_retry
  ON repos(id) WHERE unstarred = 0
    AND (readme_excerpt IS NULL OR readme_status = 'retryable');
