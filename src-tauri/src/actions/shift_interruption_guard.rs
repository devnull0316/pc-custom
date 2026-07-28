use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{
        BackupDraft, BackupEnvelope, BackupPayload, KeyboardAccessibilitySettings,
        ShiftInterruptionGuardBackup,
    },
    windows::{
        filter_confirmation_is_enabled, filter_feature_is_enabled, filter_shortcut_is_enabled,
        read_keyboard_accessibility_settings, replace_keyboard_accessibility_settings,
        sticky_confirmation_is_enabled, sticky_feature_is_enabled, sticky_shortcut_is_enabled,
        sticky_transient_state_is_active, without_shift_shortcuts,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct ShiftInterruptionGuardAction;
pub static SHIFT_INTERRUPTION_GUARD_ACTION: ShiftInterruptionGuardAction =
    ShiftInterruptionGuardAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::InputShiftInterruptionGuard,
    name: "ゲーム中のShift確認画面を止める",
    description: "Shiftを5回押したときや右Shiftを長押ししたときに出る確認画面を、このモードの間だけ出さないようにします。入力の補助機能そのものは無効にしません。",
    category: "games",
    tags: &["ゲーム", "Shift", "一時的"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
    ],
    minimumBuild: 26_100,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Caution,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Persistent,
    parameter_schema: "{}",
    resource_keys: &["windows:user-input:shift-accessibility-shortcuts"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/dxtecharts/disabling-shortcut-keys-in-games",
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfow",
    ],
    compatibility_key: "input.shift_interruption_guard.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShiftGuardTransactionState {
    Original,
    Desired,
    MixedOwned,
    Third,
}

