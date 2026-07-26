use super::*;
use crate::journal::{ItemState, TransactionState};

impl PcCustomEngine {
    pub fn rollback_item(&self, item_id: Uuid) -> CoreResult<CommitResult> {
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard =
            crate::windows::acquire_core_mutation_lock().map_err(core_mutation_lock_error)?;
        let item = self
            .journal
            .load_item(item_id)?
            .ok_or_else(|| CoreError::invalid_request("指定した履歴項目が見つかりません。"))?;
        if !matches!(item.state, ItemState::Applied | ItemState::RollingBack) {
            return Err(CoreError::invalid_request(
                "この項目は適用中ではないため、重ねて復元できません。",
            ));
        }
        let identity = self.identity_for_commit()?;
        let action = registered_action(item.action_id)?;
        if matches!(
            action.metadata().kind,
            ActionKind::Observation | ActionKind::OneWay
        ) {
            return Err(CoreError::invalid_request(
                "読み取り専用または元に戻せないActionは、この履歴から復元できません。",
            ));
        }
        let context = action_context(&identity, item.transaction_id, item.item_id);
        match classify_action(action, &context, &item.parameters, &item.backup) {
            RecoveryClassification::Original => {
                self.journal
                    .mark_item_rolled_back(item.transaction_id, item.item_id, now_ms())?;
            }
            RecoveryClassification::Applied => {
                ensure_backup_mutation_allowed(&identity, action, &item.backup)?;
                self.journal
                    .mark_item_rolling_back(item.transaction_id, item.item_id, now_ms())?;
                let verification = action
                    .rollback(&context, &item.parameters, &item.backup)
                    .and_then(|_| {
                        action.verify_rolled_back(&context, &item.parameters, &item.backup)
                    });
                match verification {
                    Ok(Verification { verified: true, .. }) => {
                        self.journal.mark_item_rolled_back(
                            item.transaction_id,
                            item.item_id,
                            now_ms(),
                        )?;
                    }
                    outcome => {
                        let rollback_error = match outcome {
                            Ok(_) => ActionError::new(
                                ActionErrorCode::RecoveryRequired,
                                ActionStage::VerifyRolledBack,
                                false,
                                "rollback.verify.mismatch",
                            ),
                            Err(error) => error,
                        };
                        self.journal.record_recovery_item(
                            item.transaction_id,
                            item.item_id,
                            RecoveryClassification::Applied,
                            item.error_code.as_deref(),
                            Some(&rollback_error),
                            now_ms(),
                        )?;
                        return Ok(CommitResult {
                            transaction_id: item.transaction_id,
                            status: "recovery_required".to_owned(),
                            message: "復元を確認できない項目があります。新しい変更を停止しました。"
                                .to_owned(),
                        });
                    }
                }
            }
            classification @ (RecoveryClassification::Third | RecoveryClassification::Unknown) => {
                self.journal.record_recovery_item(
                    item.transaction_id,
                    item.item_id,
                    classification,
                    item.error_code.as_deref(),
                    None,
                    now_ms(),
                )?;
                return Err(CoreError::recovery_required(
                    "適用値と異なる現在状態を検出したため、自動では上書きしません。",
                ));
            }
        }
        Ok(CommitResult {
            transaction_id: item.transaction_id,
            status: "rolled_back".to_owned(),
            message: "この項目を変更直前の状態へ戻し、再確認しました。".to_owned(),
        })
    }

