use std::collections::HashSet;

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{AppVolumeResetBackup, BackupDraft, BackupEnvelope, BackupPayload, Fingerprint},
    windows::{
        read_app_volume_sessions, restore_app_volume_sessions, AppVolumeRestoreOutcome,
        AppVolumeSessionState,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct AppVolumeResetAction;
pub static APP_VOLUME_RESET_ACTION: AppVolumeResetAction = AppVolumeResetAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionComparison {
    matching_count: usize,
    missing_count: usize,
    mismatched_count: usize,
}

fn session_identity(session: &AppVolumeSessionState) -> (&[u16], &[u16]) {
    (&session.device_id, &session.session_instance_id)
}

fn ensure_unique_session_identities(
    sessions: &[AppVolumeSessionState],
    stage: ActionStage,
) -> ActionResult<()> {
    let mut identities = HashSet::new();
    if sessions
        .iter()
        .any(|session| !identities.insert(session_identity(session)))
    {
        return Err(ActionError::new(
            if stage == ActionStage::Apply {
                ActionErrorCode::ExternalConflict
            } else {
                ActionErrorCode::RecoveryRequired
            },
            stage,
            false,
            "action.app_volume_reset.ambiguous_session_identity",
        ));
    }
    Ok(())
}

fn compare_saved_sessions(
    saved: &[AppVolumeSessionState],
    current: &[AppVolumeSessionState],
    stage: ActionStage,
) -> ActionResult<SessionComparison> {
    ensure_unique_session_identities(saved, stage)?;
    ensure_unique_session_identities(current, stage)?;

    let mut comparison = SessionComparison {
        matching_count: 0,
        missing_count: 0,
        mismatched_count: 0,
    };
    for expected in saved {
        match current
            .iter()
            .find(|candidate| session_identity(candidate) == session_identity(expected))
        {
            Some(candidate)
                if candidate.volume == expected.volume && candidate.muted == expected.muted =>
            {
                comparison.matching_count += 1;
            }
            Some(_) => comparison.mismatched_count += 1,
            None => comparison.missing_count += 1,
        }
    }
    Ok(comparison)
}

fn restore_outcome_matches(
    saved_count: usize,
    outcome: &AppVolumeRestoreOutcome,
    comparison: SessionComparison,
) -> bool {
    outcome.success_count == comparison.matching_count
        && outcome.missing_count == comparison.missing_count
        && comparison.mismatched_count == 0
        && outcome.success_count + outcome.missing_count == saved_count
}

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::AudioAppVolumeReset,
    name: "アプリごとの音量を一括リセットする",
    description: "配信・会議・ゲームの前にアプリ別の音量とミュート設定を控え、作業終了後にいじる前の音量バランスへ正確に戻します。",
    category: "input",
    tags: &["音量", "ミキサー", "一時的", "復元"],
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
    resource_keys: &["windows:audio:app-volume-mixer"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/audioclient/nn-audioclient-isimpleaudiovolume",
    ],
    compatibility_key: "audio.app_volume_reset.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低",
};

impl AppVolumeResetAction {
    fn validate_parameters(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<()> {
        if matches!(parameters, ActionParameters::AudioAppVolumeReset {}) {
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

    fn read_sessions(stage: ActionStage) -> ActionResult<Vec<AppVolumeSessionState>> {
        let sessions = read_app_volume_sessions().map_err(|error| {
            map_windows_error(stage, "action.app_volume_reset.read_failed", error)
        })?;
        ensure_unique_session_identities(&sessions, stage)?;
        Ok(sessions)
    }

    fn calculate_fingerprint(sessions: &[AppVolumeSessionState]) -> Fingerprint {
        let mut fingerprints: Vec<_> = sessions
            .iter()
            .map(AppVolumeSessionState::fingerprint)
            .collect();
        fingerprints.sort_by_key(|fingerprint| fingerprint.0);
        Fingerprint::of_parts(
            fingerprints
                .iter()
                .map(|fingerprint| fingerprint.0.as_slice()),
        )
    }

    fn payload(
        envelope: &BackupEnvelope,
        stage: ActionStage,
    ) -> ActionResult<&AppVolumeResetBackup> {
        let BackupPayload::AppVolumeReset(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.app_volume_reset.backup_kind_mismatch",
            ));
        };
        Ok(payload)
    }

    fn observed_state(
        context: &ActionContext<'_>,
        sessions: &[AppVolumeSessionState],
        unavailable_saved_sessions: usize,
    ) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::AppVolumeSessions {
                active_sessions: sessions.len(),
                unavailable_saved_sessions,
            },
            evidence: evidence(context, "Core Audio ISimpleAudioVolume session enumerator"),
        }
    }
}

