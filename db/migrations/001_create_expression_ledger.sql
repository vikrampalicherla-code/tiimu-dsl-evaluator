CREATE TABLE IF NOT EXISTS expression_ledger (
  expression_version_id TEXT PRIMARY KEY,
  chronicle_id          TEXT NOT NULL,
  antecedent_id         TEXT REFERENCES expression_ledger(expression_version_id),
  branch_id             TEXT REFERENCES expression_ledger(expression_version_id),

  dsl_text              TEXT NOT NULL,
  ast_json              JSONB NOT NULL,
  ast_hash              TEXT NOT NULL,

  dictionary_snapshot_id TEXT NOT NULL,

  created_by            TEXT NOT NULL,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS expression_labels (
  chronicle_id TEXT NOT NULL,
  label_name   TEXT NOT NULL,
  expression_version_id TEXT NOT NULL REFERENCES expression_ledger(expression_version_id),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (chronicle_id, label_name)
);

CREATE TABLE IF NOT EXISTS expression_dependencies (
  expression_version_id TEXT NOT NULL REFERENCES expression_ledger(expression_version_id),
  dep_type TEXT NOT NULL CHECK (dep_type IN ('field','function','expression')),
  dep_key  TEXT NOT NULL,
  PRIMARY KEY (expression_version_id, dep_type, dep_key)
);

CREATE TABLE IF NOT EXISTS expression_usage_index (
  expression_ref_kind TEXT NOT NULL CHECK (expression_ref_kind IN ('pinned','by_label')),
  expression_version_id TEXT,
  expression_chronicle_id TEXT,
  expression_label_name TEXT,

  referencer_type TEXT NOT NULL,
  referencer_id TEXT NOT NULL,
  referencer_version_id TEXT NOT NULL,

  role TEXT NOT NULL,
  path TEXT,
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
