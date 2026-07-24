use super::*;
use crate::journal::{PreparedItem, TransactionState};

impl TotonoeEngine {
    pub fn commit_preview(&self, preview_token: &str) -> CoreResult<CommitResult> {
        let preview = self
            .previews
            .lock()
            .remove(preview_token)
            .ok_or_else(|| {
                CoreError::invalid_request("プレビューが期限切れか、既に使用されています。")
            })?;
        if preview.expires_at_ms < now_ms() {
            return Err(CoreError::invalid_request(
                "プレビューの期限が切れました。現在状態を確認し直してください。",
            ));
        }
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard = crate::windows::acquire_core_mutation_lock()
            .map_err(core_mutation_lock_error)?;
        let identity = self.identity_for_commit()?;
        if os_identity_fingerprint(&identity)? != preview.os_fingerprint {
            return Err(CoreError::recovery_required(
                "プレビュー後にWindows識別情報が変わったため、書き込みを停止しました。",
            ));
        }
        if self.journal.recovery_count()? > 0 {
            return Err(CoreError::recovery_required(
                "未復元項目を解決するまで、新しい適用は開始できません。",
            ));
        }

        let transaction_id = Uuid::new_v4();
        let mut work = Vec::with_capacity(preview.invocations.len());
        let mut prepared = Vec::with_capacity(preview.invocations.len());
        for (ordinal, parameters) in preview.invocations.iter().enumerate() {
            let action = registered_action(parameters.action_id())?;
            let item_id = Uuid::new_v4();
            let context = action_context(&identity, transaction_id, item_id);
            let validation = action
                .validate(&context, parameters)
                .map_err(CoreError::from)?;
            let current = action
                .detect_current_state(&context, parameters)
                .map_err(CoreError::from)?;
            if preview.before.get(&parameters.action_id()) != Some(&state_fingerprint(&current)?) {
                return Err(CoreError::new(
                    "STALE_PREVIEW",
                    "VALIDATE",
                    false,
                    "確認後に状態が変わりました。差分を確認し直してください。",
                ));
            }
            let draft = action
                .create_backup(&context, parameters)
                .map_err(CoreError::from)?;
            let backup = BackupEnvelope::from_draft(
                draft,
                transaction_id,
                item_id,
                parameters.action_id(),
                action.metadata().action_version,
                now_ms(),
                identity.base_build,
            );
            prepared.push(PreparedItem {
                item_id,
                ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                action_id: parameters.action_id(),
                action_version: action.metadata().action_version,
                parameters: parameters.clone(),
                resource_keys: validation.resource_keys,
                backup: backup.clone(),
            });
            work.push(WorkItem {
                item_id,
                parameters: parameters.clone(),
                backup,
            });
        }

        self.journal.record_prepared_transaction(
            transaction_id,
            "manual preview",
            "manual",
            &os_identity_fingerprint(&identity)?.to_hex(),
            &prepared,
            now_ms(),
        )?;

        let mut applied_indices = Vec::new();
        for index in 0..work.len() {
            let item_id = work[index].item_id;
            self.journal.mark_item_applying(
                transaction_id,
                item_id,
                u32::try_from(index).unwrap_or(u32::MAX),
                now_ms(),
            )?;
            let action = registered_action(work[index].parameters.action_id())?;
            let context = action_context(&identity, transaction_id, item_id);
            let applied = match action.apply(
                &context,
                &work[index].parameters,
                &work[index].backup,
            ) {
                Ok(applied) => applied,
                Err(error) => {
                    self.journal
                        .mark_apply_failure(transaction_id, item_id, &error, now_ms())?;
                    return self.rollback_after_failure(
                        &identity,
                        transaction_id,
                        &mut work,
                        applied_indices,
                        index,
                        error,
                    );
                }
            };
            work[index]
                .backup
                .record_applied(applied.applied_fingerprint);
            match action.verify_applied(
                &context,
                &work[index].parameters,
                &work[index].backup,
            ) {
                Ok(Verification { verified: true, .. }) => {
                    self.journal.mark_item_applied(
                        transaction_id,
                        item_id,
                        &work[index].backup,
                        now_ms(),
                    )?;
                    applied_indices.push(index);
                }
                Ok(_) => {
                    let error = ActionError::new(
                        ActionErrorCode::StateUnknown,
                        ActionStage::VerifyApplied,
                        false,
                        "action.verify_applied.mismatch",
                    );
                    self.journal
                        .mark_apply_failure(transaction_id, item_id, &error, now_ms())?;
                    return self.rollback_after_failure(
                        &identity,
                        transaction_id,
                        &mut work,
                        applied_indices,
                        index,
                        error,
                    );
                }
                Err(error) => {
                    self.journal
                        .mark_apply_failure(transaction_id, item_id, &error, now_ms())?;
                    return self.rollback_after_failure(
                        &identity,
                        transaction_id,
                        &mut work,
                        applied_indices,
                        index,
                        error,
                    );
                }
            }
        }

        self.journal.set_transaction_state(
            transaction_id,
            TransactionState::Succeeded,
            true,
            now_ms(),
        )?;
        Ok(CommitResult {
            transaction_id,
            status: "succeeded".to_owned(),
            message: "全Actionを適用し、現在状態を再確認しました。".to_owned(),
        })
    }

