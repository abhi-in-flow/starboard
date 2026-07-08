-- Migration 001: initial schema (HLD §3, minus vec0)

CREATE TABLE repos (
  id            INTEGER PRIMARY KEY,
  full_name     TEXT NOT NULL UNIQUE,
  owner         TEXT NOT NULL,
  name          TEXT NOT NULL,
  description   TEXT,
  language      TEXT,
  topics        TEXT,
  stars_count   INTEGER,
  forks_count   INTEGER,
  open_issues   INTEGER,
  license       TEXT,
  homepage      TEXT,
  html_url      TEXT NOT NULL,
  archived      INTEGER DEFAULT 0,
  fork          INTEGER DEFAULT 0,
  repo_created_at TEXT,
  pushed_at     TEXT,
  starred_at    TEXT NOT NULL,
  readme_excerpt TEXT,
  fetched_at    TEXT NOT NULL,
  unstarred     INTEGER DEFAULT 0
);

CREATE TABLE categories (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  name      TEXT NOT NULL,
  parent_id INTEGER REFERENCES categories(id),
  UNIQUE(name, parent_id)
);

CREATE TABLE repo_categories (
  repo_id     INTEGER REFERENCES repos(id),
  category_id INTEGER REFERENCES categories(id),
  source      TEXT CHECK(source IN ('llm','manual')),
  confidence  REAL,
  PRIMARY KEY (repo_id, category_id)
);

CREATE TABLE sync_log (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at  TEXT,
  finished_at TEXT,
  kind        TEXT,
  repos_added INTEGER,
  repos_updated INTEGER,
  repos_removed INTEGER,
  status      TEXT,
  error       TEXT
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT
);

CREATE VIRTUAL TABLE repos_fts USING fts5(
  full_name, description, topics, readme_excerpt,
  content='repos', content_rowid='id', tokenize='porter unicode61'
);

CREATE TRIGGER repos_ai AFTER INSERT ON repos BEGIN
  INSERT INTO repos_fts(rowid, full_name, description, topics, readme_excerpt)
  VALUES (new.id, new.full_name, new.description, new.topics, new.readme_excerpt);
END;

CREATE TRIGGER repos_ad AFTER DELETE ON repos BEGIN
  INSERT INTO repos_fts(repos_fts, rowid, full_name, description, topics, readme_excerpt)
  VALUES ('delete', old.id, old.full_name, old.description, old.topics, old.readme_excerpt);
END;

CREATE TRIGGER repos_au AFTER UPDATE ON repos BEGIN
  INSERT INTO repos_fts(repos_fts, rowid, full_name, description, topics, readme_excerpt)
  VALUES ('delete', old.id, old.full_name, old.description, old.topics, old.readme_excerpt);
  INSERT INTO repos_fts(rowid, full_name, description, topics, readme_excerpt)
  VALUES (new.id, new.full_name, new.description, new.topics, new.readme_excerpt);
END;

INSERT INTO settings (key, value) VALUES
  ('ollama_base_url', 'http://127.0.0.1:11434'),
  ('ollama_chat_model', ''),
  ('ollama_embed_model', 'nomic-embed-text');