impl Action for AppVolumeResetAction {
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
        let sessions = Self::read_sessions(ActionStage::Detect)?;
        Ok(Self::observed_state(context, &sessions, 0))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Self::read_sessions(ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        Self::validate_parameters(parameters, ActionStage::Backup)?;
        let sessions = Self::read_sessions(ActionStage::Backup)?;
        let fp = Self::calculate_fingerprint(&sessions);
        Ok(BackupDraft {
            precondition_fingerprint: fp,
            intended_fingerprint: fp,
            payload: BackupPayload::AppVolumeReset(AppVolumeResetBackup {
                original_sessions: sessions,
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
        let current_sessions = Self::read_sessions(ActionStage::Apply)?;

        let comparison = compare_saved_sessions(
            &payload.original_sessions,
            &current_sessions,
            ActionStage::Apply,
        )?;
        if comparison.missing_count != 0 || comparison.mismatched_count != 0 {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Apply,
                false,
                "action.app_volume_reset.external_conflict",
            ));
        }

        let fp = Self::calculate_fingerprint(&current_sessions);
        Ok(AppliedEvidence {
            state: Self::observed_state(context, &current_sessions, 0),
            applied_fingerprint: fp,
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
        let sessions = Self::read_sessions(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: true,
            observed: Self::observed_state(context, &sessions, 0),
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Rollback)?;
        Self::validate_parameters(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;

        let outcome = restore_app_volume_sessions(&payload.original_sessions).map_err(|error| {
            map_windows_error(
                ActionStage::Rollback,
                "action.app_volume_reset.rollback_failed",
                error,
            )
        })?;

        let current_sessions = Self::read_sessions(ActionStage::Rollback)?;
        let comparison = compare_saved_sessions(
            &payload.original_sessions,
            &current_sessions,
            ActionStage::Rollback,
        )?;
        if !restore_outcome_matches(payload.original_sessions.len(), &outcome, comparison) {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.app_volume_reset.rollback_unverified",
            ));
        }
        let fp = Self::calculate_fingerprint(&current_sessions);

        Ok(RollbackEvidence {
            state: Self::observed_state(context, &current_sessions, outcome.missing_count),
            restored_fingerprint: fp,
        })
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
        let sessions = Self::read_sessions(ActionStage::VerifyRolledBack)?;
        let comparison = compare_saved_sessions(
            &payload.original_sessions,
            &sessions,
            ActionStage::VerifyRolledBack,
        )?;
        Ok(Verification {
            verified: comparison.mismatched_count == 0,
            observed: Self::observed_state(context, &sessions, comparison.missing_count),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "控えたアプリ別音量とミュート設定を元へ戻します。".to_owned(),
            method: "Windows公開Core Audio ISimpleAudioVolume API".to_owned(),
            resources: vec!["Core Audioアプリ別音量セッション".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "控えたセッションの音量・ミュートを元へ戻します。新しく起動したアプリは触らず、終了したアプリは復元対象外としてカウントします。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.app_volume_reset.check_mixer",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(
        device_id: u16,
        instance_id: u16,
        volume: f32,
        muted: bool,
    ) -> AppVolumeSessionState {
        AppVolumeSessionState {
            device_id: vec![device_id],
            session_instance_id: vec![instance_id],
            volume,
            muted,
        }
    }

    #[test]
    fn saved_sessions_match_by_endpoint_and_session_identity() {
        let saved = vec![session(1, 10, 0.25, false)];
        let wrong_endpoint = vec![session(2, 10, 0.25, false)];

        let comparison =
            compare_saved_sessions(&saved, &wrong_endpoint, ActionStage::VerifyRolledBack)
                .expect("unambiguous comparison");

        assert_eq!(comparison.matching_count, 0);
        assert_eq!(comparison.missing_count, 1);
        assert_eq!(comparison.mismatched_count, 0);
    }

    #[test]
    fn duplicate_session_identity_is_rejected_as_ambiguous() {
        let saved = vec![session(1, 10, 0.25, false)];
        let current = vec![session(1, 10, 0.25, false), session(1, 10, 0.50, true)];

        let error = compare_saved_sessions(&saved, &current, ActionStage::VerifyRolledBack)
            .expect_err("duplicate identities must not be matched with find()");

        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
    }

    #[test]
    fn missing_restore_count_must_agree_with_the_read_back() {
        let comparison = SessionComparison {
            matching_count: 1,
            missing_count: 1,
            mismatched_count: 0,
        };

        assert!(restore_outcome_matches(
            2,
            &AppVolumeRestoreOutcome {
                success_count: 1,
                missing_count: 1,
            },
            comparison,
        ));
        assert!(!restore_outcome_matches(
            2,
            &AppVolumeRestoreOutcome {
                success_count: 1,
                missing_count: 0,
            },
            comparison,
        ));
    }

    #[test]
    fn session_fingerprint_is_order_independent_but_value_sensitive() {
        let first = session(1, 10, 0.25, false);
        let second = session(2, 20, 0.75, true);
        assert_eq!(
            AppVolumeResetAction::calculate_fingerprint(&[first.clone(), second.clone()]),
            AppVolumeResetAction::calculate_fingerprint(&[second.clone(), first.clone()])
        );

        let mut changed = second.clone();
        changed.volume = 0.5;
        assert_ne!(
            AppVolumeResetAction::calculate_fingerprint(&[first.clone(), changed]),
            AppVolumeResetAction::calculate_fingerprint(&[first, second])
        );
    }
}
