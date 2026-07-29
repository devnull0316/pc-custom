-- Storage history is deliberately separate from the Action journal:
-- it observes Windows and has no rollback/timeline semantics.
--
-- Privacy boundary: these tables contain aggregate numbers and a fixed category
-- enum only. There is no column capable of storing a file name or a path.
CREATE TABLE IF NOT EXISTS storage_history_snapshots (
  snapshot_id INTEGER PRIMARY KEY AUTOINCREMENT,
  captured_at_unix_ms INTEGER NOT NULL,
  drive_total_bytes INTEGER NOT NULL CHECK (drive_total_bytes >= 0),
  drive_total_free_bytes INTEGER NOT NULL CHECK (drive_total_free_bytes >= 0),
  drive_available_bytes INTEGER NOT NULL CHECK (drive_available_bytes >= 0)
) STRICT;

CREATE TABLE IF NOT EXISTS storage_history_categories (
  snapshot_id INTEGER NOT NULL
    REFERENCES storage_history_snapshots(snapshot_id) ON DELETE CASCADE,
  category TEXT NOT NULL CHECK (category IN (
    'documents', 'downloads', 'desktop', 'pictures', 'videos'
  )),
  total_bytes INTEGER NOT NULL CHECK (total_bytes >= 0),
  file_count INTEGER NOT NULL CHECK (file_count >= 0),
  directory_count INTEGER NOT NULL CHECK (directory_count >= 0),
  skipped_reparse_points INTEGER NOT NULL CHECK (skipped_reparse_points >= 0),
  access_denied_count INTEGER NOT NULL CHECK (access_denied_count >= 0),
  unreadable_entries INTEGER NOT NULL CHECK (unreadable_entries >= 0),
  truncated INTEGER NOT NULL CHECK (truncated IN (0, 1)),
  PRIMARY KEY(snapshot_id, category)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_storage_history_captured
  ON storage_history_snapshots(captured_at_unix_ms);
