-- Migration 002: vector embeddings (Phase 4 / HLD §3 v2)
-- sqlite-vec must be registered as an auto-extension before this runs.

INSERT INTO settings (key, value) VALUES ('embed_dimension', '768')
ON CONFLICT(key) DO NOTHING;

INSERT INTO settings (key, value) VALUES ('embeddings_table_dimension', '768')
ON CONFLICT(key) DO NOTHING;

-- Tracks content-hash staleness alongside the vec0 table (dimension is fixed
-- at CREATE time; rebuild_embeddings_table drops + recreates when it changes).
CREATE TABLE repo_embedding_meta (
  repo_id     INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
  content_hash TEXT NOT NULL,
  model       TEXT NOT NULL,
  dimension   INTEGER NOT NULL,
  embedded_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE repo_embeddings USING vec0(
  repo_id INTEGER PRIMARY KEY,
  embedding float[768]
);
