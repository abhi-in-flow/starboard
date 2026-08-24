-- Migration 004: local stale-star review state (Feature 3).
-- Applied after 003_hardening. Review actions are local-only: they never
-- write stars back to GitHub.

CREATE TABLE repo_review (
  repo_id       INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
  reviewed_at   TEXT,
  snoozed_until TEXT
);

CREATE INDEX idx_repo_review_snoozed_until ON repo_review (snoozed_until);
