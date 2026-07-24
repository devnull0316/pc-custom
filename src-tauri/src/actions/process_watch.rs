use std::path::Path;

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionMetadata, ActionParameters, ActionResult, ActionRiskLevel, ActionStage,
        AppliedEvidence, ChangeExplanation, DetectedState, MethodClass, ObservedProcess,
        ObservedValue, ProcessBindingParameters, RollbackEvidence, TroubleshootingStep,
        ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ProcessWatchBackup},
    windows::{registered_file_identity, snapshot_process_identities},
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup,
    validate_backup_for_apply, validate_base,
};

pub struct ProcessWatchAction;
pub static PROCESS_WATCH_ACTION: ProcessWatchAction = ProcessWatchAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::GamesProcessWatch,
    name: "登録したゲームの起動を確認する",
    description: "ToolhelpとWMIを組み合わせ、注入せずにpath・file identity・PID creation timeを照合します。",
    category: "games",
    tags: &["game", "process", "read-only"],
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
    parameter_schema: r#"{"binding":{"canonical_path":"local fixed-drive exe","file_identity":"volume+file-id"}}"#,
    resource_keys: &["process:registered-file-identity"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot",
        "https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew",
        "https://learn.microsoft.com/previous-versions/windows/desktop/krnlprov/win32-processstarttrace",
    ],
    compatibility_key: "games.process_watch.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。WMI欠落率とprotected processの挙動を更新後に再測定します。",
};

impl ProcessWatchAction {
    fn binding(parameters: &ActionParameters) -> ActionResult<&ProcessBindingParameters> {
        match parameters {
            ActionParameters::GamesProcessWatch { binding } => Ok(binding),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn validate_binding(binding: &ProcessBindingParameters) -> ActionResult<()> {
        if binding.canonical_path.is_empty()
            || binding.canonical_path.len() > 32_768
            || binding.canonical_path.contains('\0')
            || binding.file_identity.file_id == [0; 16]
        {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                ActionStage::Validate,
                false,
                "action.process_watch.invalid_binding",
            ));
        }
        let (canonical, identity) = registered_file_identity(&binding.canonical_path).map_err(|error| {
            map_windows_error(
                ActionStage::Validate,
                "action.process_watch.binding_revalidation_failed",
                error,
            )
        })?;
        if canonical != binding.canonical_path || identity != binding.file_identity {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Validate,
                false,
                "action.process_watch.file_identity_changed",
            ));
        }
        Ok(())
    }

    fn state(
        context: &ActionContext<'_>,
        binding: &ProcessBindingParameters,
    ) -> ActionResult<DetectedState> {
        let report = snapshot_process_identities().map_err(|error| {
            map_windows_error(
                ActionStage::Detect,
                "action.process_watch.snapshot_failed",
                error,
            )
        })?;
        let matches: Vec<ObservedProcess> = report
            .processes
            .into_iter()
            .filter(|process| {
                process.file_identity == binding.file_identity
                    && process.canonical_path == binding.canonical_path
            })
            .map(|process| ObservedProcess {
                process_id: process.process_id,
                creation_time_100ns: process.creation_time_100ns,
                canonical_path: process.canonical_path,
                file_identity: process.file_identity,
                corroborated_by_wmi: process.corroborated_by_wmi,
            })
            .collect();
        if matches.is_empty() {
            let expected_name = Path::new(&binding.canonical_path)
                .file_name()
                .and_then(|value| value.to_str());
            let possibly_inaccessible = expected_name.is_some_and(|expected| {
                report
                    .inaccessible_executable_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(expected))
            });
            if possibly_inaccessible {
                return Ok(DetectedState::Unknown {
                    reason: "A matching executable name exists but its identity is inaccessible"
                        .to_owned(),
                });
            }
        }
        Ok(DetectedState::Known {
            value: ObservedValue::Processes { matches },
            evidence: evidence(
                context,
                if report.wmi_available {
                    "Toolhelp + WMI + limited process handles"
                } else {
                    "Toolhelp + limited process handles (WMI unavailable)"
                },
            ),
        })
    }
}

impl Action for ProcessWatchAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let binding = Self::binding(parameters)?;
        Self::validate_binding(binding)?;
        Self::state(context, binding)
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
        Self::validate_binding(Self::binding(parameters)?)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let binding = Self::binding(parameters)?.clone();
        let state = Self::state(context, &binding)?;
        let fingerprint = fingerprint_state(&state, ActionStage::Backup)?;
        let intended_bytes = serde_json::to_vec(&binding).map_err(|_| {
            ActionError::new(
                ActionErrorCode::InternalInvariant,
                ActionStage::Backup,
                false,
                "action.process_watch.binding_serialization_failed",
            )
        })?;
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint,
            intended_fingerprint: Fingerprint::of_bytes(&intended_bytes),
            payload: BackupPayload::ProcessWatch(ProcessWatchBackup { binding }),
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
        let BackupPayload::ProcessWatch(saved) = &backup.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.process_watch.backup_kind_mismatch",
            ));
        };
        if &saved.binding != Self::binding(parameters)? {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.process_watch.binding_mismatch",
            ));
        }
        let state = Self::state(context, &saved.binding)?;
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
                Some(ObservedValue::Processes { .. })
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
        let observed = self.detect_current_state(context, parameters)?;
        Ok(RollbackEvidence {
            restored_fingerprint: fingerprint_state(&observed, ActionStage::Rollback)?,
            state: observed,
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
                Some(ObservedValue::Processes { .. })
            ),
            observed,
        })
    }

    fn explain_changes(
        &self,
        parameters: &ActionParameters,
    ) -> ActionResult<ChangeExplanation> {
        let _ = Self::binding(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "登録済みfile identityに一致するprocessの起動状態を観測します。".to_owned(),
            method: "Toolhelp snapshot＋WMI補助＋限定権限process handle（読み取り専用）".to_owned(),
            resources: METADATA.resource_keys.iter().map(|v| (*v).to_owned()).collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "OS変更がないため監視観測の終了だけで、processを終了しません。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.process_watch.rebind_after_executable_update",
            opens_official_settings: false,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    fn assert_current_process_is_observed(state: &DetectedState) {
        let Some(ObservedValue::Processes { matches }) = state.known_value() else {
            panic!("process observation must be known");
        };
        let current = matches
            .iter()
            .find(|process| process.process_id == std::process::id())
            .expect("current process must match its registered file identity");
        assert!(current.creation_time_100ns > 0);
    }

    #[test]
    fn apply_detect_rollback_detect_observes_current_process_without_mutation() {
        let executable = std::env::current_exe().expect("locate current test executable");
        let executable = executable
            .to_str()
            .expect("test executable path is valid Unicode");
        let (canonical_path, file_identity) = registered_file_identity(executable)
            .expect("register current executable identity");
        let binding = ProcessBindingParameters {
            canonical_path,
            file_identity,
        };
        let parameters = ActionParameters::GamesProcessWatch { binding };
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
        let draft = PROCESS_WATCH_ACTION
            .create_backup(&context, &parameters)
            .expect("create process observation backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let applied = PROCESS_WATCH_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("perform read-only process watch apply");
        envelope.record_applied(applied.applied_fingerprint);
        let detected = PROCESS_WATCH_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect current process after apply");
        assert_current_process_is_observed(&detected);

        PROCESS_WATCH_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("perform read-only process watch rollback");
        let restored = PROCESS_WATCH_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect current process after rollback");
        assert_current_process_is_observed(&restored);
        assert!(PROCESS_WATCH_ACTION
            .verify_rolled_back(&context, &parameters, &envelope)
            .expect("verify read-only rollback")
            .verified);
    }
}
