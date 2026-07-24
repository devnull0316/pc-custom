use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionMetadata, ActionParameters, ActionResult, ActionRiskLevel, ActionStage,
        AppliedEvidence, ChangeExplanation, DetectedState, MethodClass, ObservedValue,
        RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{
        prepare_registry_backup, read_registry_state, restore_registry_backup,
        verify_registry_backup_restored, BackupDraft, BackupEnvelope, BackupPayload,
        RegistryBackup, RegistryRestoreOutcome, RegistryTarget,
    },
    windows::{notify_explorer_settings_changed, write_raw_value},
};

use super::common::{
    decode_dword, dword_bytes, evidence, map_windows_error, validate_backup,
    validate_backup_for_apply, validate_base, REG_DWORD_TYPE,
};

const ADVANCED_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const EXTENSIONS_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(ADVANCED_SUBKEY, "HideFileExt");
const HIDDEN_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(ADVANCED_SUBKEY, "Hidden");
const CLOCK_SECONDS_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(ADVANCED_SUBKEY, "ShowSecondsInSystemClock");

pub struct ShowExtensionsAction;
pub struct ShowHiddenAction;
pub struct ClockSecondsAction;
pub static SHOW_EXTENSIONS_ACTION: ShowExtensionsAction = ShowExtensionsAction;
pub static SHOW_HIDDEN_ACTION: ShowHiddenAction = ShowHiddenAction;
pub static CLOCK_SECONDS_ACTION: ClockSecondsAction = ClockSecondsAction;

static EXTENSIONS_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::ExplorerShowExtensions,
    name: "ファイルの拡張子を常に表示する",
    description: "HKCUの文書化されたExplorer設定を変更し、Explorerを終了せず通知します。",
    category: "explorer",
    tags: &["files", "extensions", "explorer"],
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
    kind: ActionKind::Persistent,
    parameter_schema: r#"{"show":"boolean"}"#,
    resource_keys: &[
        "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:hidefileext",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11",
    ],
    compatibility_key: "explorer.show_extensions.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "中。Windows更新後に値と非破壊通知の実機スモークを再実施します。",
};

static HIDDEN_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::ExplorerShowHidden,
    name: "隠しファイルを表示する",
    description: "保護されたOSファイルは対象にせず、通常の隠しファイル表示だけを変更します。",
    category: "explorer",
    tags: &["files", "hidden", "explorer"],
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
    parameter_schema: r#"{"show":"boolean"}"#,
    resource_keys: &[
        "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:hidden",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11",
    ],
    compatibility_key: "explorer.show_hidden.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "中。Windows更新後に値と非破壊通知の実機スモークを再実施します。",
};

static CLOCK_SECONDS_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::ExplorerClockSeconds,
    name: "タスクバーの時計に秒を表示する",
    description: "HKCUの文書化されたExplorer設定を変更し、Explorerを終了せず通知します。",
    category: "explorer",
    tags: &["taskbar", "clock", "explorer"],
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
    kind: ActionKind::Persistent,
    parameter_schema: r#"{"show":"boolean"}"#,
    resource_keys: &[
        "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:showsecondsinsystemclock",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11",
    ],
    compatibility_key: "explorer.clock_seconds.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "中。Windows更新後に値と非破壊通知の実機スモークを再実施します。",
};

fn clock_seconds_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    match parameters {
        ActionParameters::ExplorerClockSeconds { show } => Ok(u32::from(*show)),
        _ => Err(wrong_parameters()),
    }
}

fn clock_seconds_value_is_valid(value: u32) -> bool {
    value <= 1
}

fn extensions_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    match parameters {
        ActionParameters::ExplorerShowExtensions { show } => Ok(u32::from(!*show)),
        _ => Err(wrong_parameters()),
    }
}

fn hidden_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    match parameters {
        ActionParameters::ExplorerShowHidden { show } => Ok(if *show { 1 } else { 2 }),
        _ => Err(wrong_parameters()),
    }
}

fn extensions_value_is_valid(value: u32) -> bool {
    value <= 1
}

fn hidden_value_is_valid(value: u32) -> bool {
    matches!(value, 1 | 2)
}

fn wrong_parameters() -> ActionError {
    ActionError::new(
        ActionErrorCode::WrongParameters,
        ActionStage::Validate,
        false,
        "action.parameters.id_mismatch",
    )
}

