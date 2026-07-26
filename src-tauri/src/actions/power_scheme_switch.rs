//! Explicit-only switching between Windows' three documented power-scheme personalities.
//!
//! The request contains a closed enum, never a GUID. The exact pre-state is read
//! with PowerGetActiveScheme and restored with PowerSetActiveScheme only while
//! the active scheme still equals the value applied by this Action.

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, PowerScheme,
        RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, PowerSchemeBackup, PowerSchemeGuid},
    windows::{active_power_scheme, set_active_power_scheme},
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup, validate_backup_for_apply,
    validate_base,
};

pub struct PowerSchemeSwitchAction;
pub static POWER_SCHEME_SWITCH_ACTION: PowerSchemeSwitchAction = PowerSchemeSwitchAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::PowerActiveSchemeSwitch,
    name: "電源プランを明示的に切り替える",
    description: "Windows公開Power APIで、バランス・省電力・高パフォーマンスのいずれかを現在ユーザーのactive schemeに設定します。FPS向上は保証しません。",
    category: "power",
    tags: &["power", "public-api", "explicit-only"],
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
    parameter_schema: r#"{"scheme":"balanced|power_saver|high_performance"}"#,
    resource_keys: &["power:active-scheme"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/powersetting/nf-powersetting-powergetactivescheme",
        "https://learn.microsoft.com/windows/win32/api/powersetting/nf-powersetting-powersetactivescheme",
        "https://learn.microsoft.com/windows/win32/power/power-setting-guids",
    ],
    compatibility_key: "power.active_scheme_switch.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開Power APIと固定scheme GUIDの往復を対象buildで再確認します。",
};

impl PowerSchemeSwitchAction {
    fn scheme(parameters: &ActionParameters) -> ActionResult<PowerScheme> {
        match parameters {
            ActionParameters::PowerActiveSchemeSwitch { scheme } => Ok(*scheme),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn state_for_guid(context: &ActionContext<'_>, guid: PowerSchemeGuid) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::ActivePowerScheme {
                guid: guid.canonical_string(),
            },
            evidence: evidence(context, "PowerGetActiveScheme"),
        }
    }

    fn read_state(
        context: &ActionContext<'_>,
        stage: ActionStage,
    ) -> ActionResult<(PowerSchemeGuid, DetectedState)> {
        let guid = active_power_scheme().map_err(|error| {
            map_windows_error(stage, "action.power_scheme.detect_failed", error)
        })?;
        Ok((guid, Self::state_for_guid(context, guid)))
    }

    fn payload<'a>(
        parameters: &ActionParameters,
        envelope: &'a BackupEnvelope,
        stage: ActionStage,
    ) -> ActionResult<&'a PowerSchemeBackup> {
        let scheme = Self::scheme(parameters)?;
        let BackupPayload::PowerScheme(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.power_scheme.backup_kind_mismatch",
            ));
        };
        let expected_guid = PowerSchemeGuid::for_scheme(scheme);
        if payload.intended_scheme != scheme || payload.intended_guid != expected_guid {
            return Err(ActionError::recovery_required(
                stage,
                "action.power_scheme.backup_parameter_mismatch",
            ));
        }
        Ok(payload)
    }

    fn external_change(stage: ActionStage) -> ActionError {
        ActionError::new(
            ActionErrorCode::ExternalConflict,
            stage,
            false,
            "action.power_scheme.external_change_detected",
        )
    }
}

