use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope},
    windows::read_audio_output_observation,
};

use super::common::{evidence, map_windows_error, validate_base};

#[derive(Clone, Copy)]
enum GuidedSetupKind {
    DefaultApps,
    AudioOutput,
}

pub struct GuidedSetupAction {
    metadata: &'static ActionMetadata,
    kind: GuidedSetupKind,
    evidence_source: &'static str,
    result: &'static str,
    method: &'static str,
}

pub static SETUP_DEFAULT_APPS_ACTION: GuidedSetupAction = GuidedSetupAction {
    metadata: &SETUP_DEFAULT_APPS_METADATA,
    kind: GuidedSetupKind::DefaultApps,
    evidence_source: "Windows default-app settings guidance",
    result: "既定のアプリはWindows自身の設定画面で選びます。Windowsは既定の記録に改ざん検出用の符号を付けているため、PCカスタムは記録を直接書き換えません。",
    method: "外から書き換える正規の方法がないため記録は変更せず、決め打ちのURIでWindowsの「既定のアプリ」画面を開く",
};

pub static SETUP_AUDIO_OUTPUT_ACTION: GuidedSetupAction = GuidedSetupAction {
    metadata: &SETUP_AUDIO_OUTPUT_METADATA,
    kind: GuidedSetupKind::AudioOutput,
    evidence_source: "IMMDeviceEnumerator active render endpoints",
    result: "公開Core Audio APIで現在の出力候補と既定デバイスを確認し、変更はWindowsのサウンド設定で行います。",
    method: "Windows公開の機能で読み取るだけ。切り替えは決め打ちのURIでWindowsのサウンド設定を開く",
};

static SETUP_DEFAULT_APPS_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupDefaultApps,
    name: "既定のアプリを確認・設定する",
    description: "Windowsは既定の関連付けの記録に改ざん検出用の符号を付けており、外から書き換える正規の方法もありません。そのため記録には触れず、Windows自身の「既定のアプリ」画面へ案内します。",
    category: "setup",
    tags: &["default-apps", "file-associations", "guided"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Guided,
    parameter_schema: "{}",
    resource_keys: &["windows-settings:defaultapps:guided"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/apps/develop/launch/launch-default-apps-settings",
        "https://learn.microsoft.com/windows-hardware/manufacture/desktop/export-or-import-default-application-associations",
    ],
    compatibility_key: "setup.default_apps.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。固定のWindows設定URIだけを開き、関連付けの保存領域は変更しません。",
};

static SETUP_AUDIO_OUTPUT_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupAudioOutput,
    name: "音声出力デバイスを確認・設定する",
    description: "公開Core Audio APIで有効な出力候補と既定デバイスを読み取り、切り替えはWindows自身のサウンド設定へ案内します。",
    category: "setup",
    tags: &["audio", "output", "guided", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Guided,
    parameter_schema: "{}",
    resource_keys: &[
        "core-audio:render-endpoints:observation",
        "windows-settings:sound:guided",
    ],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immdeviceenumerator",
        "https://learn.microsoft.com/windows/apps/develop/launch/launch-settings-app",
    ],
    compatibility_key: "setup.audio_output.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開Core Audio APIの読み取りと固定のWindows設定URIだけを使用します。",
};

const SETTINGS_TROUBLESHOOTING: &[TroubleshootingStep] = &[TroubleshootingStep {
    message_key: "action.guided_setup.open_windows_settings",
    opens_official_settings: true,
}];

fn ensure_parameters(
    metadata: &'static ActionMetadata,
    parameters: &ActionParameters,
    stage: ActionStage,
) -> ActionResult<()> {
    if parameters.action_id() != metadata.id {
        return Err(ActionError::new(
            ActionErrorCode::WrongParameters,
            stage,
            false,
            "action.parameters.id_mismatch",
        ));
    }
    Ok(())
}

fn guided_mutation_blocked(stage: ActionStage) -> ActionError {
    ActionError::new(
        ActionErrorCode::CompatibilityBlocked,
        stage,
        false,
        "action.guided_setup.windows_settings_required",
    )
}

