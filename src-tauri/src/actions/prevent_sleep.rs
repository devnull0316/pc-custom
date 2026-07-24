use uuid::Uuid;

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionMetadata, ActionParameters, ActionResult, ActionRiskLevel, ActionStage,
        AppliedEvidence, ChangeExplanation, DetectedState, MethodClass, ObservedValue,
        RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{
        BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, SleepLeaseBackup,
    },
    windows::sleep_lease_manager,
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup,
    validate_backup_for_apply, validate_base,
};

pub struct PreventSleepAction;
pub static PREVENT_SLEEP_ACTION: PreventSleepAction = PreventSleepAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SessionPreventSleep,
    name: "このモード中は自動スリープを防ぐ",
    description: "専用スレッドの公開Windows API要求をleaseとして保持します。画面の常時点灯は既定で無効です。",
    category: "focus",
    tags: &["session", "sleep", "game"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
    ],
    minimumBuild: 26_100,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Session,
    parameter_schema: r#"{"keep_display_on":"boolean(default=false)"}"#,
    resource_keys: &["session:execution-state:system-required"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate",
    ],
    compatibility_key: "session.prevent_sleep.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低。公開APIの実機スモークを更新後に再実施します。",
};

impl PreventSleepAction {
    fn keep_display_on(parameters: &ActionParameters) -> ActionResult<bool> {
        match parameters {
            ActionParameters::SessionPreventSleep { keep_display_on } => Ok(*keep_display_on),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn state_for_owner(
        context: &ActionContext<'_>,
        owner: Uuid,
    ) -> ActionResult<DetectedState> {
        let snapshot = sleep_lease_manager()
            .and_then(|manager| manager.snapshot_for(owner))
            .map_err(|error| {
                map_windows_error(
                    ActionStage::Detect,
                    "action.prevent_sleep.detect_failed",
                    error,
                )
            })?;
        Ok(DetectedState::Known {
            value: ObservedValue::SleepLease {
                owned: snapshot.requested_owner_active,
                owner_count: snapshot.owner_count,
                keep_display_on: snapshot.keep_display_on,
            },
            evidence: evidence(context, "SetThreadExecutionState lease worker"),
        })
    }
}

impl Action for PreventSleepAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let _ = Self::keep_display_on(parameters)?;
        Self::state_for_owner(context, context.item_id)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(
            &METADATA,
            context,
            parameters,
            true,
            ActionStage::Validate,
        )?;
        let _ = Self::keep_display_on(parameters)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let keep_display_on = Self::keep_display_on(parameters)?;
        let before = Self::state_for_owner(context, context.item_id)?;
        let intended = Fingerprint::of_parts([
            context.item_id.as_bytes().as_slice(),
            &[u8::from(keep_display_on)],
        ]);
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint_state(&before, ActionStage::Backup)?,
            intended_fingerprint: intended,
            payload: BackupPayload::SleepLease(SleepLeaseBackup {
                owner_id: context.item_id,
                keep_display_on,
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        self.validate(context, parameters)?;
        validate_backup_for_apply(&METADATA, context, backup)?;
        let keep_display_on = Self::keep_display_on(parameters)?;
        let BackupPayload::SleepLease(lease) = &backup.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.prevent_sleep.backup_kind_mismatch",
            ));
        };
        if lease.owner_id != context.item_id || lease.keep_display_on != keep_display_on {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.prevent_sleep.backup_parameter_mismatch",
            ));
        }
        sleep_lease_manager()
            .and_then(|manager| manager.acquire(lease.owner_id, keep_display_on))
            .map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.prevent_sleep.apply_failed",
                    error,
                )
            })?;
        let state = Self::state_for_owner(context, lease.owner_id)?;
        Ok(AppliedEvidence {
            applied_fingerprint: fingerprint_state(&state, ActionStage::Apply)?,
            state,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, backup, ActionStage::VerifyApplied)?;
        let keep_display_on = Self::keep_display_on(parameters)?;
        let BackupPayload::SleepLease(lease) = &backup.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::VerifyApplied,
                "action.prevent_sleep.backup_kind_mismatch",
            ));
        };
        let observed = Self::state_for_owner(context, lease.owner_id)?;
        let verified = matches!(
            observed.known_value(),
            Some(ObservedValue::SleepLease {
                owned: true,
                keep_display_on: observed_display,
                ..
            }) if !keep_display_on || *observed_display
        );
        Ok(Verification { verified, observed })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(
            &METADATA,
            context,
            parameters,
            true,
            ActionStage::Rollback,
        )?;
        validate_backup(&METADATA, context, backup, ActionStage::Rollback)?;
        let BackupPayload::SleepLease(lease) = &backup.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.prevent_sleep.backup_kind_mismatch",
            ));
        };
        sleep_lease_manager()
            .and_then(|manager| manager.release(lease.owner_id))
            .map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.prevent_sleep.rollback_failed",
                    error,
                )
            })?;
        let state = Self::state_for_owner(context, lease.owner_id)?;
        Ok(RollbackEvidence {
            restored_fingerprint: fingerprint_state(&state, ActionStage::Rollback)?,
            state,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        _parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, backup, ActionStage::VerifyRolledBack)?;
        let BackupPayload::SleepLease(lease) = &backup.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::VerifyRolledBack,
                "action.prevent_sleep.backup_kind_mismatch",
            ));
        };
        let observed = Self::state_for_owner(context, lease.owner_id)?;
        let verified = matches!(
            observed.known_value(),
            Some(ObservedValue::SleepLease { owned: false, .. })
        );
        Ok(Verification { verified, observed })
    }

    fn explain_changes(
        &self,
        parameters: &ActionParameters,
    ) -> ActionResult<ChangeExplanation> {
        let keep_display_on = Self::keep_display_on(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: if keep_display_on {
                "自動スリープと画面消灯を、このleaseの間だけ防ぎます。".to_owned()
            } else {
                "自動スリープだけを、このleaseの間だけ防ぎます。".to_owned()
            },
            method: "SetThreadExecutionState（専用スレッド）".to_owned(),
            resources: METADATA.resource_keys.iter().map(|v| (*v).to_owned()).collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "最後の所有lease解放時に同じスレッドで要求を解除します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.prevent_sleep.retry_after_power_policy_check",
            opens_official_settings: false,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    struct LeaseCleanup {
        owner_id: Uuid,
        armed: bool,
    }

    impl Drop for LeaseCleanup {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            match sleep_lease_manager().and_then(|manager| manager.release(self.owner_id)) {
                Ok(()) => {}
                Err(error) => {
                    eprintln!("emergency sleep-lease cleanup failed: {error}");
                }
            }
        }
    }

    #[test]
    fn apply_detect_rollback_detect_uses_real_worker_thread() {
        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let parameters = ActionParameters::SessionPreventSleep {
            keep_display_on: false,
        };
        let draft = PREVENT_SLEEP_ACTION
            .create_backup(&context, &parameters)
            .expect("create real session backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let mut cleanup = LeaseCleanup {
            owner_id: item_id,
            armed: true,
        };
        let applied = PREVENT_SLEEP_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("acquire real SetThreadExecutionState lease");
        envelope.record_applied(applied.applied_fingerprint);
        let detected = PREVENT_SLEEP_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect owned lease");
        assert!(matches!(
            detected.known_value(),
            Some(ObservedValue::SleepLease { owned: true, .. })
        ));

        PREVENT_SLEEP_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("release real SetThreadExecutionState lease");
        let restored = PREVENT_SLEEP_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect released lease");
        cleanup.armed = false;
        assert!(matches!(
            restored.known_value(),
            Some(ObservedValue::SleepLease { owned: false, .. })
        ));
    }
}