fn detect_setting(
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    metadata: &'static ActionMetadata,
    target: RegistryTarget,
    desired: fn(&ActionParameters) -> ActionResult<u32>,
    valid_value: fn(u32) -> bool,
) -> ActionResult<DetectedState> {
    validate_base(metadata, context, parameters, false, ActionStage::Detect)?;
    let _ = desired(parameters)?;
    let state = read_registry_state(&target.location()).map_err(|error| {
        map_windows_error(ActionStage::Detect, "action.explorer.detect_failed", error)
    })?;
    let configured = if state.value_existed {
        match decode_dword(state.value_type, &state.raw_bytes) {
            Some(value) if valid_value(value) => Some(value),
            _ => {
                return Ok(DetectedState::Unknown {
                    reason: "Explorer setting has an unexpected type or value".to_owned(),
                })
            }
        }
    } else {
        None
    };
    Ok(DetectedState::Known {
        value: ObservedValue::RegistryDword { configured },
        evidence: evidence(context, "HKCU Explorer Advanced (64-bit view)"),
    })
}


fn compensate_failed_apply(backup: &RegistryBackup) -> ActionResult<()> {
    match restore_registry_backup(backup).map_err(|error| {
        map_windows_error(
            ActionStage::Recovery,
            "action.explorer.apply_compensation_failed",
            error,
        )
    })? {
        RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal => Ok(()),
        RegistryRestoreOutcome::ExternalConflict => Err(ActionError::recovery_required(
            ActionStage::Recovery,
            "action.explorer.apply_compensation_conflict",
        )),
    }
}

