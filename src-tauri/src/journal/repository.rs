use std::{collections::BTreeMap, error::Error as StdError, str::FromStr};

use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type, OptionalExtension, Row};
use uuid::Uuid;

use crate::{
    action::{ActionError, ActionId, ActionParameters},
    backup::{BackupEnvelope, BackupPayload, Fingerprint, RegistryBackup},
    error::{CoreError, CoreResult},
};

use super::{
    database::JournalDatabase,
    models::{
        AppliedBackup, ItemState, PersistedItem, PreparedItem, RecoveryClassification,
        RecoveryTransaction, TimelineItem, TimelineStage, TransactionState,
    },
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: u32 = 1;

impl JournalDatabase {
    pub fn record_prepared_transaction(
        &self,
        transaction_id: Uuid,
        purpose: &str,
        owner: &str,
        os_fingerprint: &str,
        items: &[PreparedItem],
        now_ms: u64,
    ) -> CoreResult<()> {
        self.record_prepared_transaction_inner(
            transaction_id,
            purpose,
            owner,
            os_fingerprint,
            items,
            None,
            now_ms,
        )
    }

    /// Trial registration and its rollback backup are one durable decision.
    /// If either insert fails, no prepared transaction is left for the engine to apply.
    #[allow(clippy::too_many_arguments)]
    pub fn record_prepared_trial_transaction(
        &self,
        transaction_id: Uuid,
        purpose: &str,
        owner: &str,
        os_fingerprint: &str,
        items: &[PreparedItem],
        expires_at_unix_ms: u64,
        now_ms: u64,
    ) -> CoreResult<()> {
        self.record_prepared_transaction_inner(
            transaction_id,
            purpose,
            owner,
            os_fingerprint,
            items,
            Some(expires_at_unix_ms),
            now_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn record_prepared_transaction_inner(
        &self,
        transaction_id: Uuid,
        purpose: &str,
        owner: &str,
        os_fingerprint: &str,
        items: &[PreparedItem],
        trial_expires_at_unix_ms: Option<u64>,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        self.with_immediate_transaction(|database| {
            database.execute(
                "INSERT INTO transactions(
                    transaction_id, purpose, owner, state, os_fingerprint, app_version,
                    protocol_version, started_at_unix_ms
                 ) VALUES (?1, ?2, ?3, 'PREPARED', ?4, ?5, ?6, ?7)",
                params![
                    transaction_id.to_string(),
                    purpose,
                    owner,
                    os_fingerprint,
                    APP_VERSION,
                    PROTOCOL_VERSION,
                    now
                ],
            )?;

            for item in items {
                let invocation_json = serialize_invocation_for_journal(&item.parameters)
                    .map_err(|error| sql_conversion(0, Type::Text, error))?;
                let resource_keys_json = serde_json::to_string(&item.resource_keys)
                    .map_err(|error| sql_conversion(0, Type::Text, error))?;
                database.execute(
                    "INSERT INTO transaction_items(
                        item_id, transaction_id, ordinal, action_id, action_version,
                        invocation_json, resource_keys_json, stage, state,
                        precondition_fingerprint, desired_fingerprint, started_at_unix_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'BACKUP', 'PREPARED',
                               ?8, ?9, ?10)",
                    params![
                        item.item_id.to_string(),
                        transaction_id.to_string(),
                        i64::from(item.ordinal),
                        item.action_id.as_str(),
                        i64::from(item.action_version),
                        invocation_json,
                        resource_keys_json,
                        item.backup.precondition_fingerprint.to_hex(),
                        item.backup.intended_fingerprint.to_hex(),
                        now
                    ],
                )?;
                insert_backup(database, &item.backup, item.resource_keys.first())?;
                insert_stage_event(
                    database,
                    transaction_id,
                    Some(item.item_id),
                    "BACKUP",
                    "complete",
                    1,
                    None,
                    None,
                    now,
                )?;
            }

            insert_stage_event(
                database,
                transaction_id,
                None,
                "PREPARED",
                "complete",
                1,
                None,
                None,
                now,
            )?;
            if let Some(expires_at) = trial_expires_at_unix_ms {
                require_exactly_one(database.execute(
                    "INSERT INTO trials(
                        transaction_id, expires_at_unix_ms,
                        confirmed_at_unix_ms, created_at_unix_ms
                     ) VALUES (?1, ?2, NULL, ?3)",
                    params![transaction_id.to_string(), to_i64(expires_at), now],
                )?)?;
            }
            Ok(())
        })
    }

    pub fn mark_item_applying(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        apply_order: u32,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "UPDATE transactions SET state = 'APPLYING'
                 WHERE transaction_id = ?1 AND state IN ('PREPARED', 'APPLYING')",
                [transaction_id.to_string()],
            )?)?;
            require_exactly_one(database.execute(
                "UPDATE transaction_items
                 SET state = 'APPLYING', stage = 'APPLY', apply_order = ?2,
                     started_at_unix_ms = COALESCE(started_at_unix_ms, ?3)
                 WHERE item_id = ?1 AND state = 'PREPARED'",
                params![item_id.to_string(), i64::from(apply_order), now],
            )?)?;
            insert_stage_event(
                database,
                transaction_id,
                Some(item_id),
                "APPLY",
                "started",
                1,
                None,
                None,
                now,
            )?;
            Ok(())
        })
    }

    pub fn mark_item_applied(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        backup: &BackupEnvelope,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        let payload = serialize_backup(backup)?;
        let applied = backup
            .applied_fingerprint
            .map(Fingerprint::to_hex)
            .unwrap_or_default();
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "UPDATE backups
                 SET applied_fingerprint = ?2, payload = ?3, payload_length = ?4,
                     integrity_sha256 = ?5
                 WHERE item_id = ?1",
                params![
                    item_id.to_string(),
                    applied,
                    payload,
                    i64::try_from(payload.len()).unwrap_or(i64::MAX),
                    backup.integrity_hash.0.as_slice()
                ],
            )?)?;
            require_exactly_one(database.execute(
                "UPDATE transaction_items
                 SET state = 'APPLIED', stage = 'VERIFY_APPLIED', applied_fingerprint = ?2,
                     finished_at_unix_ms = ?3
                 WHERE item_id = ?1 AND state = 'APPLYING'",
                params![item_id.to_string(), applied, now],
            )?)?;
            insert_stage_event(
                database,
                transaction_id,
                Some(item_id),
                "VERIFY_APPLIED",
                "complete",
                1,
                None,
                None,
                now,
            )?;
            Ok(())
        })
    }

    pub fn mark_apply_failure(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        error: &ActionError,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        let code = error.code.as_code().to_owned();
        let stage = error.stage.as_code().to_owned();
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "UPDATE transaction_items
                 SET state = 'APPLY_FAILED', stage = ?2, error_code = ?3,
                     error_stage = ?2, error_retryable = ?4, diagnostic_id = ?5,
                     finished_at_unix_ms = ?6
                 WHERE item_id = ?1 AND state = 'APPLYING'",
                params![
                    item_id.to_string(),
                    stage,
                    code,
                    bool_i64(error.retryable),
                    error.diagnostic_id.to_string(),
                    now
                ],
            )?)?;
            require_exactly_one(database.execute(
                "UPDATE transactions
                 SET primary_error_code = ?2, primary_error_stage = ?3, diagnostic_id = ?4
                 WHERE transaction_id = ?1
                   AND state IN ('APPLYING', 'ROLLING_BACK')",
                params![
                    transaction_id.to_string(),
                    code,
                    stage,
                    error.diagnostic_id.to_string()
                ],
            )?)?;
            insert_stage_event(
                database,
                transaction_id,
                Some(item_id),
                &stage,
                "failed",
                1,
                Some(&code),
                Some(error.diagnostic_id),
                now,
            )?;
            Ok(())
        })
    }

    pub fn mark_item_rolling_back(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        now_ms: u64,
    ) -> CoreResult<()> {
        self.mark_item_rolling_back_inner(transaction_id, item_id, false, now_ms)
    }

    pub fn mark_transaction_item_rolling_back(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        now_ms: u64,
    ) -> CoreResult<()> {
        self.mark_item_rolling_back_inner(transaction_id, item_id, true, now_ms)
    }

    fn mark_item_rolling_back_inner(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        transaction_rollback: bool,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        self.with_immediate_transaction(|database| {
            if transaction_rollback {
                require_exactly_one(database.execute(
                    "UPDATE transactions SET state = 'ROLLING_BACK'
                     WHERE transaction_id = ?1
                       AND state IN ('APPLYING', 'SUCCEEDED', 'ROLLING_BACK',
                                     'RECOVERY_REQUIRED', 'ROLLBACK_FAILED')",
                    [transaction_id.to_string()],
                )?)?;
            }
            require_exactly_one(database.execute(
                "UPDATE transaction_items SET state = 'ROLLING_BACK', stage = 'ROLLBACK'
                 WHERE item_id = ?1
                   AND state IN ('APPLYING', 'APPLIED', 'APPLY_FAILED', 'ROLLING_BACK',
                                 'RECOVERY_REQUIRED', 'ROLLBACK_FAILED')",
                [item_id.to_string()],
            )?)?;
            insert_stage_event(
                database,
                transaction_id,
                Some(item_id),
                "ROLLBACK",
                "started",
                1,
                None,
                None,
                now,
            )?;
            Ok(())
        })
    }

    pub fn mark_item_rolled_back(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        now_ms: u64,
    ) -> CoreResult<()> {
        let now = to_i64(now_ms);
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "UPDATE transaction_items
                 SET state = 'ROLLED_BACK', stage = 'VERIFY_ROLLED_BACK',
                     finished_at_unix_ms = ?2
                 WHERE item_id = ?1
                   AND state IN ('PREPARED', 'APPLYING', 'APPLIED', 'APPLY_FAILED',
                                 'ROLLING_BACK', 'RECOVERY_REQUIRED', 'ROLLBACK_FAILED')",
                params![item_id.to_string(), now],
            )?)?;
            database.execute(
                "UPDATE recovery_items
                 SET status = 'resolved', resolved_at_unix_ms = ?2
                 WHERE item_id = ?1 AND status != 'resolved'",
                params![item_id.to_string(), now],
            )?;

            insert_stage_event(
                database,
                transaction_id,
                Some(item_id),
                "VERIFY_ROLLED_BACK",
                "complete",
                1,
                None,
                None,
                now,
            )?;
            Ok(())
        })
    }

    pub fn record_recovery_item(
        &self,
        transaction_id: Uuid,
        item_id: Uuid,
        classification: RecoveryClassification,
        original_error_code: Option<&str>,
        rollback_error: Option<&ActionError>,
        now_ms: u64,
    ) -> CoreResult<Uuid> {
        let recovery_id = Uuid::new_v4();
        let diagnostic_id = rollback_error
            .map(|error| error.diagnostic_id)
            .unwrap_or_else(Uuid::new_v4);
        let rollback_code = rollback_error.map(|error| error.code.as_code().to_owned());
        let item_state = if rollback_error.is_some() {
            "ROLLBACK_FAILED"
        } else {
            "RECOVERY_REQUIRED"
        };
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "INSERT INTO recovery_items(
                    recovery_id, transaction_id, item_id, classification, status,
                    original_error_code, rollback_error_code, diagnostic_id, created_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, 'recovery_required', ?5, ?6, ?7, ?8)
                 ON CONFLICT(item_id) DO UPDATE SET
                   classification = excluded.classification,
                   status = 'recovery_required',
                   original_error_code = excluded.original_error_code,
                   rollback_error_code = excluded.rollback_error_code,
                   diagnostic_id = excluded.diagnostic_id,
                   created_at_unix_ms = excluded.created_at_unix_ms,
                   resolved_at_unix_ms = NULL",
                params![
                    recovery_id.to_string(),
                    transaction_id.to_string(),
                    item_id.to_string(),
                    classification.as_db(),
                    original_error_code,
                    rollback_code,
                    diagnostic_id.to_string(),
                    to_i64(now_ms)
                ],
            )?)?;
            require_exactly_one(database.execute(
                "UPDATE transaction_items
                 SET state = ?2, stage = 'RECOVERY', diagnostic_id = ?3,
                     error_code = COALESCE(?4, error_code)
                 WHERE item_id = ?1 AND state != 'ROLLED_BACK'",
                params![
                    item_id.to_string(),
                    item_state,
                    diagnostic_id.to_string(),
                    rollback_code
                ],
            )?)?;
            Ok(())
        })?;
        Ok(recovery_id)
    }

    pub fn set_transaction_state(
        &self,
        transaction_id: Uuid,
        state: TransactionState,
        finished: bool,
        now_ms: u64,
    ) -> CoreResult<()> {
        self.with_immediate_transaction(|database| {
            require_exactly_one(database.execute(
                "UPDATE transactions
                 SET state = ?2, finished_at_unix_ms = CASE WHEN ?3 = 1 THEN ?4 ELSE NULL END
                 WHERE transaction_id = ?1",
                params![
                    transaction_id.to_string(),
                    state.as_db(),
                    bool_i64(finished),
                    to_i64(now_ms)
                ],
            )?)?;
            insert_stage_event(
                database,
                transaction_id,
                None,
                state.as_db(),
                "complete",
                1,
                None,
                None,
                to_i64(now_ms),
            )?;
            Ok(())
        })
    }

    pub fn load_item(&self, item_id: Uuid) -> CoreResult<Option<PersistedItem>> {
        self.with_connection(|database| {
            database
                .query_row(
                    "SELECT i.item_id, i.transaction_id, i.ordinal, i.apply_order,
                            i.action_id, i.action_version, i.invocation_json, i.state,
                            b.payload, i.error_code, i.diagnostic_id
                     FROM transaction_items i
                     JOIN backups b ON b.item_id = i.item_id
                     WHERE i.item_id = ?1",
                    [item_id.to_string()],
                    persisted_item_from_row,
                )
                .optional()
        })
    }

    pub fn load_recovery_transactions(&self) -> CoreResult<Vec<RecoveryTransaction>> {
        self.with_connection(|database| {
            let mut transaction_statement = database.prepare(
                "SELECT t.transaction_id, t.state, t.os_fingerprint
                 FROM transactions t
                 WHERE t.state IN ('PREPARED', 'APPLYING', 'APPLIED', 'ROLLING_BACK')
                    OR EXISTS (
                        SELECT 1 FROM recovery_items r
                        WHERE r.transaction_id = t.transaction_id
                          AND r.status != 'resolved')
                    OR (t.state = 'SUCCEEDED' AND EXISTS (
                        SELECT 1 FROM transaction_items si
                        WHERE si.transaction_id = t.transaction_id
                          AND si.action_id = 'session.prevent_sleep'
                          AND si.state != 'ROLLED_BACK'
                    ))
                 ORDER BY t.started_at_unix_ms ASC",
            )?;
            let transaction_rows = transaction_statement
                .query_map([], |row| {
                    let transaction_id = parse_uuid(row.get::<_, String>(0)?, 0)?;
                    let state_text: String = row.get(1)?;
                    let state = TransactionState::from_db(&state_text)
                        .ok_or_else(|| sql_message(1, Type::Text, "unknown transaction state"))?;
                    Ok((transaction_id, state, row.get::<_, String>(2)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut transactions = Vec::with_capacity(transaction_rows.len());
            for (transaction_id, state, os_fingerprint) in transaction_rows {
                let mut item_statement = database.prepare(
                    "SELECT i.item_id, i.transaction_id, i.ordinal, i.apply_order,
                            i.action_id, i.action_version, i.invocation_json, i.state,
                            b.payload, i.error_code, i.diagnostic_id
                     FROM transaction_items i
                     JOIN backups b ON b.item_id = i.item_id
                     WHERE i.transaction_id = ?1
                       AND (EXISTS (
                              SELECT 1 FROM recovery_items ri
                              WHERE ri.item_id = i.item_id
                                AND ri.status != 'resolved')
                         OR (?2 = 'SUCCEEDED'
                             AND i.action_id = 'session.prevent_sleep'
                             AND i.state != 'ROLLED_BACK')
                         OR ?2 IN ('PREPARED', 'APPLYING', 'APPLIED', 'ROLLING_BACK'))
                     ORDER BY COALESCE(i.apply_order, i.ordinal) ASC",
                )?;
                let items = item_statement
                    .query_map(
                        params![transaction_id.to_string(), state.as_db()],
                        persisted_item_from_row,
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                transactions.push(RecoveryTransaction {
                    transaction_id,
                    state,
                    os_fingerprint,
                    items,
                });
            }
            Ok(transactions)
        })
    }

    /// 試用として適用したことを記録する。期限までに確定されなければ、起動時に戻す対象になる。
    pub fn begin_trial(
        &self,
        transaction_id: Uuid,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> CoreResult<()> {
        self.with_connection(|database| {
            database
                .execute(
                    "INSERT OR REPLACE INTO trials
                       (transaction_id, expires_at_unix_ms, confirmed_at_unix_ms, created_at_unix_ms)
                     VALUES (?1, ?2, NULL, ?3)",
                    rusqlite::params![
                        transaction_id.to_string(),
                        expires_at_unix_ms as i64,
                        now_unix_ms as i64
                    ],
                )
                ?;
            Ok(())
        })
    }

    /// 利用者が「保存する」を押した。以後この試用は自動で戻さない。
    pub fn confirm_trial(&self, transaction_id: Uuid, now_unix_ms: u64) -> CoreResult<bool> {
        self.with_connection(|database| {
            let changed = database.execute(
                "UPDATE trials SET confirmed_at_unix_ms = ?2
                     WHERE transaction_id = ?1 AND confirmed_at_unix_ms IS NULL",
                rusqlite::params![transaction_id.to_string(), now_unix_ms as i64],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn transaction_contains_action(
        &self,
        transaction_id: Uuid,
        action_id: crate::action::ActionId,
    ) -> CoreResult<bool> {
        self.with_connection(|database| {
            let count: i64 = database.query_row(
                "SELECT COUNT(*) FROM transaction_items
                 WHERE transaction_id = ?1 AND action_id = ?2",
                rusqlite::params![transaction_id.to_string(), action_id.as_str()],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }

    /// 期限を過ぎても確定されていない試用。起動時にこれを元へ戻す。
    pub fn expired_trials(&self, now_unix_ms: u64) -> CoreResult<Vec<Uuid>> {
        self.with_connection(|database| {
            let mut statement = database.prepare(
                "SELECT transaction_id FROM trials
                     WHERE confirmed_at_unix_ms IS NULL AND expires_at_unix_ms <= ?1
                     ORDER BY expires_at_unix_ms",
            )?;
            let rows = statement.query_map(rusqlite::params![now_unix_ms as i64], |row| {
                row.get::<_, String>(0)
            })?;
            let mut ids = Vec::new();
            for row in rows {
                let raw = row?;
                if let Ok(id) = Uuid::parse_str(&raw) {
                    ids.push(id);
                }
            }
            Ok(ids)
        })
    }

    /// Items belonging to one still-pending trial, without a timeline-wide limit.
    pub fn pending_trial_items(&self, transaction_id: Uuid) -> CoreResult<Vec<(Uuid, ItemState)>> {
        self.with_connection(|database| {
            let mut statement = database.prepare(
                "SELECT i.item_id, i.state
                 FROM trials tr
                 JOIN transaction_items i ON i.transaction_id = tr.transaction_id
                 WHERE tr.transaction_id = ?1 AND tr.confirmed_at_unix_ms IS NULL
                 ORDER BY i.ordinal ASC",
            )?;
            let items = statement
                .query_map([transaction_id.to_string()], |row| {
                    let item_id = parse_uuid(row.get::<_, String>(0)?, 0)?;
                    let state_text: String = row.get(1)?;
                    let state = ItemState::from_db(&state_text)
                        .ok_or_else(|| sql_message(1, Type::Text, "unknown item state"))?;
                    Ok((item_id, state))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        })
    }

    /// 戻し終えた、または確定済みになった試用を片付ける。
    pub fn clear_trial(&self, transaction_id: Uuid) -> CoreResult<()> {
        self.with_connection(|database| {
            database.execute(
                "DELETE FROM trials WHERE transaction_id = ?1",
                rusqlite::params![transaction_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn recovery_count(&self) -> CoreResult<u32> {
        self.with_connection(|database| {
            let count: i64 = database.query_row(
                "SELECT
                   (SELECT COUNT(*) FROM recovery_items WHERE status != 'resolved') +
                   (SELECT COUNT(*) FROM transactions t
                    WHERE t.state IN ('PREPARED', 'APPLYING', 'APPLIED', 'ROLLING_BACK')
                      AND NOT EXISTS (
                        SELECT 1 FROM recovery_items r
                        WHERE r.transaction_id = t.transaction_id
                          AND r.status != 'resolved'))",
                [],
                |row| row.get(0),
            )?;
            Ok(u32::try_from(count).unwrap_or(u32::MAX))
        })
    }

    /// いま効いているはずの適用を、バックアップつきで返す。
    ///
    /// 「効いているはず」は、項目が APPLIED のまま、取引が SUCCEEDED のもの。
    /// 戻した分（ROLLED_BACK）は基準にならないので外す。
    /// 同じ Action を何度も適用していたら、**最後の1回だけ**を基準にする。
    pub fn applied_backups(&self) -> CoreResult<(Vec<AppliedBackup>, usize)> {
        self.with_connection(|database| {
            let mut statement = database.prepare(
                "SELECT i.action_id, b.payload, b.integrity_sha256,
                        COALESCE(i.finished_at_unix_ms, t.started_at_unix_ms)
                 FROM transaction_items i
                 JOIN transactions t ON t.transaction_id = i.transaction_id
                 JOIN backups b ON b.item_id = i.item_id
                 WHERE i.state = 'APPLIED' AND t.state = 'SUCCEEDED'
                 ORDER BY COALESCE(i.finished_at_unix_ms, t.started_at_unix_ms) ASC",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            // 古い順に読んで同じ Action を上書きしていくので、残るのは最後の適用。
            let mut latest: BTreeMap<String, AppliedBackup> = BTreeMap::new();
            let mut unreadable = 0usize;
            for (action_id, payload, stored_integrity, applied_at_unix_ms) in rows {
                let Ok(backup) = serde_json::from_slice::<BackupEnvelope>(&payload) else {
                    // 読めない記録を「基準どおり」に数えるわけにはいかない。
                    // かといって黙って落とすと、件数のどこにも現れず、
                    // **確認できなかったことすら見えなくなる。** 数だけ残す。
                    unreadable += 1;
                    continue;
                };
                if !backup.verify_integrity()
                    || stored_integrity.as_slice() != backup.integrity_hash.0.as_slice()
                {
                    unreadable += 1;
                    continue;
                }
                latest.insert(
                    action_id.clone(),
                    AppliedBackup {
                        action_id,
                        applied_at_unix_ms,
                        backup,
                    },
                );
            }
            Ok((latest.into_values().collect(), unreadable))
        })
    }

    pub fn list_timeline(&self, limit: u32) -> CoreResult<Vec<TimelineItem>> {
        self.with_connection(|database| {
            let mut statement = database.prepare(
                "SELECT i.item_id, i.transaction_id, i.action_id, i.state,
                        t.state, t.started_at_unix_ms, i.error_retryable,
                        i.diagnostic_id
                 FROM transaction_items i
                 JOIN transactions t ON t.transaction_id = i.transaction_id
                 ORDER BY t.started_at_unix_ms DESC, i.ordinal DESC
                 LIMIT ?1",
            )?;
            let raw_rows = statement
                .query_map([i64::from(limit.min(500))], timeline_raw_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            hydrate_timeline(database, raw_rows)
        })
    }

    /// One transaction's complete item set, independent of the UI timeline cap.
    pub fn transaction_timeline(&self, transaction_id: Uuid) -> CoreResult<Vec<TimelineItem>> {
        self.with_connection(|database| {
            let mut statement = database.prepare(
                "SELECT i.item_id, i.transaction_id, i.action_id, i.state,
                        t.state, t.started_at_unix_ms, i.error_retryable,
                        i.diagnostic_id
                 FROM transaction_items i
                 JOIN transactions t ON t.transaction_id = i.transaction_id
                 WHERE i.transaction_id = ?1
                 ORDER BY i.ordinal DESC",
            )?;
            let raw_rows = statement
                .query_map([transaction_id.to_string()], timeline_raw_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            hydrate_timeline(database, raw_rows)
        })
    }
}

type TimelineRawRow = (Uuid, Uuid, String, String, String, i64, bool, Option<Uuid>);

fn timeline_raw_row(row: &Row<'_>) -> rusqlite::Result<TimelineRawRow> {
    Ok((
        parse_uuid(row.get::<_, String>(0)?, 0)?,
        parse_uuid(row.get::<_, String>(1)?, 1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, i64>(5)?,
        row.get::<_, i64>(6)? != 0,
        row.get::<_, Option<String>>(7)?
            .map(|value| parse_uuid(value, 7))
            .transpose()?,
    ))
}

fn hydrate_timeline(
    database: &rusqlite::Connection,
    raw_rows: Vec<TimelineRawRow>,
) -> rusqlite::Result<Vec<TimelineItem>> {
    let mut result = Vec::with_capacity(raw_rows.len());
    for (
        item_id,
        transaction_id,
        action_id,
        item_state,
        transaction_state,
        started_ms,
        retryable,
        diagnostic_id,
    ) in raw_rows
    {
        let mut stage_statement = database.prepare(
            "SELECT stage, outcome FROM stage_events
             WHERE item_id = ?1 ORDER BY event_id ASC",
        )?;
        let stages = stage_statement
            .query_map([item_id.to_string()], |row| {
                let outcome: String = row.get(1)?;
                let status = match outcome.as_str() {
                    "complete" => "complete",
                    "failed" => "failed",
                    _ => "pending",
                };
                Ok(TimelineStage {
                    name: row.get(0)?,
                    status: status.to_owned(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let status = timeline_status(&item_state, &transaction_state);
        result.push(TimelineItem {
            item_id,
            transaction_id,
            action_id: action_id.clone(),
            title: action_id,
            summary: "変更前状態と適用結果を耐久記録で保持しています。".to_owned(),
            status: status.to_owned(),
            started_at: format_timestamp(started_ms),
            before: "変更直前の状態（保存済み）".to_owned(),
            after: "適用後の検証状態".to_owned(),
            rollback_available: item_state == "APPLIED" && transaction_state == "SUCCEEDED",
            retry_available: retryable,
            stages,
            diagnostic_id,
        });
    }
    Ok(result)
}

fn insert_backup(
    database: &rusqlite::Transaction<'_>,
    backup: &BackupEnvelope,
    resource_key: Option<&String>,
) -> rusqlite::Result<()> {
    let payload = serialize_backup_sql(backup)?;
    database.execute(
        "INSERT INTO backups(
            backup_id, transaction_id, item_id, action_id, action_version,
            primitive_kind, codec_version, scope, owner, resource_key,
            precondition_fingerprint, desired_fingerprint, applied_fingerprint,
            payload, payload_length, integrity_sha256, os_base_build,
            rollback_across_unknown_build, created_at_unix_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'user', 'manual', ?8,
                   ?9, ?10, NULL, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            backup.backup_id.to_string(),
            backup.transaction_id.to_string(),
            backup.item_id.to_string(),
            backup.action_id.as_str(),
            i64::from(backup.action_version),
            primitive_name(&backup.payload),
            i64::from(backup.codec_version),
            resource_key
                .map(String::as_str)
                .unwrap_or(backup.action_id.as_str()),
            backup.precondition_fingerprint.to_hex(),
            backup.intended_fingerprint.to_hex(),
            payload,
            i64::try_from(payload.len()).unwrap_or(i64::MAX),
            backup.integrity_hash.0.as_slice(),
            i64::from(backup.os_build),
            bool_i64(backup.rollback_across_unknown_build),
            to_i64(backup.created_at_unix_ms)
        ],
    )?;
    for (ordinal, registry) in registry_backups(&backup.payload).into_iter().enumerate() {
        database.execute(
            "INSERT INTO registry_backup_entries(
                backup_id, entry_ordinal, hive, canonical_subkey, value_name,
                registry_view, key_existed, value_existed, original_type, original_raw,
                intended_type, intended_raw, applied_type, applied_raw, key_created
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15)",
            params![
                backup.backup_id.to_string(),
                i64::try_from(ordinal).unwrap_or(i64::MAX),
                format!("{:?}", registry.location.hive).to_lowercase(),
                registry.location.canonical_subkey,
                registry.location.value_name,
                format!("{:?}", registry.location.view).to_lowercase(),
                bool_i64(registry.original.key_existed),
                bool_i64(registry.original.value_existed),
                registry.original.value_type.map(i64::from),
                registry.original.raw_bytes,
                i64::from(registry.intended_type),
                registry.intended_raw,
                i64::from(registry.applied_type),
                registry.applied_raw,
                bool_i64(!registry.original.key_existed)
            ],
        )?;
    }
    Ok(())
}

fn serialize_invocation_for_journal(parameters: &ActionParameters) -> serde_json::Result<String> {
    let safe_parameters = match parameters {
        ActionParameters::SessionDefaultPrinter { .. } => ActionParameters::SessionDefaultPrinter {
            scene: crate::action::SceneLabel::journal_redacted(),
            printer: crate::windows::PrinterName::journal_redacted(),
        },
        ActionParameters::SessionTemporaryVpn { .. } => ActionParameters::SessionTemporaryVpn {
            connection: crate::windows::VpnEntryName::journal_redacted(),
        },
        ActionParameters::SetupWindowLayout { invocation } => {
            let mut invocation = invocation.clone();
            for entry in &mut invocation.desired.entries {
                entry.application_label = "[REDACTED]".to_owned();
                entry.class_name = "[REDACTED]".to_owned();
                entry.title = crate::window_layout::SensitiveWindowTitle::journal_redacted();
            }
            ActionParameters::SetupWindowLayout { invocation }
        }
        _ => parameters.clone(),
    };
    serde_json::to_string(&safe_parameters)
}

fn persisted_item_from_row(row: &Row<'_>) -> rusqlite::Result<PersistedItem> {
    let item_id = parse_uuid(row.get::<_, String>(0)?, 0)?;
    let transaction_id = parse_uuid(row.get::<_, String>(1)?, 1)?;
    let ordinal = u32::try_from(row.get::<_, i64>(2)?)
        .map_err(|error| sql_conversion(2, Type::Integer, error))?;
    let apply_order = row
        .get::<_, Option<i64>>(3)?
        .map(|value| u32::try_from(value).map_err(|error| sql_conversion(3, Type::Integer, error)))
        .transpose()?;
    let action_text: String = row.get(4)?;
    let action_id =
        ActionId::from_str(&action_text).map_err(|error| sql_conversion(4, Type::Text, error))?;
    let action_version = u32::try_from(row.get::<_, i64>(5)?)
        .map_err(|error| sql_conversion(5, Type::Integer, error))?;
    let mut parameters = serde_json::from_str::<ActionParameters>(&row.get::<_, String>(6)?)
        .map_err(|error| sql_conversion(6, Type::Text, error))?;
    if parameters.action_id() != action_id {
        return Err(sql_message(
            6,
            Type::Text,
            "Action ID and parameters disagree",
        ));
    }
    let state_text: String = row.get(7)?;
    let state = ItemState::from_db(&state_text)
        .ok_or_else(|| sql_message(7, Type::Text, "unknown item state"))?;
    let backup = serde_json::from_slice::<BackupEnvelope>(&row.get::<_, Vec<u8>>(8)?)
        .map_err(|error| sql_conversion(8, Type::Blob, error))?;
    if backup.item_id != item_id
        || backup.transaction_id != transaction_id
        || backup.action_id != action_id
        || !backup.verify_integrity()
    {
        return Err(sql_message(8, Type::Blob, "backup integrity mismatch"));
    }
    restore_invocation_from_backup(&mut parameters, &backup.payload);
    let diagnostic_id = row
        .get::<_, Option<String>>(10)?
        .map(|value| parse_uuid(value, 10))
        .transpose()?;
    Ok(PersistedItem {
        item_id,
        transaction_id,
        ordinal,
        apply_order,
        action_id,
        action_version,
        parameters,
        state,
        backup,
        error_code: row.get(9)?,
        diagnostic_id,
    })
}

fn restore_invocation_from_backup(parameters: &mut ActionParameters, payload: &BackupPayload) {
    match (parameters, payload) {
        (
            ActionParameters::SessionTemporaryVpn { connection },
            BackupPayload::TemporaryVpn(payload),
        ) => *connection = payload.connection.clone(),
        (
            ActionParameters::SessionDefaultPrinter { printer, .. },
            BackupPayload::DefaultPrinter(payload),
        ) => *printer = payload.intended.clone(),
        (
            ActionParameters::SetupWindowLayout { invocation },
            BackupPayload::WindowLayout(payload),
        ) => {
            invocation.desired = payload.desired.clone();
            invocation.excluded_game_file_identities =
                payload.excluded_game_file_identities.clone();
        }
        _ => {}
    }
}

// journal の 1 行に入る列がそのまま引数になっている。まとめる構造体を作っても
// 呼び出し側で同じ数を埋めることに変わりはない。
#[allow(clippy::too_many_arguments)]
fn insert_stage_event(
    database: &rusqlite::Transaction<'_>,
    transaction_id: Uuid,
    item_id: Option<Uuid>,
    stage: &str,
    outcome: &str,
    attempt: u32,
    error_code: Option<&str>,
    diagnostic_id: Option<Uuid>,
    occurred_at: i64,
) -> rusqlite::Result<()> {
    database.execute(
        "INSERT INTO stage_events(
            transaction_id, item_id, stage, outcome, attempt, error_code,
            diagnostic_id, occurred_at_unix_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            transaction_id.to_string(),
            item_id.map(|value| value.to_string()),
            stage,
            outcome,
            i64::from(attempt),
            error_code,
            diagnostic_id.map(|value| value.to_string()),
            occurred_at
        ],
    )?;
    Ok(())
}

fn primitive_name(payload: &BackupPayload) -> &'static str {
    match payload {
        BackupPayload::Registry(_) => "registry",
        BackupPayload::Composite(_) => "composite",
        BackupPayload::SleepLease(_) => "sleep_lease",
        BackupPayload::Observation(_) => "observation",
        BackupPayload::ProcessWatch(_) => "process_watch",
        BackupPayload::PowerScheme(_) => "power_scheme",
        BackupPayload::PowerMode(_) => "power_mode",
        BackupPayload::PointerFeel(_) => "pointer_feel",
        BackupPayload::CommsMicMute(_) => "comms_mic_mute",
        BackupPayload::WindowLayout(_) => "window_layout",
        BackupPayload::ShiftInterruptionGuard(_) => "shift_interruption_guard",
        BackupPayload::DefaultPrinter(_) => "default_printer",
        BackupPayload::TemporaryVpn(_) => "temporary_vpn",
        BackupPayload::HighContrast(_) => "high_contrast",
        BackupPayload::AppVolumeReset(_) => "app_volume_reset",
    }
}

fn registry_backups(payload: &BackupPayload) -> Vec<&RegistryBackup> {
    match payload {
        BackupPayload::Registry(backup) => vec![backup],
        BackupPayload::Composite(composite) => composite.registry_entries.iter().collect(),
        _ => Vec::new(),
    }
}

fn serialize_backup(backup: &BackupEnvelope) -> CoreResult<Vec<u8>> {
    serde_json::to_vec(backup).map_err(|_| CoreError::storage())
}

fn serialize_backup_sql(backup: &BackupEnvelope) -> rusqlite::Result<Vec<u8>> {
    serde_json::to_vec(backup).map_err(|error| sql_conversion(0, Type::Blob, error))
}

fn parse_uuid(value: String, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| sql_conversion(column, Type::Text, error))
}

fn sql_conversion(
    column: usize,
    data_type: Type,
    error: impl StdError + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, data_type, Box::new(error))
}

fn sql_message(column: usize, data_type: Type, message: &'static str) -> rusqlite::Error {
    sql_conversion(column, data_type, std::io::Error::other(message))
}

fn require_exactly_one(affected_rows: usize) -> rusqlite::Result<()> {
    if affected_rows == 1 {
        Ok(())
    } else {
        Err(sql_message(
            0,
            Type::Null,
            "journal state transition affected an unexpected row count",
        ))
    }
}

fn bool_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn timeline_status(item_state: &str, transaction_state: &str) -> &'static str {
    match item_state {
        "ROLLED_BACK" => "rolled_back",
        "ROLLBACK_FAILED" => "rollback_failed",
        "RECOVERY_REQUIRED" => "recovery_required",
        "ROLLING_BACK" => "rolling_back",
        "APPLYING" => "applying",
        _ if transaction_state == "RECOVERY_REQUIRED" => "recovery_required",
        _ if transaction_state == "ROLLBACK_FAILED" => "rollback_failed",
        _ => "succeeded",
    }
}

fn format_timestamp(unix_ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(unix_ms)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

#[cfg(test)]
mod privacy_tests {
    use super::*;

    fn private_window_invocation(private_title: &str) -> ActionParameters {
        use crate::{
            action::ProcessFileIdentity,
            window_layout::{
                SavedWindowPlacement, SavedWindowPlacementEntry, SensitiveWindowTitle,
                WindowLayoutInvocation, WindowLayoutSnapshot, WindowPoint, WindowRect,
            },
        };

        ActionParameters::SetupWindowLayout {
            invocation: WindowLayoutInvocation {
                desired: WindowLayoutSnapshot {
                    snapshot_id: uuid::Uuid::new_v4(),
                    captured_at_unix_ms: 1,
                    display_profile: None,
                    entries: vec![SavedWindowPlacementEntry {
                        entry_id: uuid::Uuid::new_v4(),
                        process_file_identity: ProcessFileIdentity {
                            volume_serial_number: 7,
                            file_id: [3; 16],
                        },
                        application_label: "private-app.exe".to_owned(),
                        class_name: "PrivateWindowClass".to_owned(),
                        title: SensitiveWindowTitle::new(private_title.to_owned())
                            .expect("bounded private title"),
                        placement: SavedWindowPlacement {
                            flags: 0,
                            show_cmd: 1,
                            min_position: WindowPoint { x: -1, y: -1 },
                            max_position: WindowPoint { x: -1, y: -1 },
                            normal_position: WindowRect {
                                left: 10,
                                top: 20,
                                right: 410,
                                bottom: 320,
                            },
                        },
                        observed_rect: WindowRect {
                            left: 10,
                            top: 20,
                            right: 410,
                            bottom: 320,
                        },
                    }],
                    excluded_game_windows: 0,
                    skipped_windows: 0,
                    exclusions: Vec::new(),
                },
                excluded_game_file_identities: Vec::new(),
            },
        }
    }

    #[test]
    fn temporary_vpn_invocation_journal_never_contains_the_connection_name() {
        let private = "Confidential connection name";
        let parameters = ActionParameters::SessionTemporaryVpn {
            connection: crate::windows::VpnEntryName::new(private.to_owned())
                .expect("valid test connection name"),
        };
        let serialized =
            serialize_invocation_for_journal(&parameters).expect("serialize safe invocation");
        assert!(!serialized.contains(private));
        assert!(serialized.contains("[REDACTED]"));
        let decoded: ActionParameters =
            serde_json::from_str(&serialized).expect("redacted invocation remains typed");
        assert_eq!(decoded.action_id(), ActionId::SessionTemporaryVpn);
    }

    #[test]
    fn default_printer_invocation_journal_never_contains_private_labels() {
        let private_scene = "Confidential office";
        let private_printer = "Finance floor printer";
        let parameters = ActionParameters::SessionDefaultPrinter {
            scene: crate::action::SceneLabel::new(private_scene.to_owned()),
            printer: crate::windows::PrinterName::new(private_printer.to_owned())
                .expect("valid test printer"),
        };

        let serialized =
            serialize_invocation_for_journal(&parameters).expect("serialize safe invocation");
        assert!(!serialized.contains(private_scene));
        assert!(!serialized.contains(private_printer));
        assert!(serialized.contains("[REDACTED]"));

        let original = crate::windows::PrinterName::new("Original printer".to_owned())
            .expect("valid original printer");
        let intended = crate::windows::PrinterName::new(private_printer.to_owned())
            .expect("valid intended printer");
        let payload = BackupPayload::DefaultPrinter(crate::backup::DefaultPrinterBackup {
            original,
            intended: intended.clone(),
        });
        let mut decoded: ActionParameters =
            serde_json::from_str(&serialized).expect("decode redacted invocation");
        restore_invocation_from_backup(&mut decoded, &payload);
        let ActionParameters::SessionDefaultPrinter { scene, printer } = decoded else {
            unreachable!();
        };
        assert_eq!(scene.as_str(), "[REDACTED]");
        assert_eq!(printer, intended);
    }

    #[test]
    fn window_layout_invocation_journal_never_contains_window_identity_text() {
        let private_title = "Confidential quarterly plan";
        let parameters = private_window_invocation(private_title);

        let serialized =
            serialize_invocation_for_journal(&parameters).expect("serialize safe invocation");
        assert!(!serialized.contains(private_title));
        assert!(!serialized.contains("private-app.exe"));
        assert!(!serialized.contains("PrivateWindowClass"));
        assert!(serialized.contains("[REDACTED]"));
    }

    #[test]
    fn redacted_window_invocation_is_rehydrated_from_durable_backup() {
        let original = private_window_invocation("Private recovery title");
        let ActionParameters::SetupWindowLayout { invocation } = &original else {
            unreachable!();
        };
        let payload = BackupPayload::WindowLayout(crate::window_layout::WindowLayoutBackup {
            desired: invocation.desired.clone(),
            original_entries: Vec::new(),
            excluded_game_file_identities: invocation.excluded_game_file_identities.clone(),
        });
        let serialized =
            serialize_invocation_for_journal(&original).expect("serialize safe invocation");
        let mut decoded: ActionParameters =
            serde_json::from_str(&serialized).expect("decode redacted invocation");

        restore_invocation_from_backup(&mut decoded, &payload);
        assert_eq!(decoded, original);
    }
}
