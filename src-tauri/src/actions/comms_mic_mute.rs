use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, CommsMicMuteBackup},
    windows::{
        read_comms_mic_mute_by_id, read_default_comms_mic_mute, replace_comms_mic_mute_by_id,
        replace_default_comms_mic_mute, CommsMicMuteState,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

const MAX_ENDPOINT_ID_UTF16: usize = 4_096;

pub struct CommsMicMuteAction;
pub static COMMS_MIC_MUTE_ACTION: CommsMicMuteAction = CommsMicMuteAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::AudioCommsMicMute,
    name: "既定の通話マイクをミュートする",
    description: "Windowsの既定の通話用入力デバイス1台を、このモードの間だけミュート設定にします。別の入力デバイスを選んだアプリや排他モードには効かないことがあります。無音は保証しません。",
    category: "input",
    tags: &["通話", "マイク", "一時的"],
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
    resource_keys: &["windows:audio:communications-capture-mute"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint",
        "https://learn.microsoft.com/windows/win32/api/endpointvolume/nf-endpointvolume-iaudioendpointvolume-getmute",
        "https://learn.microsoft.com/windows/win32/api/endpointvolume/nf-endpointvolume-iaudioendpointvolume-setmute",
        "https://learn.microsoft.com/windows/win32/coreaudio/endpointvolume-api",
    ],
    compatibility_key: "audio.comms_mic_mute.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低",
};

impl CommsMicMuteAction {
    fn validate_parameters(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<()> {
        if matches!(parameters, ActionParameters::AudioCommsMicMute {}) {
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

    fn read_default(stage: ActionStage) -> ActionResult<CommsMicMuteState> {
        read_default_comms_mic_mute().map_err(|error| {
            map_windows_error(stage, "action.comms_mic_mute.read_default_failed", error)
        })
    }

    fn read_saved(endpoint_id: &[u16], stage: ActionStage) -> ActionResult<CommsMicMuteState> {
        read_comms_mic_mute_by_id(endpoint_id).map_err(|error| {
            map_windows_error(stage, "action.comms_mic_mute.read_saved_failed", error)
        })
    }

    fn observed_state(context: &ActionContext<'_>, current: &CommsMicMuteState) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::CommunicationsMicrophone {
                muted: current.muted,
            },
            evidence: evidence(
                context,
                "Core Audio eCommunications capture endpoint GetMute",
            ),
        }
    }

    fn payload(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&CommsMicMuteBackup> {
        let BackupPayload::CommsMicMute(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.comms_mic_mute.backup_kind_mismatch",
            ));
        };
        let id = &payload.original.device_id;
        if id.is_empty()
            || id.len() > MAX_ENDPOINT_ID_UTF16
            || id.contains(&0)
            || payload.intended != payload.original.with_mute(true)
        {
            return Err(ActionError::recovery_required(
                stage,
                "action.comms_mic_mute.backup_contract_mismatch",
            ));
        }
        Ok(payload)
    }
}

