use std::{path::Path, time::Duration};

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

use crate::error::{CoreError, CoreResult};

const INITIAL_SCHEMA: &str = include_str!("../../migrations/0001_initial.sql");
/// 追加分。どちらも `IF NOT EXISTS` なので、既に動いている DB へそのまま流せる。
const TRIALS_SCHEMA: &str = include_str!("../../migrations/0002_trials.sql");
const STORAGE_HISTORY_SCHEMA: &str = include_str!("../../migrations/0003_storage_history.sql");

pub struct JournalDatabase {
    connection: Mutex<Connection>,
}

impl JournalDatabase {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| CoreError::storage())?;
        Self::initialize(connection, true)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> CoreResult<Self> {
        let connection = Connection::open_in_memory().map_err(|_| CoreError::storage())?;
        Self::initialize(connection, false)
    }

    fn initialize(connection: Connection, require_wal: bool) -> CoreResult<Self> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|_| CoreError::storage())?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA trusted_schema = OFF;
                 PRAGMA synchronous = FULL;
                 PRAGMA wal_autocheckpoint = 1000;
                 PRAGMA journal_size_limit = 16777216;
                 PRAGMA secure_delete = FAST;",
            )
            .map_err(|_| CoreError::storage())?;

        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|_| CoreError::storage())?;
        if require_wal && !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(CoreError::new(
                "JOURNAL_MODE_UNSAFE",
                "JOURNAL",
                false,
                "安全な変更記録モードを開始できないため、設定変更を停止しました。",
            ));
        }

        connection
            .execute_batch(INITIAL_SCHEMA)
            .map_err(|_| CoreError::storage())?;
        connection
            .execute_batch(TRIALS_SCHEMA)
            .map_err(|_| CoreError::storage())?;
        connection
            .execute_batch(STORAGE_HISTORY_SCHEMA)
            .map_err(|_| CoreError::storage())?;
        let integrity: String = connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|_| CoreError::storage())?;
        if integrity != "ok" {
            return Err(CoreError::recovery_required(
                "変更記録の整合性を確認できません。新しい変更は停止しています。",
            ));
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> CoreResult<T> {
        let connection = self.connection.lock();
        operation(&connection).map_err(|_| CoreError::storage())
    }

    pub(crate) fn with_immediate_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>,
    ) -> CoreResult<T> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CoreError::storage())?;
        let result = operation(&transaction).map_err(|_| CoreError::storage())?;
        transaction.commit().map_err(|_| CoreError::storage())?;
        Ok(result)
    }

    pub fn checkpoint(&self) -> CoreResult<()> {
        self.with_connection(|connection| connection.execute_batch("PRAGMA wal_checkpoint(FULL);"))
    }
}