fn apply_registry_backup(backup: &RegistryBackup) -> ActionResult<()> {
    let current = read_registry_state(&backup.location).map_err(|error| {
        map_windows_error(
            ActionStage::Apply,
            "action.explorer.precondition_read_failed",
            error,
        )
    })?;
    if current != backup.original {
        return Err(ActionError::new(
            ActionErrorCode::ExternalConflict,
            ActionStage::Apply,
            false,
            "action.apply.stale_preview",
        ));
    }
    write_raw_value(
        &backup.location,
        backup.intended_type,
        &backup.intended_raw,
    )
    .map_err(|error| {
        map_windows_error(ActionStage::Apply, "action.explorer.apply_failed", error)
    })?;

    let applied = match read_registry_state(&backup.location) {
        Ok(applied) => applied,
        Err(error) => {
            compensate_failed_apply(backup)?;
            return Err(map_windows_error(
                ActionStage::VerifyApplied,
                "action.explorer.apply_verify_read_failed",
                error,
            ));
        }
    };
    if applied != backup.applied_state() {
        compensate_failed_apply(backup)?;
        return Err(ActionError::new(
            ActionErrorCode::StateUnknown,
            ActionStage::VerifyApplied,
            false,
            "action.explorer.apply_verify_mismatch",
        ));
    }
    Ok(())
}
macro_rules! impl_explorer_action {
    ($type:ty, $metadata:ident, $target:ident, $desired:ident, $valid:ident, $result:literal) => {
        impl Action for $type {
            fn metadata(&self) -> &'static ActionMetadata {
                &$metadata
            }

            fn detect_current_state(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
            ) -> ActionResult<DetectedState> {
                detect_setting(
                    context,
                    parameters,
                    &$metadata,
                    $target,
                    $desired,
                    $valid,
                )
            }

            fn validate(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
            ) -> ActionResult<ValidationReport> {
                let report = validate_base(
                    &$metadata,
                    context,
                    parameters,
                    true,
                    ActionStage::Validate,
                )?;
                let _ = $desired(parameters)?;
                Ok(report)
            }

            fn create_backup(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
            ) -> ActionResult<BackupDraft> {
                self.validate(context, parameters)?;
                let intended = dword_bytes($desired(parameters)?);
                let backup = prepare_registry_backup(
                    $target,
                    REG_DWORD_TYPE,
                    intended,
                    $metadata.action_version,
                    context.os_identity.base_build,
                )
                .map_err(|error| {
                    map_windows_error(ActionStage::Backup, "action.explorer.backup_failed", error)
                })?;
                Ok(BackupDraft {
                    precondition_fingerprint: backup
                        .original
                        .fingerprint(&backup.location),
                    intended_fingerprint: backup
                        .intended_state()
                        .fingerprint(&backup.location),
                    payload: BackupPayload::Registry(backup),
                })
            }

            fn apply(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
                envelope: &BackupEnvelope,
            ) -> ActionResult<AppliedEvidence> {
                self.validate(context, parameters)?;
                validate_backup_for_apply(&$metadata, context, envelope)?;
                let expected = $desired(parameters)?;
                let BackupPayload::Registry(backup) = &envelope.payload else {
                    return Err(ActionError::recovery_required(
                        ActionStage::Apply,
                        "action.explorer.backup_kind_mismatch",
                    ));
                };
                if backup.location != $target.location()
                    || backup.intended_type != REG_DWORD_TYPE
                    || backup.intended_raw != dword_bytes(expected)
                {
                    return Err(ActionError::recovery_required(
                        ActionStage::Apply,
                        "action.explorer.backup_target_mismatch",
                    ));
                }
                apply_registry_backup(backup)?;
                let _broadcast = notify_explorer_settings_changed();
                let state = self.detect_current_state(context, parameters)?;
                Ok(AppliedEvidence {
                    applied_fingerprint: backup
                        .applied_state()
                        .fingerprint(&backup.location),
                    state,
                })
            }

            fn verify_applied(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
                envelope: &BackupEnvelope,
            ) -> ActionResult<Verification> {
                validate_backup(&$metadata, context, envelope, ActionStage::VerifyApplied)?;
                let expected = $desired(parameters)?;
                let observed = self.detect_current_state(context, parameters)?;
                let verified = matches!(
                    observed.known_value(),
                    Some(ObservedValue::RegistryDword {
                        configured: Some(value),
                    }) if *value == expected
                );
                Ok(Verification { verified, observed })
            }

            fn rollback(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
                envelope: &BackupEnvelope,
            ) -> ActionResult<RollbackEvidence> {
                validate_base(
                    &$metadata,
                    context,
                    parameters,
                    true,
                    ActionStage::Rollback,
                )?;
                validate_backup(&$metadata, context, envelope, ActionStage::Rollback)?;
                let BackupPayload::Registry(backup) = &envelope.payload else {
                    return Err(ActionError::recovery_required(
                        ActionStage::Rollback,
                        "action.explorer.backup_kind_mismatch",
                    ));
                };
                if backup.location != $target.location() {
                    return Err(ActionError::recovery_required(
                        ActionStage::Rollback,
                        "action.explorer.backup_target_mismatch",
                    ));
                }
                let outcome = restore_registry_backup(backup).map_err(|error| {
                    map_windows_error(
                        ActionStage::Rollback,
                        "action.explorer.rollback_failed",
                        error,
                    )
                })?;
                if outcome == RegistryRestoreOutcome::ExternalConflict {
                    return Err(ActionError::new(
                        ActionErrorCode::ExternalConflict,
                        ActionStage::Rollback,
                        false,
                        "action.rollback.external_change_detected",
                    ));
                }
                let _broadcast = notify_explorer_settings_changed();
                let state = self.detect_current_state(context, parameters)?;
                Ok(RollbackEvidence {
                    restored_fingerprint: backup
                        .original
                        .fingerprint(&backup.location),
                    state,
                })
            }

            fn verify_rolled_back(
                &self,
                context: &ActionContext<'_>,
                parameters: &ActionParameters,
                envelope: &BackupEnvelope,
            ) -> ActionResult<Verification> {
                validate_backup(
                    &$metadata,
                    context,
                    envelope,
                    ActionStage::VerifyRolledBack,
                )?;
                let BackupPayload::Registry(backup) = &envelope.payload else {
                    return Err(ActionError::recovery_required(
                        ActionStage::VerifyRolledBack,
                        "action.explorer.backup_kind_mismatch",
                    ));
                };
                let verified = verify_registry_backup_restored(backup).map_err(|error| {
                    map_windows_error(
                        ActionStage::VerifyRolledBack,
                        "action.explorer.rollback_verify_failed",
                        error,
                    )
                })?;
                let observed = self.detect_current_state(context, parameters)?;
                Ok(Verification { verified, observed })
            }

            fn explain_changes(
                &self,
                parameters: &ActionParameters,
            ) -> ActionResult<ChangeExplanation> {
                let _ = $desired(parameters)?;
                Ok(ChangeExplanation {
                    action_id: $metadata.id,
                    result: $result.to_owned(),
                    method: "HKCU registry（型付きraw backup）＋SHChangeNotify".to_owned(),
                    resources: $metadata
                        .resource_keys
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect(),
                    requires_admin: false,
                    requires_restart: false,
                    windows_update_impact: $metadata.windows_update_impact.to_owned(),
                    rollback_scope: "元の欠如・型・raw bytesへ正確に戻します。".to_owned(),
                })
            }

            fn troubleshooting(
                &self,
                _code: ActionErrorCode,
            ) -> &'static [TroubleshootingStep] {
                &[TroubleshootingStep {
                    message_key: "action.explorer.sign_out_if_visual_refresh_is_delayed",
                    opens_official_settings: false,
                }]
            }
        }
    };
}

impl_explorer_action!(
    ShowExtensionsAction,
    EXTENSIONS_METADATA,
    EXTENSIONS_TARGET,
    extensions_desired,
    extensions_value_is_valid,
    "ファイル名の拡張子表示を選択した状態へ変更します。"
);

impl_explorer_action!(
    ShowHiddenAction,
    HIDDEN_METADATA,
    HIDDEN_TARGET,
    hidden_desired,
    hidden_value_is_valid,
    "通常の隠しファイル表示を選択した状態へ変更します。"
);