    pub fn reconcile_now(&self) -> CoreResult<ReconcileResult> {
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard =
            crate::windows::acquire_core_mutation_lock().map_err(core_mutation_lock_error)?;
        let transactions = self.journal.load_recovery_transactions()?;
        if transactions.is_empty() {
            return Ok(ReconcileResult {
                status: "clean".to_owned(),
                recovered_count: 0,
                remaining_count: self.journal.recovery_count()?,
                message: "未完了の変更はありません。".to_owned(),
            });
        }
        let identity = self.initial_identity.clone();
        let mut recovered = 0u32;
        let mut remaining = 0u32;
        for transaction in transactions {
            let mut recovery_failure = false;
            let mut rollback_failure = false;
            for item in transaction.items.iter().rev() {
                if item.state == ItemState::RolledBack {
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    self.journal.record_recovery_item(
                        transaction.transaction_id,
                        item.item_id,
                        RecoveryClassification::Unknown,
                        item.error_code.as_deref(),
                        None,
                        now_ms(),
                    )?;
                    recovery_failure = true;
                    remaining = remaining.saturating_add(1);
                    continue;
                };
                let Some(action) = ACTION_REGISTRY.get(item.action_id) else {
                    self.journal.record_recovery_item(
                        transaction.transaction_id,
                        item.item_id,
                        RecoveryClassification::Unknown,
                        item.error_code.as_deref(),
                        None,
                        now_ms(),
                    )?;
                    recovery_failure = true;
                    remaining = remaining.saturating_add(1);
                    continue;
                };
                let context = action_context(identity, transaction.transaction_id, item.item_id);
                let classification =
                    classify_action(action, &context, &item.parameters, &item.backup);
                if item.state == ItemState::Prepared
                    && classification != RecoveryClassification::Original
                {
                    // PREPARED proves no OS primitive was dispatched. A matching
                    // intended value therefore belongs to an external actor and
                    // must never be overwritten by recovery.
                    let ownership = match classification {
                        RecoveryClassification::Applied | RecoveryClassification::Third => {
                            RecoveryClassification::Third
                        }
                        _ => RecoveryClassification::Unknown,
                    };
                    self.journal.record_recovery_item(
                        transaction.transaction_id,
                        item.item_id,
                        ownership,
                        item.error_code.as_deref(),
                        None,
                        now_ms(),
                    )?;
                    recovery_failure = true;
                    remaining = remaining.saturating_add(1);
                    continue;
                }
                match classification {
                    RecoveryClassification::Original => {
                        self.journal.mark_item_rolled_back(
                            transaction.transaction_id,
                            item.item_id,
                            now_ms(),
                        )?;
                        recovered = recovered.saturating_add(1);
                    }
                    RecoveryClassification::Applied => {
                        if ensure_backup_mutation_allowed(identity, action, &item.backup).is_err() {
                            self.journal.record_recovery_item(
                                transaction.transaction_id,
                                item.item_id,
                                RecoveryClassification::Unknown,
                                item.error_code.as_deref(),
                                None,
                                now_ms(),
                            )?;
                            recovery_failure = true;
                            remaining = remaining.saturating_add(1);
                            continue;
                        }
                        self.journal.mark_item_rolling_back(
                            transaction.transaction_id,
                            item.item_id,
                            now_ms(),
                        )?;
                        let result = action
                            .rollback(&context, &item.parameters, &item.backup)
                            .and_then(|_| {
                                action.verify_rolled_back(&context, &item.parameters, &item.backup)
                            });
                        match result {
                            Ok(Verification { verified: true, .. }) => {
                                self.journal.mark_item_rolled_back(
                                    transaction.transaction_id,
                                    item.item_id,
                                    now_ms(),
                                )?;
                                recovered = recovered.saturating_add(1);
                            }
                            Ok(_) => {
                                let error = ActionError::new(
                                    ActionErrorCode::RecoveryRequired,
                                    ActionStage::VerifyRolledBack,
                                    false,
                                    "recovery.verify_rolled_back.mismatch",
                                );
                                self.record_rollback_failure(&transaction, item, &error)?;
                                rollback_failure = true;
                                remaining = remaining.saturating_add(1);
                            }
                            Err(error) => {
                                self.record_rollback_failure(&transaction, item, &error)?;
                                rollback_failure = true;
                                remaining = remaining.saturating_add(1);
                            }
                        }
                    }
                    classification @ (RecoveryClassification::Third
                    | RecoveryClassification::Unknown) => {
                        self.journal.record_recovery_item(
                            transaction.transaction_id,
                            item.item_id,
                            classification,
                            item.error_code.as_deref(),
                            None,
                            now_ms(),
                        )?;
                        recovery_failure = true;
                        remaining = remaining.saturating_add(1);
                    }
                }
            }
            let state = if transaction.state == TransactionState::Succeeded {
                TransactionState::Succeeded
            } else if rollback_failure {
                TransactionState::RollbackFailed
            } else if recovery_failure {
                TransactionState::RecoveryRequired
            } else {
                TransactionState::RolledBack
            };
            self.journal.set_transaction_state(
                transaction.transaction_id,
                state,
                matches!(
                    state,
                    TransactionState::RolledBack | TransactionState::Succeeded
                ),
                now_ms(),
            )?;
        }
        self.journal.checkpoint()?;
        Ok(ReconcileResult {
            status: if remaining == 0 {
                "recovered".to_owned()
            } else {
                "recovery_required".to_owned()
            },
            recovered_count: recovered,
            remaining_count: remaining,
            message: if remaining == 0 {
                "未完了の変更を確認し、変更直前へ戻しました。".to_owned()
            } else {
                "自動で上書きできない項目があります。タイムラインを確認してください。".to_owned()
            },
        })
    }

    fn record_rollback_failure(
        &self,
        transaction: &crate::journal::RecoveryTransaction,
        item: &crate::journal::PersistedItem,
        error: &ActionError,
    ) -> CoreResult<()> {
        self.journal.record_recovery_item(
            transaction.transaction_id,
            item.item_id,
            RecoveryClassification::Applied,
            item.error_code.as_deref(),
            Some(error),
            now_ms(),
        )?;
        Ok(())
    }
}