impl ShiftInterruptionGuardAction {
    fn validate_parameters(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<()> {
        if matches!(parameters, ActionParameters::InputShiftInterruptionGuard {}) {
            Ok(())
        } else {
            Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                stage,
                false,
                "action.parameters.id_mismatch",
            ))
        }
    }

    fn read_settings(stage: ActionStage) -> ActionResult<KeyboardAccessibilitySettings> {
        read_keyboard_accessibility_settings().map_err(|error| {
            map_windows_error(stage, "action.shift_interruption_guard.read_failed", error)
        })
    }

    fn ensure_safe_to_change(
        settings: KeyboardAccessibilitySettings,
        stage: ActionStage,
    ) -> ActionResult<()> {
        if sticky_feature_is_enabled(settings) || filter_feature_is_enabled(settings) {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.shift_interruption_guard.input_assistance_in_use",
            ));
        }
        if sticky_transient_state_is_active(settings) {
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                stage,
                false,
                "action.shift_interruption_guard.transient_input_state",
            ));
        }
        Ok(())
    }

    fn observed_state(
        context: &ActionContext<'_>,
        settings: KeyboardAccessibilitySettings,
    ) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::ShiftInterruptionGuard {
                shift_five_press_shortcut_enabled: sticky_shortcut_is_enabled(settings),
                shift_five_press_confirmation_enabled: sticky_confirmation_is_enabled(settings),
                right_shift_hold_shortcut_enabled: filter_shortcut_is_enabled(settings),
                right_shift_hold_confirmation_enabled: filter_confirmation_is_enabled(settings),
                input_assistance_in_use: sticky_feature_is_enabled(settings)
                    || filter_feature_is_enabled(settings),
            },
            evidence: evidence(
                context,
                "SystemParametersInfoW keyboard accessibility structures",
            ),
        }
    }

    fn payload(
        envelope: &BackupEnvelope,
        stage: ActionStage,
    ) -> ActionResult<&ShiftInterruptionGuardBackup> {
        let BackupPayload::ShiftInterruptionGuard(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.shift_interruption_guard.backup_kind_mismatch",
            ));
        };
        if payload.intended != without_shift_shortcuts(payload.original)
            || sticky_feature_is_enabled(payload.original)
            || filter_feature_is_enabled(payload.original)
            || sticky_transient_state_is_active(payload.original)
        {
            return Err(ActionError::recovery_required(
                stage,
                "action.shift_interruption_guard.backup_contract_mismatch",
            ));
        }
        Ok(payload)
    }

    fn components_match(
        current: KeyboardAccessibilitySettings,
        expected: KeyboardAccessibilitySettings,
    ) -> (bool, bool) {
        (
            current.sticky_size == expected.sticky_size
                && current.sticky_flags == expected.sticky_flags,
            current.filter_size == expected.filter_size
                && current.filter_flags == expected.filter_flags
                && current.filter_wait_ms == expected.filter_wait_ms
                && current.filter_delay_ms == expected.filter_delay_ms
                && current.filter_repeat_ms == expected.filter_repeat_ms
                && current.filter_bounce_ms == expected.filter_bounce_ms,
        )
    }

    fn classify_settings(
        current: KeyboardAccessibilitySettings,
        backup: &ShiftInterruptionGuardBackup,
    ) -> ShiftGuardTransactionState {
        if current == backup.original {
            return ShiftGuardTransactionState::Original;
        }
        if current == backup.intended {
            return ShiftGuardTransactionState::Desired;
        }
        let (sticky_original, filter_original) = Self::components_match(current, backup.original);
        let (sticky_intended, filter_intended) = Self::components_match(current, backup.intended);
        if (sticky_original || sticky_intended) && (filter_original || filter_intended) {
            ShiftGuardTransactionState::MixedOwned
        } else {
            ShiftGuardTransactionState::Third
        }
    }

    fn rollback_with_recovery_policy(
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
        allow_recorded_mixed_rollback: bool,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Rollback)?;
        Self::validate_parameters(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;
        let current = Self::read_settings(ActionStage::Rollback)?;
        match Self::classify_settings(current, payload) {
            ShiftGuardTransactionState::Original => {}
            ShiftGuardTransactionState::Desired => {
                replace_keyboard_accessibility_settings(current, payload.original).map_err(
                    |error| {
                        map_windows_error(
                            ActionStage::Rollback,
                            "action.shift_interruption_guard.rollback_failed",
                            error,
                        )
                    },
                )?;
            }
            ShiftGuardTransactionState::MixedOwned
                if envelope.applied_fingerprint.is_none() || allow_recorded_mixed_rollback =>
            {
                replace_keyboard_accessibility_settings(current, payload.original).map_err(
                    |error| {
                        map_windows_error(
                            ActionStage::Rollback,
                            "action.shift_interruption_guard.rollback_failed",
                            error,
                        )
                    },
                )?;
            }
            ShiftGuardTransactionState::MixedOwned | ShiftGuardTransactionState::Third => {
                return Err(ActionError::new(
                    ActionErrorCode::ExternalConflict,
                    ActionStage::Rollback,
                    false,
                    "action.rollback.external_change_detected",
                ));
            }
        }
        let restored = Self::read_settings(ActionStage::Rollback)?;
        Ok(RollbackEvidence {
            state: Self::observed_state(context, restored),
            restored_fingerprint: restored.fingerprint(),
        })
    }
}

pub(crate) fn classify_recoverable_shift_guard(
    context: &ActionContext<'_>,
    backup: &BackupEnvelope,
) -> ActionResult<ShiftGuardTransactionState> {
    validate_backup(&METADATA, context, backup, ActionStage::Recovery)?;
    let payload = ShiftInterruptionGuardAction::payload(backup, ActionStage::Recovery)?;
    let current = ShiftInterruptionGuardAction::read_settings(ActionStage::Recovery)?;
    Ok(ShiftInterruptionGuardAction::classify_settings(
        current, payload,
    ))
}

pub(crate) fn rollback_recoverable_shift_guard(
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    backup: &BackupEnvelope,
) -> ActionResult<RollbackEvidence> {
    ShiftInterruptionGuardAction::rollback_with_recovery_policy(context, parameters, backup, true)
}

impl Action for ShiftInterruptionGuardAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        Self::validate_parameters(parameters, ActionStage::Detect)?;
        Ok(Self::observed_state(
            context,
            Self::read_settings(ActionStage::Detect)?,
        ))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Self::ensure_safe_to_change(
            Self::read_settings(ActionStage::Validate)?,
            ActionStage::Validate,
        )?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        Self::validate_parameters(parameters, ActionStage::Backup)?;
        let original = Self::read_settings(ActionStage::Backup)?;
        Self::ensure_safe_to_change(original, ActionStage::Backup)?;
        let intended = without_shift_shortcuts(original);
        Ok(BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: intended.fingerprint(),
            payload: BackupPayload::ShiftInterruptionGuard(ShiftInterruptionGuardBackup {
                original,
                intended,
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Apply)?;
        Self::validate_parameters(parameters, ActionStage::Apply)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let payload = Self::payload(envelope, ActionStage::Apply)?;
        let current = Self::read_settings(ActionStage::Apply)?;
        Self::ensure_safe_to_change(current, ActionStage::Apply)?;
        if current != payload.original {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Apply,
                false,
                "action.apply.stale_preview",
            ));
        }
        replace_keyboard_accessibility_settings(payload.original, payload.intended).map_err(
            |error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.shift_interruption_guard.apply_failed",
                    error,
                )
            },
        )?;
        let applied = Self::read_settings(ActionStage::Apply)?;
        Ok(AppliedEvidence {
            state: Self::observed_state(context, applied),
            applied_fingerprint: applied.fingerprint(),
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        Self::validate_parameters(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        let current = Self::read_settings(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: current == payload.intended,
            observed: Self::observed_state(context, current),
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        Self::rollback_with_recovery_policy(context, parameters, envelope, false)
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        Self::validate_parameters(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        let current = Self::read_settings(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current == payload.original,
            observed: Self::observed_state(context, current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "Shiftを5回押したときと、右Shiftを長押ししたときの確認画面を、このモードの間だけ出さないようにします。".to_owned(),
            method: "Windowsが公開している入力設定の機能".to_owned(),
            resources: vec!["Shiftの連打と長押しで出る確認画面".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope:
                "入力の補助機能を含む全設定を保存し、変更前と同じ状態へ戻します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.shift_interruption_guard.check_input_assistance",
            opens_official_settings: false,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::{FILTER_SHORTCUT_FLAGS, STICKY_SHORTCUT_FLAGS};
    use uuid::Uuid;

    fn sample() -> KeyboardAccessibilitySettings {
        KeyboardAccessibilitySettings {
            sticky_size: 8,
            sticky_flags: 0x0000_00FE,
            filter_size: 24,
            filter_flags: 0x0000_007E,
            filter_wait_ms: 1_000,
            filter_delay_ms: 500,
            filter_repeat_ms: 300,
            filter_bounce_ms: 0,
        }
    }

    #[test]
    fn metadata_allows_mode_automation_without_admin_or_restart() {
        assert_eq!(METADATA.kind, ActionKind::Persistent);
        assert_eq!(METADATA.riskLevel, ActionRiskLevel::Caution);
        assert!(METADATA.auto_apply_eligible);
        assert!(!METADATA.requiresAdmin);
        assert!(!METADATA.requiresRestart);
        assert!(!METADATA.requiresExplorerRestart);
        METADATA
            .validate_static_contract()
            .expect("metadata contract");
    }

    #[test]
    fn active_input_assistance_is_rejected_without_changing_it() {
        let mut settings = sample();
        settings.sticky_flags |= 1;
        let error =
            ShiftInterruptionGuardAction::ensure_safe_to_change(settings, ActionStage::Apply)
                .expect_err("active input assistance must fail closed");
        assert_eq!(error.code, ActionErrorCode::InvalidParameters);
    }

    #[test]
    fn transient_latch_or_lock_state_is_rejected_before_writing() {
        let mut settings = sample();
        settings.sticky_flags |= 0x0001_0000;
        let error =
            ShiftInterruptionGuardAction::ensure_safe_to_change(settings, ActionStage::Apply)
                .expect_err("transient Sticky Keys state must fail closed");
        assert_eq!(error.code, ActionErrorCode::StateUnknown);
    }

    #[test]
    fn rollback_classification_distinguishes_owned_partial_state_from_third_party_changes() {
        let original = sample();
        let intended = without_shift_shortcuts(original);
        let backup = ShiftInterruptionGuardBackup { original, intended };

        assert_eq!(
            ShiftInterruptionGuardAction::classify_settings(original, &backup),
            ShiftGuardTransactionState::Original
        );
        assert_eq!(
            ShiftInterruptionGuardAction::classify_settings(intended, &backup),
            ShiftGuardTransactionState::Desired
        );

        let mut mixed = original;
        mixed.sticky_flags = intended.sticky_flags;
        assert_eq!(
            ShiftInterruptionGuardAction::classify_settings(mixed, &backup),
            ShiftGuardTransactionState::MixedOwned
        );

        let mut third = intended;
        third.filter_wait_ms += 1;
        assert_eq!(
            ShiftInterruptionGuardAction::classify_settings(third, &backup),
            ShiftGuardTransactionState::Third
        );
    }

    #[cfg(windows)]
    struct RealSettingsRestoreGuard {
        original: KeyboardAccessibilitySettings,
        armed: bool,
    }

    #[cfg(windows)]
    impl Drop for RealSettingsRestoreGuard {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            match read_keyboard_accessibility_settings() {
                Ok(current) => {
                    if current != self.original {
                        if let Err(error) =
                            replace_keyboard_accessibility_settings(current, self.original)
                        {
                            eprintln!(
                                "emergency Shift interruption settings cleanup failed: {error}"
                            );
                        }
                    }
                }
                Err(error) => {
                    eprintln!("emergency Shift interruption settings read failed: {error}");
                }
            }
        }
    }

    #[cfg(windows)]
    fn set_sticky_only_for_rollback_crash_simulation(settings: KeyboardAccessibilitySettings) {
        use windows::Win32::UI::{
            Accessibility::{STICKYKEYS, STICKYKEYS_FLAGS},
            WindowsAndMessaging::{
                SystemParametersInfoW, SPI_SETSTICKYKEYS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        };

        let mut sticky = STICKYKEYS {
            cbSize: settings.sticky_size,
            dwFlags: STICKYKEYS_FLAGS(settings.sticky_flags),
        };
        unsafe {
            SystemParametersInfoW(
                SPI_SETSTICKYKEYS,
                settings.sticky_size,
                Some((&mut sticky as *mut STICKYKEYS).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .expect("simulate a crash after the first rollback write");
    }

    /// 実際の利用者設定を一時変更し、公開getterで適用・完全復元を別途観測する。
    #[cfg(windows)]
    #[test]
    #[ignore = "実機のShift連打・長押し確認設定を一時的に変更し、必ず元へ戻す"]
    fn real_machine_shift_interruption_guard_round_trip() {
        use crate::{
            backup::BackupEnvelope, compatibility::OsIdentity, windows::acquire_core_mutation_lock,
        };

        let _mutation_lock = acquire_core_mutation_lock().expect("exclusive core mutation lock");
        let before =
            read_keyboard_accessibility_settings().expect("read original keyboard settings");
        let mut cleanup = RealSettingsRestoreGuard {
            original: before,
            armed: true,
        };
        assert!(
            !sticky_feature_is_enabled(before) && !filter_feature_is_enabled(before),
            "入力の補助機能を使用中の環境では変更しない"
        );
        assert!(
            !sticky_transient_state_is_active(before),
            "一時的なキー状態が残っている環境では変更しない"
        );

        let os = OsIdentity::load().expect("load real Windows identity");
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: os.observed_at_unix_ms,
            is_elevated: false,
        };
        let parameters = ActionParameters::InputShiftInterruptionGuard {};
        let draft = SHIFT_INTERRUPTION_GUARD_ACTION
            .create_backup(&context, &parameters)
            .expect("create complete keyboard settings backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );

        let applied_evidence = SHIFT_INTERRUPTION_GUARD_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("apply Shift interruption guard");
        envelope.record_applied(applied_evidence.applied_fingerprint);

        // Actionが返した値ではなく、Windowsへもう一度問い合わせて観測する。
        let applied =
            read_keyboard_accessibility_settings().expect("read applied keyboard settings");
        assert_eq!(applied.sticky_flags & STICKY_SHORTCUT_FLAGS, 0);
        assert_eq!(applied.filter_flags & FILTER_SHORTCUT_FLAGS, 0);
        assert_eq!(
            before.sticky_flags ^ applied.sticky_flags,
            before.sticky_flags & STICKY_SHORTCUT_FLAGS
        );
        assert_eq!(
            before.filter_flags ^ applied.filter_flags,
            before.filter_flags & FILTER_SHORTCUT_FLAGS
        );
        assert_eq!(applied.sticky_size, before.sticky_size);
        assert_eq!(applied.filter_size, before.filter_size);
        assert_eq!(applied.filter_wait_ms, before.filter_wait_ms);
        assert_eq!(applied.filter_delay_ms, before.filter_delay_ms);
        assert_eq!(applied.filter_repeat_ms, before.filter_repeat_ms);
        assert_eq!(applied.filter_bounce_ms, before.filter_bounce_ms);
        assert!(
            SHIFT_INTERRUPTION_GUARD_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify applied through Windows")
                .verified
        );

        SHIFT_INTERRUPTION_GUARD_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("restore complete keyboard settings");
        // 復元処理が返した値ではなく、Windowsへ三度問い合わせる。
        let restored =
            read_keyboard_accessibility_settings().expect("read restored keyboard settings");
        assert_eq!(restored, before, "全フィールドを変更前へ戻す");
        assert!(
            SHIFT_INTERRUPTION_GUARD_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify rollback through Windows")
                .verified
        );

        // ROLLING_BACK の耐久記録後、1つ目だけ戻して落ちた状態を作り、
        // 再起動時専用経路が第三者変更と誤認せず完全復元できることも確認する。
        let recovery_draft = SHIFT_INTERRUPTION_GUARD_ACTION
            .create_backup(&context, &parameters)
            .expect("create backup for rollback crash recovery");
        let mut recovery_envelope = BackupEnvelope::from_draft(
            recovery_draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );
        let recovery_applied_evidence = SHIFT_INTERRUPTION_GUARD_ACTION
            .apply(&context, &parameters, &recovery_envelope)
            .expect("reapply for rollback crash recovery");
        recovery_envelope.record_applied(recovery_applied_evidence.applied_fingerprint);
        let recovery_applied =
            read_keyboard_accessibility_settings().expect("read recovery applied settings");
        assert_eq!(recovery_applied, applied);

        set_sticky_only_for_rollback_crash_simulation(before);
        let rollback_partial =
            read_keyboard_accessibility_settings().expect("read partial rollback state");
        assert_eq!(
            ShiftInterruptionGuardAction::classify_settings(
                rollback_partial,
                &ShiftInterruptionGuardBackup {
                    original: before,
                    intended: recovery_applied,
                },
            ),
            ShiftGuardTransactionState::MixedOwned
        );
        let ordinary_rollback_error = SHIFT_INTERRUPTION_GUARD_ACTION
            .rollback(&context, &parameters, &recovery_envelope)
            .expect_err("ordinary rollback must reject recorded mixed state");
        assert_eq!(
            ordinary_rollback_error.code,
            ActionErrorCode::ExternalConflict
        );
        assert_eq!(
            read_keyboard_accessibility_settings()
                .expect("ordinary rollback must preserve mixed state"),
            rollback_partial
        );
        rollback_recoverable_shift_guard(&context, &parameters, &recovery_envelope)
            .expect("resume rollback from its owned partial state");
        let recovered =
            read_keyboard_accessibility_settings().expect("read crash-recovered settings");
        assert_eq!(recovered, before);
        cleanup.armed = false;

        println!(
            "EVIDENCE: before={before:?} applied={applied:?} rollback_partial={rollback_partial:?} restored={recovered:?}"
        );
    }
}