impl Action for CommsMicMuteAction {
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
        let current = Self::read_default(ActionStage::Detect)?;
        Ok(Self::observed_state(context, &current))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Self::read_default(ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        Self::validate_parameters(parameters, ActionStage::Backup)?;
        let original = Self::read_default(ActionStage::Backup)?;
        let intended = original.with_mute(true);
        Ok(BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: intended.fingerprint(),
            payload: BackupPayload::CommsMicMute(CommsMicMuteBackup { original, intended }),
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
        let applied = replace_default_comms_mic_mute(&payload.original, true).map_err(|error| {
            map_windows_error(
                ActionStage::Apply,
                "action.comms_mic_mute.apply_failed",
                error,
            )
        })?;
        Ok(AppliedEvidence {
            state: Self::observed_state(context, &applied),
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
        let current = Self::read_saved(&payload.original.device_id, ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: current == payload.intended,
            observed: Self::observed_state(context, &current),
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
        let current = Self::read_saved(&payload.original.device_id, ActionStage::Rollback)?;
        let restored = if current == payload.original {
            current
        } else if payload.original != payload.intended && current == payload.intended {
            replace_comms_mic_mute_by_id(&current, payload.original.muted).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.comms_mic_mute.rollback_failed",
                    error,
                )
            })?
        } else {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        };
        Ok(RollbackEvidence {
            state: Self::observed_state(context, &restored),
            restored_fingerprint: restored.fingerprint(),
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
        let current = Self::read_saved(&payload.original.device_id, ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current == payload.original,
            observed: Self::observed_state(context, &current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::validate_parameters(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "現在の既定の通話用入力デバイス1台を、Windowsの設定値としてミュートにします。別の入力デバイスを使うアプリや排他モードでの無音は保証しません。".to_owned(),
            method: "Windows公開Core Audio APIのEndpointVolume".to_owned(),
            resources: vec!["既定の通話用入力デバイス1台のsoftware mute設定".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "適用前に保存したdevice IDの端末だけを、第三者の変更がない場合に限って元のmute値へ戻します。既定端末が変わっても新しい端末は触りません。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.comms_mic_mute.check_default_input",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: &[u16], muted: bool) -> CommsMicMuteState {
        CommsMicMuteState {
            device_id: id.to_vec(),
            muted,
        }
    }

    #[test]
    fn backup_contract_keeps_one_exact_endpoint_and_true_as_the_intended_value() {
        let original = state(&[1, 2, 3], false);
        let draft = BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: original.with_mute(true).fingerprint(),
            payload: BackupPayload::CommsMicMute(CommsMicMuteBackup {
                original: original.clone(),
                intended: original.with_mute(true),
            }),
        };
        let envelope = BackupEnvelope::from_draft(
            draft,
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            METADATA.id,
            METADATA.action_version,
            0,
            26_200,
        );
        CommsMicMuteAction::payload(&envelope, ActionStage::Rollback)
            .expect("valid exact-endpoint backup");
    }

    #[test]
    fn backup_contract_rejects_a_different_rollback_endpoint() {
        let original = state(&[1], false);
        let draft = BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: state(&[2], true).fingerprint(),
            payload: BackupPayload::CommsMicMute(CommsMicMuteBackup {
                original,
                intended: state(&[2], true),
            }),
        };
        let envelope = BackupEnvelope::from_draft(
            draft,
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            METADATA.id,
            METADATA.action_version,
            0,
            26_200,
        );
        let error = CommsMicMuteAction::payload(&envelope, ActionStage::Rollback)
            .expect_err("different endpoint must fail closed");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
    }

    #[test]
    fn wording_limits_scope_and_does_not_guarantee_silence() {
        let explanation = COMMS_MIC_MUTE_ACTION
            .explain_changes(&ActionParameters::AudioCommsMicMute {})
            .expect("explain");
        assert!(METADATA.description.contains("1台"));
        assert!(METADATA.description.contains("排他モード"));
        assert!(METADATA.description.contains("無音は保証しません"));
        assert!(!METADATA.description.contains("すべてのマイク"));
        assert!(!explanation.result.contains("確実に無音"));
    }

    #[cfg(windows)]
    struct RestoreMicOnDrop {
        original: CommsMicMuteState,
        armed: bool,
    }

    #[cfg(windows)]
    impl Drop for RestoreMicOnDrop {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            let Ok(current) = read_comms_mic_mute_by_id(&self.original.device_id) else {
                return;
            };
            if current == self.original {
                return;
            }
            if current == self.original.with_mute(true) {
                let _ = replace_comms_mic_mute_by_id(&current, self.original.muted);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "実機の既定通話マイクを一時的にミュートし、同じdevice IDだけを元へ戻す"]
    fn real_machine_comms_mic_mute_round_trip() {
        use crate::{compatibility::OsIdentity, windows::acquire_core_mutation_lock};

        let _mutation_lock = acquire_core_mutation_lock().expect("exclusive core mutation lock");
        let before = read_default_comms_mic_mute().expect("read default communications microphone");
        let device_id =
            serde_json::to_string(&String::from_utf16_lossy(&before.device_id)).expect("quote ID");
        println!(
            "EVIDENCE: comms_mic_mute before device_id={device_id} muted={}",
            before.muted
        );
        let mut cleanup = RestoreMicOnDrop {
            original: before.clone(),
            armed: true,
        };

        if before.muted {
            println!(
                "EVIDENCE: comms_mic_mute measured=false reason=already_muted device_id={device_id}"
            );
            cleanup.armed = false;
            return;
        }

        let os = OsIdentity::load().expect("load real Windows identity");
        let transaction_id = uuid::Uuid::new_v4();
        let item_id = uuid::Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: os.observed_at_unix_ms,
            is_elevated: false,
        };
        let parameters = ActionParameters::AudioCommsMicMute {};
        let draft = COMMS_MIC_MUTE_ACTION
            .create_backup(&context, &parameters)
            .expect("save exact device ID and GetMute before apply");
        let BackupPayload::CommsMicMute(saved) = &draft.payload else {
            panic!("communications microphone backup kind");
        };
        assert_eq!(
            saved.original, before,
            "the default endpoint changed before apply; do not mutate either endpoint"
        );
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );

        let applied = COMMS_MIC_MUTE_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("mute default communications microphone");
        envelope.record_applied(applied.applied_fingerprint);

        let after = read_comms_mic_mute_by_id(&before.device_id)
            .expect("re-read the same device ID after apply");
        println!(
            "EVIDENCE: comms_mic_mute applied device_id={device_id} muted={}",
            after.muted
        );
        assert_eq!(after.device_id, before.device_id);
        assert!(after.muted);
        assert!(
            COMMS_MIC_MUTE_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify applied through exact saved device ID")
                .verified
        );

        COMMS_MIC_MUTE_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("restore exact saved communications microphone");
        let restored = read_comms_mic_mute_by_id(&before.device_id)
            .expect("re-read the same device ID after rollback");
        println!(
            "EVIDENCE: comms_mic_mute restored device_id={device_id} muted={}",
            restored.muted
        );
        assert_eq!(restored, before);
        assert!(
            COMMS_MIC_MUTE_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify rollback through exact saved device ID")
                .verified
        );
        cleanup.armed = false;
    }
}