    fn rollback_after_failure(
        &self,
        identity: &OsIdentity,
        transaction_id: Uuid,
        work: &mut [WorkItem],
        mut applied_indices: Vec<usize>,
        failed_index: usize,
        original_error: ActionError,
    ) -> CoreResult<CommitResult> {
        let failed_action = registered_action(work[failed_index].parameters.action_id())?;
        let failed_context = action_context(
            identity,
            transaction_id,
            work[failed_index].item_id,
        );
        let failed_classification = classify_action(
            failed_action,
            &failed_context,
            &work[failed_index].parameters,
            &work[failed_index].backup,
        );
        let primary_code = original_error.code.as_code().to_owned();
        let mut recovery_required = false;
        if failed_classification == RecoveryClassification::Original {
            self.journal.mark_item_rolled_back(
                transaction_id,
                work[failed_index].item_id,
                now_ms(),
            )?;
        } else if failed_classification == RecoveryClassification::Applied {

            applied_indices.push(failed_index);
        } else if matches!(
            failed_classification,
            RecoveryClassification::Third | RecoveryClassification::Unknown
        ) {
            self.journal.record_recovery_item(
                transaction_id,
                work[failed_index].item_id,
                failed_classification,
                Some(&primary_code),
                None,
                now_ms(),
            )?;
            recovery_required = true;
        }

        // Items after the failure were durably prepared but never dispatched to
        // an OS primitive. Terminalize them without issuing any rollback write.
        for item in work.iter().skip(failed_index.saturating_add(1)) {
            self.journal
                .mark_item_rolled_back(transaction_id, item.item_id, now_ms())?;
        }



        let mut rollback_failed = false;
        for index in applied_indices.into_iter().rev() {
            let item = &work[index];
            let action = registered_action(item.parameters.action_id())?;
            let context = action_context(identity, transaction_id, item.item_id);
            if ensure_backup_mutation_allowed(identity, action, &item.backup).is_err() {
                self.journal.record_recovery_item(
                    transaction_id,
                    item.item_id,
                    RecoveryClassification::Unknown,
                    Some(&primary_code),
                    None,
                    now_ms(),
                )?;
                recovery_required = true;
                continue;
            }
            self.journal
                .mark_item_rolling_back(transaction_id, item.item_id, now_ms())?;
            let verification = action
                .rollback(&context, &item.parameters, &item.backup)
                .and_then(|_| {
                    action.verify_rolled_back(&context, &item.parameters, &item.backup)
                });
            match verification {
                Ok(Verification { verified: true, .. }) => {
                    self.journal.mark_item_rolled_back(
                        transaction_id,
                        item.item_id,
                        now_ms(),
                    )?;
                }
                Ok(_) => {
                    let rollback_error = ActionError::new(
                        ActionErrorCode::RecoveryRequired,
                        ActionStage::VerifyRolledBack,
                        false,
                        "rollback.verify.mismatch",
                    );
                    self.journal.record_recovery_item(
                        transaction_id,
                        item.item_id,
                        RecoveryClassification::Applied,
                        Some(&primary_code),
                        Some(&rollback_error),
                        now_ms(),
                    )?;
                    rollback_failed = true;
                }
                Err(rollback_error) => {
                    self.journal.record_recovery_item(
                        transaction_id,
                        item.item_id,
                        RecoveryClassification::Applied,
                        Some(&primary_code),
                        Some(&rollback_error),
                        now_ms(),
                    )?;
                    rollback_failed = true;
                }
            }
        }

        let state = if rollback_failed {
            TransactionState::RollbackFailed
        } else if recovery_required {
            TransactionState::RecoveryRequired
        } else {
            TransactionState::RolledBack
        };
        self.journal.set_transaction_state(
            transaction_id,
            state,
            state == TransactionState::RolledBack,
            now_ms(),
        )?;
        Ok(CommitResult {
            transaction_id,
            status: if state == TransactionState::RolledBack {
                "rolled_back".to_owned()
            } else {
                "recovery_required".to_owned()
            },
            message: if state == TransactionState::RolledBack {
                "適用失敗のため、変更済み項目を逆順で元へ戻しました。".to_owned()
            } else {
                "自動で戻せない項目があります。タイムラインを確認してください。".to_owned()
            },
        })
    }
}

