use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionMetadata, ActionParameters, ActionResult, ActionRiskLevel, ActionStage,
        AppliedEvidence, ChangeExplanation, DetectedState, MethodClass, ObservedValue,
        RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, ObservationBackup},
    windows::active_power_scheme_guid,
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup,
    validate_backup_for_apply, validate_base,
};

pub struct ActiveSchemeCheckAction;
pub static ACTIVE_SCHEME_CHECK_ACTION: ActiveSchemeCheckAction = ActiveSchemeCheckAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::PowerActiveSchemeCheck,
    name: "現在の電源プランを確認する",
    description: "公開Power APIでactive schemeを読み取るだけで、設定は変更しません。",
    category: "readiness",
    tags: &["power", "read-only", "game"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 28_000,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &["power:active-scheme:observation"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/powrprof/nf-powrprof-powergetactivescheme",
    ],
    compatibility_key: "power.active_scheme_check.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低。読み取りAPIの利用可否のみ再確認します。",
};

impl ActiveSchemeCheckAction {
    fn ensure_parameters(parameters: &ActionParameters) -> ActionResult<()> {
        if !matches!(parameters, ActionParameters::PowerActiveSchemeCheck {}) {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            ));
        }
        Ok(())
    }
}

impl Action for ActiveSchemeCheckAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        Self::ensure_parameters(parameters)?;
        let guid = active_power_scheme_guid().map_err(|error| {
            map_windows_error(
                ActionStage::Detect,
                "action.active_scheme.detect_failed",
                error,
            )
        })?;
        Ok(DetectedState::Known {
            value: ObservedValue::ActivePowerScheme { guid },
            evidence: evidence(context, "PowerGetActiveScheme"),
        })
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
            false,
            ActionStage::Validate,
        )?;
        Self::ensure_parameters(parameters)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let state = self.detect_current_state(context, parameters)?;
        let fingerprint = fingerprint_state(&state, ActionStage::Backup)?;
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint,
            intended_fingerprint: fingerprint,
            payload: BackupPayload::Observation(ObservationBackup {
                source: "PowerGetActiveScheme".to_owned(),
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
        if !matches!(backup.payload, BackupPayload::Observation(_)) {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.active_scheme.backup_kind_mismatch",
            ));
        }
        let state = self.detect_current_state(context, parameters)?;
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
        let observed = self.detect_current_state(context, parameters)?;
        Ok(Verification {
            verified: matches!(
                observed.known_value(),
                Some(ObservedValue::ActivePowerScheme { .. })
            ),
            observed,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        self.validate(context, parameters)?;
        validate_backup(&METADATA, context, backup, ActionStage::Rollback)?;
        let state = self.detect_current_state(context, parameters)?;
        Ok(RollbackEvidence {
            restored_fingerprint: fingerprint_state(&state, ActionStage::Rollback)?,
            state,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, backup, ActionStage::VerifyRolledBack)?;
        let observed = self.detect_current_state(context, parameters)?;
        Ok(Verification {
            verified: matches!(
                observed.known_value(),
                Some(ObservedValue::ActivePowerScheme { .. })
            ),
            observed,
        })
    }

    fn explain_changes(
        &self,
        parameters: &ActionParameters,
    ) -> ActionResult<ChangeExplanation> {
        Self::ensure_parameters(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "現在のactive power schemeを表示します。OS設定は変更しません。".to_owned(),
            method: "PowerGetActiveScheme（読み取り専用）".to_owned(),
            resources: METADATA.resource_keys.iter().map(|v| (*v).to_owned()).collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "変更がないためrollbackはno-opです。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.active_scheme.use_windows_power_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    #[test]
    fn apply_detect_rollback_detect_is_read_only() {
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
        let parameters = ActionParameters::PowerActiveSchemeCheck {};
        let before = ACTIVE_SCHEME_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("read active power scheme before round trip");
        let draft = ACTIVE_SCHEME_CHECK_ACTION
            .create_backup(&context, &parameters)
            .expect("create observation backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let applied = ACTIVE_SCHEME_CHECK_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("perform read-only apply");
        envelope.record_applied(applied.applied_fingerprint);
        let detected = ACTIVE_SCHEME_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect active power scheme after apply");
        assert!(matches!(
            detected.known_value(),
            Some(ObservedValue::ActivePowerScheme { .. })
        ));

        ACTIVE_SCHEME_CHECK_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("perform read-only rollback");
        let after = ACTIVE_SCHEME_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect active power scheme after rollback");
        assert_eq!(before.known_value(), after.known_value());
        assert!(ACTIVE_SCHEME_CHECK_ACTION
            .verify_rolled_back(&context, &parameters, &envelope)
            .expect("verify read-only rollback")
            .verified);
    }
}