impl Action for GuidedSetupAction {
    fn metadata(&self) -> &'static ActionMetadata {
        self.metadata
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Detect,
        )?;
        let value = match self.kind {
            GuidedSetupKind::DefaultApps => ObservedValue::NoOsChange,
            GuidedSetupKind::AudioOutput => {
                let observation = read_audio_output_observation().map_err(|error| {
                    map_windows_error(
                        ActionStage::Detect,
                        "action.guided_setup.audio_detect_failed",
                        error,
                    )
                })?;
                ObservedValue::AudioOutput(observation)
            }
        };
        Ok(DetectedState::Known {
            value,
            evidence: evidence(context, self.evidence_source),
        })
    }

    fn validate(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        ensure_parameters(self.metadata, parameters, ActionStage::Validate)?;
        Err(guided_mutation_blocked(ActionStage::Validate))
    }

    fn create_backup(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        ensure_parameters(self.metadata, parameters, ActionStage::Backup)?;
        Err(guided_mutation_blocked(ActionStage::Backup))
    }

    fn apply(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
        _backup: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        ensure_parameters(self.metadata, parameters, ActionStage::Apply)?;
        Err(guided_mutation_blocked(ActionStage::Apply))
    }

    fn verify_applied(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
        _backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        ensure_parameters(self.metadata, parameters, ActionStage::VerifyApplied)?;
        Err(guided_mutation_blocked(ActionStage::VerifyApplied))
    }

    fn rollback(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
        _backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        ensure_parameters(self.metadata, parameters, ActionStage::Rollback)?;
        Err(guided_mutation_blocked(ActionStage::Rollback))
    }

    fn verify_rolled_back(
        &self,
        _context: &ActionContext<'_>,
        parameters: &ActionParameters,
        _backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        ensure_parameters(self.metadata, parameters, ActionStage::VerifyRolledBack)?;
        Err(guided_mutation_blocked(ActionStage::VerifyRolledBack))
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        ensure_parameters(self.metadata, parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: self.metadata.id,
            result: self.result.to_owned(),
            method: self.method.to_owned(),
            resources: self
                .metadata
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: self.metadata.windows_update_impact.to_owned(),
            rollback_scope: "PCカスタムはOS設定を変更しないため、rollback対象はありません。"
                .to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        SETTINGS_TROUBLESHOOTING
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{
        backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup},
        compatibility::OsIdentity,
    };

    fn context(os: &OsIdentity, transaction_id: Uuid, item_id: Uuid) -> ActionContext<'_> {
        ActionContext {
            os_identity: os,
            transaction_id,
            item_id,
            observed_at_unix_ms: 1,
            is_elevated: false,
        }
    }

    fn harmless_envelope(
        action: &GuidedSetupAction,
        os: &OsIdentity,
        transaction_id: Uuid,
        item_id: Uuid,
    ) -> BackupEnvelope {
        BackupEnvelope::from_draft(
            BackupDraft {
                precondition_fingerprint: Fingerprint::of_bytes(b"unchanged"),
                intended_fingerprint: Fingerprint::of_bytes(b"unchanged"),
                payload: BackupPayload::Observation(ObservationBackup {
                    source: "guided rejection test".to_owned(),
                }),
            },
            transaction_id,
            item_id,
            action.metadata.id,
            action.metadata.action_version,
            1,
            os.base_build,
        )
    }

    fn assert_mutation_pipeline_refused(
        action: &'static GuidedSetupAction,
        parameters: ActionParameters,
    ) {
        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);
        let envelope = harmless_envelope(action, &os, transaction_id, item_id);

        for (expected_stage, error) in [
            (
                ActionStage::Validate,
                action
                    .validate(&context, &parameters)
                    .expect_err("guided validation must refuse mutation"),
            ),
            (
                ActionStage::Backup,
                action
                    .create_backup(&context, &parameters)
                    .expect_err("guided action must not create a backup"),
            ),
            (
                ActionStage::Apply,
                action
                    .apply(&context, &parameters, &envelope)
                    .expect_err("guided direct apply must refuse mutation"),
            ),
            (
                ActionStage::VerifyApplied,
                action
                    .verify_applied(&context, &parameters, &envelope)
                    .expect_err("guided apply verification must be unreachable"),
            ),
            (
                ActionStage::Rollback,
                action
                    .rollback(&context, &parameters, &envelope)
                    .expect_err("guided rollback must be unreachable"),
            ),
            (
                ActionStage::VerifyRolledBack,
                action
                    .verify_rolled_back(&context, &parameters, &envelope)
                    .expect_err("guided rollback verification must be unreachable"),
            ),
        ] {
            assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
            assert_eq!(error.stage, expected_stage);
            assert!(!error.retryable);
        }
    }

    #[test]
    fn guided_metadata_is_public_api_and_never_auto_applied() {
        for action in [&SETUP_DEFAULT_APPS_ACTION, &SETUP_AUDIO_OUTPUT_ACTION] {
            assert_eq!(action.metadata.kind, ActionKind::Guided);
            assert_eq!(action.metadata.method_class, MethodClass::PublicApi);
            assert!(!action.metadata.auto_apply_eligible);
            action
                .metadata
                .validate_static_contract()
                .expect("valid Guided Action metadata");
        }
    }

    #[test]
    fn default_apps_detects_guidance_without_an_os_change() {
        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);
        let parameters = ActionParameters::SetupDefaultApps {};

        let before = SETUP_DEFAULT_APPS_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect default-app guidance");
        assert!(matches!(
            before.known_value(),
            Some(ObservedValue::NoOsChange)
        ));
        assert_mutation_pipeline_refused(&SETUP_DEFAULT_APPS_ACTION, parameters.clone());
        let after = SETUP_DEFAULT_APPS_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect unchanged default-app guidance");
        assert_eq!(before, after);
    }

    #[test]
    fn audio_output_mutation_pipeline_is_blocked() {
        assert_mutation_pipeline_refused(
            &SETUP_AUDIO_OUTPUT_ACTION,
            ActionParameters::SetupAudioOutput {},
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "live Core Audio topology can legitimately change between two reads"]
    fn blocked_audio_action_leaves_real_endpoint_observation_unchanged() {
        let before =
            read_audio_output_observation().expect("read audio endpoints before rejection");
        assert_mutation_pipeline_refused(
            &SETUP_AUDIO_OUTPUT_ACTION,
            ActionParameters::SetupAudioOutput {},
        );
        let after = read_audio_output_observation().expect("read audio endpoints after rejection");
        assert_eq!(before, after);
    }
}
