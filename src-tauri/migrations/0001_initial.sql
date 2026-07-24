CREATE TABLE IF NOT EXISTS schema_meta (
  schema_version INTEGER PRIMARY KEY,
  applied_at_unix_ms INTEGER NOT NULL,
  app_version TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS os_observations (
  observation_id TEXT PRIMARY KEY,
  base_build INTEGER,
  revision INTEGER,
  edition TEXT NOT NULL,
  architecture TEXT NOT NULL,
  identity_status TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  observed_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS transactions (
  transaction_id TEXT PRIMARY KEY,
  purpose TEXT NOT NULL,
  owner TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN (
    'PLANNED', 'PREFLIGHTING', 'PREPARED', 'APPLYING', 'APPLIED',
    'SUCCEEDED', 'ROLLING_BACK', 'ROLLED_BACK', 'ROLLBACK_FAILED',
    'RECOVERY_REQUIRED'
  )),
  os_fingerprint TEXT NOT NULL,
  app_version TEXT NOT NULL,
  protocol_version INTEGER NOT NULL,
  started_at_unix_ms INTEGER NOT NULL,
  finished_at_unix_ms INTEGER,
  primary_error_code TEXT,
  primary_error_stage TEXT,
  diagnostic_id TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS transaction_items (
  item_id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE RESTRICT,
  ordinal INTEGER NOT NULL,
  apply_order INTEGER,
  action_id TEXT NOT NULL,
  action_version INTEGER NOT NULL,
  invocation_json TEXT NOT NULL,
  resource_keys_json TEXT NOT NULL,
  stage TEXT NOT NULL,
  state TEXT NOT NULL,
  attempt INTEGER NOT NULL DEFAULT 1,
  precondition_fingerprint TEXT NOT NULL,
  desired_fingerprint TEXT NOT NULL,
  applied_fingerprint TEXT,
  started_at_unix_ms INTEGER,
  finished_at_unix_ms INTEGER,
  error_code TEXT,
  error_stage TEXT,
  error_retryable INTEGER NOT NULL DEFAULT 0 CHECK (error_retryable IN (0, 1)),
  diagnostic_id TEXT,
  UNIQUE(transaction_id, ordinal)
) STRICT;

CREATE TABLE IF NOT EXISTS backups (
  backup_id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE RESTRICT,
  item_id TEXT NOT NULL UNIQUE REFERENCES transaction_items(item_id) ON DELETE RESTRICT,
  action_id TEXT NOT NULL,
  action_version INTEGER NOT NULL,
  primitive_kind TEXT NOT NULL,
  codec_version INTEGER NOT NULL,
  scope TEXT NOT NULL CHECK (scope IN ('user', 'privileged')),
  owner TEXT NOT NULL,
  resource_key TEXT NOT NULL,
  precondition_fingerprint TEXT NOT NULL,
  desired_fingerprint TEXT NOT NULL,
  applied_fingerprint TEXT,
  payload BLOB NOT NULL,
  payload_length INTEGER NOT NULL,
  integrity_sha256 BLOB NOT NULL,
  os_base_build INTEGER,
  os_revision INTEGER,
  rollback_across_unknown_build INTEGER NOT NULL DEFAULT 0 CHECK (rollback_across_unknown_build IN (0, 1)),
  created_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS registry_backup_entries (
  backup_id TEXT NOT NULL REFERENCES backups(backup_id) ON DELETE RESTRICT,
  entry_ordinal INTEGER NOT NULL,
  hive TEXT NOT NULL,
  canonical_subkey TEXT NOT NULL,
  value_name TEXT NOT NULL,
  registry_view TEXT NOT NULL CHECK (registry_view IN ('registry32', 'registry64')),
  key_existed INTEGER NOT NULL CHECK (key_existed IN (0, 1)),
  value_existed INTEGER NOT NULL CHECK (value_existed IN (0, 1)),
  original_type INTEGER,
  original_raw BLOB,
  intended_type INTEGER NOT NULL,
  intended_raw BLOB NOT NULL,
  applied_type INTEGER,
  applied_raw BLOB,
  key_created INTEGER NOT NULL DEFAULT 0 CHECK (key_created IN (0, 1)),
  PRIMARY KEY(backup_id, entry_ordinal)
) STRICT;

CREATE TABLE IF NOT EXISTS stage_events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE RESTRICT,
  item_id TEXT REFERENCES transaction_items(item_id) ON DELETE RESTRICT,
  stage TEXT NOT NULL,
  outcome TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  error_code TEXT,
  diagnostic_id TEXT,
  occurred_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS recovery_items (
  recovery_id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE RESTRICT,
  item_id TEXT NOT NULL UNIQUE REFERENCES transaction_items(item_id) ON DELETE RESTRICT,
  classification TEXT NOT NULL CHECK (classification IN ('original', 'applied', 'third', 'unknown')),
  status TEXT NOT NULL CHECK (status IN ('pending', 'resolved', 'recovery_required')),
  original_error_code TEXT,
  rollback_error_code TEXT,
  diagnostic_id TEXT NOT NULL,
  created_at_unix_ms INTEGER NOT NULL,
  resolved_at_unix_ms INTEGER
) STRICT;

CREATE TABLE IF NOT EXISTS action_leases (
  resource_key TEXT NOT NULL,
  desired_fingerprint TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  transaction_id TEXT NOT NULL REFERENCES transactions(transaction_id) ON DELETE RESTRICT,
  acquired_at_unix_ms INTEGER NOT NULL,
  PRIMARY KEY(resource_key, owner_id)
) STRICT;

CREATE TABLE IF NOT EXISTS preview_tokens (
  token_hash BLOB PRIMARY KEY,
  action_ids_json TEXT NOT NULL,
  invocation_json TEXT NOT NULL,
  before_fingerprints_json TEXT NOT NULL,
  os_fingerprint TEXT NOT NULL,
  expires_at_unix_ms INTEGER NOT NULL,
  consumed_at_unix_ms INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS idx_transactions_state ON transactions(state);
CREATE INDEX IF NOT EXISTS idx_items_transaction ON transaction_items(transaction_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_items_state ON transaction_items(state);
CREATE INDEX IF NOT EXISTS idx_stage_events_transaction ON stage_events(transaction_id, event_id);
CREATE INDEX IF NOT EXISTS idx_recovery_status ON recovery_items(status, created_at_unix_ms);

INSERT OR IGNORE INTO schema_meta(schema_version, applied_at_unix_ms, app_version)
VALUES (1, unixepoch('subsec') * 1000, '0.1.0');