impl_explorer_action!(
    ClockSecondsAction,
    CLOCK_SECONDS_METADATA,
    CLOCK_SECONDS_TARGET,
    clock_seconds_desired,
    clock_seconds_value_is_valid,
    "タスクバーの時計に秒を表示する状態へ変更します。"
);

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{
        backup::RegistryLocation,
        compatibility::OsIdentity,
        windows::{delete_key_if_empty, delete_value},
    };

    struct IsolatedKeyCleanup(RegistryLocation);

    impl Drop for IsolatedKeyCleanup {
        fn drop(&mut self) {
            if let Err(error) = delete_value(&self.0) {
                eprintln!("isolated Explorer test value cleanup failed: {error}");
            }
            match delete_key_if_empty(&self.0) {
                Ok(true) => {}
                Ok(false) => eprintln!("isolated Explorer test key was not empty"),
                Err(error) => eprintln!("isolated Explorer test key cleanup failed: {error}"),
            }
        }
    }

    fn unique_target(value_name: &'static str) -> RegistryTarget {
        let key = Box::leak(
            format!(
                r"Software\Totonoe\IntegrationTests\Explorer\{}",
                Uuid::new_v4()
            )
            .into_boxed_str(),
        );
        RegistryTarget::current_user_64(key, value_name)
    }

    fn round_trip(
        metadata: &'static ActionMetadata,
        target: RegistryTarget,
        parameters: &ActionParameters,
        desired: fn(&ActionParameters) -> ActionResult<u32>,
        valid: fn(u32) -> bool,
        original: Option<u32>,
    ) {
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        if let Some(original) = original {
            write_raw_value(&location, REG_DWORD_TYPE, &dword_bytes(original))
                .expect("seed isolated Explorer value");
        }
        let intended = desired(parameters).expect("derive intended Explorer value");
        let backup = prepare_registry_backup(
            target,
            REG_DWORD_TYPE,
            dword_bytes(intended),
            metadata.action_version,
            26_100,
        )
        .expect("prepare isolated Explorer backup");
        let os = OsIdentity::from_test_build(26_100);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };

        apply_registry_backup(&backup).expect("apply isolated Explorer action storage");
        let detected = detect_setting(
            &context,
            parameters,
            metadata,
            target,
            desired,
            valid,
        )
        .expect("detect applied isolated Explorer setting");
        assert!(matches!(
            detected.known_value(),
            Some(ObservedValue::RegistryDword {
                configured: Some(value),
            }) if *value == intended
        ));

        assert_eq!(
            restore_registry_backup(&backup).expect("rollback isolated Explorer setting"),
            RegistryRestoreOutcome::Restored
        );
        assert!(verify_registry_backup_restored(&backup)
            .expect("verify isolated Explorer rollback"));
        let restored = detect_setting(
            &context,
            parameters,
            metadata,
            target,
            desired,
            valid,
        )
        .expect("detect restored isolated Explorer setting");
        assert!(matches!(
            restored.known_value(),
            Some(ObservedValue::RegistryDword { configured }) if *configured == original
        ));
    }

    #[test]
    fn show_extensions_storage_apply_detect_rollback_detect_is_isolated() {
        round_trip(
            &EXTENSIONS_METADATA,
            unique_target("HideFileExt"),
            &ActionParameters::ExplorerShowExtensions { show: true },
            extensions_desired,
            extensions_value_is_valid,
            None,
        );
    }

    #[test]
    fn show_hidden_storage_apply_detect_rollback_detect_is_isolated() {
        round_trip(
            &HIDDEN_METADATA,
            unique_target("Hidden"),
            &ActionParameters::ExplorerShowHidden { show: true },
            hidden_desired,
            hidden_value_is_valid,
            Some(2),
        );
    }

    #[test]
    fn clock_seconds_storage_apply_detect_rollback_detect_is_isolated() {
        round_trip(
            &CLOCK_SECONDS_METADATA,
            unique_target("ShowSecondsInSystemClock"),
            &ActionParameters::ExplorerClockSeconds { show: true },
            clock_seconds_desired,
            clock_seconds_value_is_valid,
            None,
        );
    }
}

#[cfg(test)]
mod compatibility_tests {
    use uuid::Uuid;

    use super::*;
    use crate::compatibility::OsIdentity;

    #[test]
    fn unknown_build_blocks_mutation_before_registry_access() {
        let os = OsIdentity::from_test_build(99_999);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let error = SHOW_EXTENSIONS_ACTION
            .validate(
                &context,
                &ActionParameters::ExplorerShowExtensions { show: true },
            )
            .expect_err("unknown Windows build must fail closed");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
        assert_eq!(error.stage, ActionStage::Validate);
        assert_eq!(error.code.as_code(), "RECOVERY_REQUIRED");
    }
}