impl Action for PowerSchemeSwitchAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let _ = Self::scheme(parameters)?;
        Self::read_state(context, ActionStage::Detect).map(|(_, state)| state)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let _ = Self::scheme(parameters)?;
        let _ = Self::read_state(context, ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let scheme = Self::scheme(parameters)?;
        let (original_guid, original_state) = Self::read_state(context, ActionStage::Backup)?;
        let intended_guid = PowerSchemeGuid::for_scheme(scheme);
        let intended_state = Self::state_for_guid(context, intended_guid);
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint_state(&original_state, ActionStage::Backup)?,
            intended_fingerprint: fingerprint_state(&intended_state, ActionStage::Backup)?,
            payload: BackupPayload::PowerScheme(PowerSchemeBackup {
                original_guid,
                intended_scheme: scheme,
                intended_guid,
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        self.validate(context, parameters)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let payload = Self::payload(parameters, envelope, ActionStage::Apply)?;
        let (before, _) = Self::read_state(context, ActionStage::Apply)?;
        if before != payload.original_guid {
            return Err(Self::external_change(ActionStage::Apply));
        }

        let set_error = if before == payload.intended_guid {
            None
        } else {
            set_active_power_scheme(&payload.intended_guid)
                .err()
                .map(|error| {
                    map_windows_error(
                        ActionStage::Apply,
                        "action.power_scheme.apply_failed",
                        error,
                    )
                })
        };
        let (after, state) = Self::read_state(context, ActionStage::Apply).map_err(|_| {
            ActionError::recovery_required(
                ActionStage::Apply,
                "action.power_scheme.applied_state_unknown",
            )
        })?;
        if after == payload.intended_guid {
            return Ok(AppliedEvidence {
                applied_fingerprint: envelope.intended_fingerprint,
                state,
            });
        }
        if after != payload.original_guid {
            return Err(Self::external_change(ActionStage::Apply));
        }
        Err(set_error.unwrap_or_else(|| {
            ActionError::new(
                ActionErrorCode::StateUnknown,
                ActionStage::Apply,
                false,
                "action.power_scheme.apply_verify_mismatch",
            )
        }))
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        let payload = Self::payload(parameters, envelope, ActionStage::VerifyApplied)?;
        let (current, observed) = Self::read_state(context, ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: current == payload.intended_guid,
            observed,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(parameters, envelope, ActionStage::Rollback)?;
        let (before, state) = Self::read_state(context, ActionStage::Rollback)?;
        if before == payload.original_guid {
            return Ok(RollbackEvidence {
                restored_fingerprint: envelope.precondition_fingerprint,
                state,
            });
        }
        if before != payload.intended_guid {
            return Err(Self::external_change(ActionStage::Rollback));
        }

        let set_error = set_active_power_scheme(&payload.original_guid)
            .err()
            .map(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.power_scheme.rollback_failed",
                    error,
                )
            });
        let (after, state) = Self::read_state(context, ActionStage::Rollback).map_err(|_| {
            ActionError::recovery_required(
                ActionStage::Rollback,
                "action.power_scheme.rollback_state_unknown",
            )
        })?;
        if after == payload.original_guid {
            return Ok(RollbackEvidence {
                restored_fingerprint: envelope.precondition_fingerprint,
                state,
            });
        }
        if after != payload.intended_guid {
            return Err(Self::external_change(ActionStage::Rollback));
        }
        Err(set_error.unwrap_or_else(|| {
            ActionError::recovery_required(
                ActionStage::Rollback,
                "action.power_scheme.rollback_verify_mismatch",
            )
        }))
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(parameters, envelope, ActionStage::VerifyRolledBack)?;
        let (current, observed) = Self::read_state(context, ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current == payload.original_guid,
            observed,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let scheme = Self::scheme(parameters)?;
        let result = match scheme {
            PowerScheme::Balanced => "現在ユーザーの電源プランをバランスへ切り替えます。",
            PowerScheme::PowerSaver => "現在ユーザーの電源プランを省電力へ切り替えます。",
            PowerScheme::HighPerformance => {
                "現在ユーザーの電源プランを高パフォーマンスへ切り替えます。消費電力が増える場合があります。"
            }
        };
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: result.to_owned(),
            method: "PowerGetActiveScheme + PowerSetActiveScheme".to_owned(),
            resources: METADATA
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope:
                "適用後のactive schemeがTotonoeの設定値のままの場合だけ、保存した元GUIDへ戻します。"
                    .to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.power_scheme.open_windows_power_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    fn scheme_for_guid(guid: PowerSchemeGuid) -> Option<PowerScheme> {
        [
            PowerScheme::Balanced,
            PowerScheme::PowerSaver,
            PowerScheme::HighPerformance,
        ]
        .into_iter()
        .find(|scheme| PowerSchemeGuid::for_scheme(*scheme) == guid)
    }

    fn context<'a>(os: &'a OsIdentity, transaction_id: Uuid, item_id: Uuid) -> ActionContext<'a> {
        ActionContext {
            os_identity: os,
            transaction_id,
            item_id,
            observed_at_unix_ms: 1,
            is_elevated: false,
        }
    }

    #[test]
    fn current_builtin_scheme_has_no_change_apply_detect_rollback_round_trip() {
        let current = active_power_scheme().expect("read active scheme");
        let Some(scheme) = scheme_for_guid(current) else {
            // OEM schemes are valid pre-states but cannot be requested by the closed enum.
            return;
        };
        let os = OsIdentity::from_test_build(26_200);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);
        let parameters = ActionParameters::PowerActiveSchemeSwitch { scheme };
        let draft = POWER_SCHEME_SWITCH_ACTION
            .create_backup(&context, &parameters)
            .expect("create typed power backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );
        let applied = POWER_SCHEME_SWITCH_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("no-change apply");
        envelope.record_applied(applied.applied_fingerprint);
        assert!(
            POWER_SCHEME_SWITCH_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify apply")
                .verified
        );
        POWER_SCHEME_SWITCH_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("rollback to exact original");
        assert!(
            POWER_SCHEME_SWITCH_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify rollback")
                .verified
        );
        assert_eq!(active_power_scheme().expect("read final scheme"), current);
    }

    #[test]
    fn valid_integrity_backup_with_wrong_intended_guid_is_rejected_without_change() {
        let original = active_power_scheme().expect("read active scheme");
        let os = OsIdentity::from_test_build(26_200);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);
        let parameters = ActionParameters::PowerActiveSchemeSwitch {
            scheme: PowerScheme::Balanced,
        };
        let original_state = PowerSchemeSwitchAction::state_for_guid(&context, original);
        let intended_state =
            PowerSchemeSwitchAction::state_for_guid(&context, PowerSchemeGuid::BALANCED);
        let draft = BackupDraft {
            precondition_fingerprint: fingerprint_state(&original_state, ActionStage::Backup)
                .expect("original fingerprint"),
            intended_fingerprint: fingerprint_state(&intended_state, ActionStage::Backup)
                .expect("intended fingerprint"),
            payload: BackupPayload::PowerScheme(PowerSchemeBackup {
                original_guid: original,
                intended_scheme: PowerScheme::Balanced,
                intended_guid: PowerSchemeGuid::HIGH_PERFORMANCE,
            }),
        };
        let envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );
        let error = POWER_SCHEME_SWITCH_ACTION
            .apply(&context, &parameters, &envelope)
            .expect_err("mismatched typed backup must fail closed");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
        assert_eq!(
            active_power_scheme().expect("read unchanged scheme"),
            original
        );
    }

    #[test]
    #[ignore = "changes the real current-user power scheme briefly; run as an explicit smoke"]
    fn real_power_scheme_switch_round_trip_restores_exact_original() {
        struct RestoreGuard(PowerSchemeGuid);
        impl Drop for RestoreGuard {
            fn drop(&mut self) {
                let _ = set_active_power_scheme(&self.0);
            }
        }

        let original = active_power_scheme().expect("read original scheme");
        let _guard = RestoreGuard(original);
        let candidates = [
            PowerScheme::Balanced,
            PowerScheme::PowerSaver,
            PowerScheme::HighPerformance,
        ]
        .into_iter()
        .filter(|scheme| PowerSchemeGuid::for_scheme(*scheme) != original);
        let os = OsIdentity::from_test_build(26_200);
        for scheme in candidates {
            let transaction_id = Uuid::new_v4();
            let item_id = Uuid::new_v4();
            let context = context(&os, transaction_id, item_id);
            let parameters = ActionParameters::PowerActiveSchemeSwitch { scheme };
            let draft = POWER_SCHEME_SWITCH_ACTION
                .create_backup(&context, &parameters)
                .expect("create backup");
            let mut envelope = BackupEnvelope::from_draft(
                draft,
                transaction_id,
                item_id,
                METADATA.id,
                METADATA.action_version,
                1,
                os.base_build,
            );
            let applied = match POWER_SCHEME_SWITCH_ACTION.apply(&context, &parameters, &envelope) {
                Ok(applied) => applied,
                Err(error) => {
                    eprintln!("scheme {scheme:?} unavailable: {error:?}");
                    let expected_environment_rejection = error.code
                        == ActionErrorCode::WindowsApiFailure
                        && error.safe_detail.as_deref().is_some_and(|detail| {
                            detail.ends_with("(OS code 2)") || detail.ends_with("(OS code 5)")
                        });
                    assert!(
                        expected_environment_rejection,
                        "unexpected power-scheme mutation failure: {error:?}"
                    );
                    assert_eq!(
                        active_power_scheme().expect("read after unavailable scheme"),
                        original,
                        "a failed fixed-scheme attempt must leave the original active"
                    );
                    continue;
                }
            };
            envelope.record_applied(applied.applied_fingerprint);
            POWER_SCHEME_SWITCH_ACTION
                .rollback(&context, &parameters, &envelope)
                .expect("restore exact original");
            assert_eq!(
                active_power_scheme().expect("read restored scheme"),
                original
            );
            return;
        }
        eprintln!(
            "power-scheme mutation smoke skipped: every alternative was rejected by the OS; the original scheme remained active"
        );
    }
}
