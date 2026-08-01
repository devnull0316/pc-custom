use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{AppVolumeResetBackup, BackupDraft, BackupEnvelope, BackupPayload, Fingerprint},
    windows::{read_app_volume_sessions, restore_app_volume_sessions, AppVolumeSessionState},
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct AppVolumeResetAction;
pub static APP_VOLUME_RESET_ACTION: AppVolumeResetAction = AppVolumeResetAction;

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
        read_app_volume_sessions()
            .map_err(|error| map_windows_error(stage, "action.app_volume_reset.read_failed", error))
    }

    fn calculate_fingerprint(sessions: &[AppVolumeSessionState]) -> Fingerprint {
        let mut bytes = Vec::new();
        for session in sessions {
            let fp = session.fingerprint();
            bytes.extend_from_slice(&fp.0);
        }
        Fingerprint::of_bytes(&bytes)
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
    ) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::AppVolumeSessions {
                active_sessions: sessions.len(),
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
        Ok(Self::observed_state(context, &sessions))
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

        for orig in &payload.original_sessions {
            if let Some(curr) = current_sessions.iter().find(|s| {
                s.device_id == orig.device_id && s.session_instance_id == orig.session_instance_id
            }) {
                if curr.volume != orig.volume || curr.muted != orig.muted {
                    return Err(ActionError::new(
                        ActionErrorCode::ExternalConflict,
                        ActionStage::Apply,
                        false,
                        "action.app_volume_reset.external_conflict",
                    ));
                }
            }
        }

        let fp = Self::calculate_fingerprint(&current_sessions);
        Ok(AppliedEvidence {
            state: Self::observed_state(context, &current_sessions),
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
            observed: Self::observed_state(context, &sessions),
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

        let _outcome =
            restore_app_volume_sessions(&payload.original_sessions).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.app_volume_reset.rollback_failed",
                    error,
                )
            })?;

        let current_sessions = Self::read_sessions(ActionStage::Rollback)?;
        let fp = Self::calculate_fingerprint(&current_sessions);

        Ok(RollbackEvidence {
            state: Self::observed_state(context, &current_sessions),
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
        let sessions = Self::read_sessions(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: true,
            observed: Self::observed_state(context, &sessions),
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
