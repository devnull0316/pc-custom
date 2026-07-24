use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionMetadata, ActionParameters, ActionResult, ActionRiskLevel, ActionStage,
        AppliedEvidence, ChangeExplanation, DetectedState, MethodClass, ObservedValue,
        RollbackEvidence, ThemeColorMode, ThemeObservation, TroubleshootingStep,
        ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{
        classify_registry_backup, prepare_registry_backup, read_registry_state,
        restore_registry_backup, verify_registry_backup_restored, BackupDraft, BackupEnvelope,
        BackupPayload, CompositeBackup, Fingerprint, RegistryBackup, RegistryClassification,
        RegistryRestoreOutcome, RegistryTarget,
    },
    windows::{notify_theme_changed, write_raw_value},
};

use super::common::{
    decode_dword, dword_bytes, ensure_registry_key_preexisted, evidence, map_windows_error,
    validate_backup, validate_backup_for_apply, validate_base, REG_DWORD_TYPE,
};

const PERSONALIZE_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(PERSONALIZE_SUBKEY, "AppsUseLightTheme");
const SYSTEM_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(PERSONALIZE_SUBKEY, "SystemUsesLightTheme");

pub struct ColorModeAction;
pub static COLOR_MODE_ACTION: ColorModeAction = ColorModeAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::ThemeColorMode,
    name: "Windowsとアプリの明るさをそろえる",
    description: "アプリとWindowsの2つのテーマ値を1 Actionとして変更し、混在状態を避けます。",
    category: "appearance",
    tags: &["theme", "light", "dark"],
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
    parameter_schema: r#"{"mode":"light|dark"}"#,
    resource_keys: &[
        "registry:hkcu:64:software/microsoft/windows/currentversion/themes/personalize:appsuselighttheme",
        "registry:hkcu:64:software/microsoft/windows/currentversion/themes/personalize:systemuseslighttheme",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11",
        "https://learn.microsoft.com/windows/win32/winmsg/wm-settingchange",
    ],
    compatibility_key: "theme.color_mode.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "中。Windows更新後に2値・通知・rollbackの実機スモークを再実施します。",
};

impl ColorModeAction {
    fn mode(parameters: &ActionParameters) -> ActionResult<ThemeColorMode> {
        match parameters {
            ActionParameters::ThemeColorMode { mode } => Ok(*mode),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn desired_value(mode: ThemeColorMode) -> u32 {
        match mode {
            ThemeColorMode::Light => 1,
            ThemeColorMode::Dark => 0,
        }
    }

    fn read_theme_value(
        target: RegistryTarget,
        stage: ActionStage,
    ) -> ActionResult<Option<u32>> {
        let state = read_registry_state(&target.location()).map_err(|error| {
            map_windows_error(stage, "action.color_mode.detect_failed", error)
        })?;
        if !state.value_existed {
            return Ok(None);
        }
        match decode_dword(state.value_type, &state.raw_bytes) {
            Some(value @ (0 | 1)) => Ok(Some(value)),
            _ => Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                stage,
                false,
                "action.color_mode.unexpected_registry_value",
            )),
        }
    }

    fn state(context: &ActionContext<'_>) -> ActionResult<DetectedState> {
        let apps = Self::read_theme_value(APPS_TARGET, ActionStage::Detect)?;
        let system = Self::read_theme_value(SYSTEM_TARGET, ActionStage::Detect)?;
        let observation = match (apps, system) {
            (None, _) | (_, None) => ThemeObservation::Unconfigured,
            (Some(1), Some(1)) => ThemeObservation::Light,
            (Some(0), Some(0)) => ThemeObservation::Dark,
            _ => ThemeObservation::Mixed,
        };
        Ok(DetectedState::Known {
            value: ObservedValue::Theme(observation),
            evidence: evidence(context, "HKCU Themes Personalize (64-bit view)"),
        })
    }

    fn composite(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&CompositeBackup> {
        let BackupPayload::Composite(composite) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.color_mode.backup_kind_mismatch",
            ));
        };
        if composite.registry_entries.len() != 2
            || composite.registry_entries[0].location != APPS_TARGET.location()
            || composite.registry_entries[1].location != SYSTEM_TARGET.location()
            || !composite.all_or_stop
        {
            return Err(ActionError::recovery_required(
                stage,
                "action.color_mode.backup_target_mismatch",
            ));
        }
        Ok(composite)
    }

    fn verify_entry_applied(entry: &RegistryBackup) -> ActionResult<()> {
        let current = read_registry_state(&entry.location).map_err(|error| {
            map_windows_error(
                ActionStage::VerifyApplied,
                "action.color_mode.apply_verify_failed",
                error,
            )
        })?;
        if current != entry.applied_state() {
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                ActionStage::VerifyApplied,
                false,
                "action.color_mode.apply_verify_mismatch",
            ));
        }
        Ok(())
    }

    fn compensate_apply(entries: &[RegistryBackup]) -> ActionResult<()> {
        for entry in entries.iter().rev() {
            match restore_registry_backup(entry) {
                Ok(RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal) => {}
                Ok(RegistryRestoreOutcome::RestoredValueKeyRetained) => {
                    return Err(ActionError::recovery_required(
                        ActionStage::Recovery,
                        "action.color_mode.compensation_key_retained",
                    ));
                }
                Ok(RegistryRestoreOutcome::ExternalConflict) => {
                    return Err(ActionError::recovery_required(
                        ActionStage::Recovery,
                        "action.color_mode.compensation_conflict",
                    ));
                }
                Err(error) => {
                    return Err(map_windows_error(
                        ActionStage::Recovery,
                        "action.color_mode.compensation_failed",
                        error,
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply_registry_entries(entries: &[RegistryBackup]) -> ActionResult<()> {
        for entry in entries {
            ensure_registry_key_preexisted(entry, ActionStage::Apply)?;
            let current = read_registry_state(&entry.location).map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.color_mode.precondition_read_failed",
                    error,
                )
            })?;
            if current != entry.original {
                return Err(ActionError::new(
                    ActionErrorCode::ExternalConflict,
                    ActionStage::Apply,
                    false,
                    "action.apply.stale_preview",
                ));
            }
        }

        let mut touched = 0usize;
        for entry in entries {
            let current = match read_registry_state(&entry.location) {
                Ok(current) => current,
                Err(error) => {
                    Self::compensate_apply(&entries[..touched])?;
                    return Err(map_windows_error(
                        ActionStage::Apply,
                        "action.color_mode.prewrite_read_failed",
                        error,
                    ));
                }
            };
            if current != entry.original {
                Self::compensate_apply(&entries[..touched])?;
                return Err(ActionError::new(
                    ActionErrorCode::ExternalConflict,
                    ActionStage::Apply,
                    false,
                    "action.color_mode.prewrite_external_change",
                ));
            }

            if let Err(error) = write_raw_value(
                &entry.location,
                entry.intended_type,
                &entry.intended_raw,
            )
            .map_err(|error| {
                map_windows_error(ActionStage::Apply, "action.color_mode.apply_failed", error)
            }) {
                Self::compensate_apply(&entries[..touched])?;
                return Err(error);
            }
            touched += 1;
            if let Err(error) = Self::verify_entry_applied(entry) {
                Self::compensate_apply(&entries[..touched])?;
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Action for ColorModeAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let _ = Self::mode(parameters)?;
        Self::state(context)
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
        let _ = Self::mode(parameters)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let desired = dword_bytes(Self::desired_value(Self::mode(parameters)?));
        let apps = prepare_registry_backup(
            APPS_TARGET,
            REG_DWORD_TYPE,
            desired.clone(),
            METADATA.action_version,
            context.os_identity.base_build,
        )
        .map_err(|error| {
            map_windows_error(ActionStage::Backup, "action.color_mode.backup_failed", error)
        })?;
        let system = prepare_registry_backup(
            SYSTEM_TARGET,
            REG_DWORD_TYPE,
            desired,
            METADATA.action_version,
            context.os_identity.base_build,
        )
        .map_err(|error| {
            map_windows_error(ActionStage::Backup, "action.color_mode.backup_failed", error)
        })?;
        let precondition_fingerprint = Fingerprint::of_parts([
            apps.original.fingerprint(&apps.location).0.as_slice(),
            system.original.fingerprint(&system.location).0.as_slice(),
        ]);
        let intended_fingerprint = Fingerprint::of_parts([
            apps.intended_state().fingerprint(&apps.location).0.as_slice(),
            system
                .intended_state()
                .fingerprint(&system.location)
                .0
                .as_slice(),
        ]);
        Ok(BackupDraft {
            precondition_fingerprint,
            intended_fingerprint,
            payload: BackupPayload::Composite(CompositeBackup {
                registry_entries: vec![apps, system],
                all_or_stop: true,
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
        let desired = dword_bytes(Self::desired_value(Self::mode(parameters)?));
        let composite = Self::composite(envelope, ActionStage::Apply)?;
        if composite.registry_entries.iter().any(|entry| {
            entry.intended_type != REG_DWORD_TYPE || entry.intended_raw != desired
        }) {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.color_mode.backup_parameter_mismatch",
            ));
        }

        Self::apply_registry_entries(&composite.registry_entries)?;

        let broadcast = notify_theme_changed();
        let detected = Self::state(context)?;
        let state = if broadcast.setting_change_acknowledged {
            detected
        } else {
            match detected {
                DetectedState::Known { value, evidence } => {
                    DetectedState::NeedsRestart { value, evidence }
                }
                other => other,
            }
        };
        Ok(AppliedEvidence {
            applied_fingerprint: envelope.intended_fingerprint,
            state,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        let expected = ThemeObservation::from(Self::mode(parameters)?);
        let observed = Self::state(context)?;
        let verified = matches!(
            observed.known_value(),
            Some(ObservedValue::Theme(value)) if *value == expected
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
            &METADATA,
            context,
            parameters,
            true,
            ActionStage::Rollback,
        )?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let composite = Self::composite(envelope, ActionStage::Rollback)?;

        let mut classifications = Vec::with_capacity(composite.registry_entries.len());
        for entry in &composite.registry_entries {
            classifications.push(classify_registry_backup(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.color_mode.rollback_preflight_failed",
                    error,
                )
            })?);
        }
        if classifications.contains(&RegistryClassification::Third) {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }

        for entry in composite.registry_entries.iter().rev() {
            match restore_registry_backup(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.color_mode.rollback_failed",
                    error,
                )
            })? {
                RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal => {}
                RegistryRestoreOutcome::RestoredValueKeyRetained => {
                    return Err(ActionError::recovery_required(
                        ActionStage::Rollback,
                        "action.color_mode.rollback_key_retained",
                    ));
                }
                RegistryRestoreOutcome::ExternalConflict => {
                    return Err(ActionError::recovery_required(
                        ActionStage::Rollback,
                        "action.color_mode.rollback_race_detected",
                    ));
                }
            }
        }
        let _broadcast = notify_theme_changed();
        let state = Self::state(context)?;
        Ok(RollbackEvidence {
            restored_fingerprint: envelope.precondition_fingerprint,
            state,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        _parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        let composite = Self::composite(envelope, ActionStage::VerifyRolledBack)?;
        let mut verified = true;
        for entry in &composite.registry_entries {
            verified &= verify_registry_backup_restored(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::VerifyRolledBack,
                    "action.color_mode.rollback_verify_failed",
                    error,
                )
            })?;
        }
        let observed = Self::state(context)?;
        Ok(Verification { verified, observed })
    }

    fn explain_changes(
        &self,
        parameters: &ActionParameters,
    ) -> ActionResult<ChangeExplanation> {
        let mode = Self::mode(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: format!(
                "Windowsとアプリの配色を{}へそろえます。",
                match mode {
                    ThemeColorMode::Light => "ライト",
                    ThemeColorMode::Dark => "ダーク",
                }
            ),
            method: "2つのHKCU値をcomposite backup後に変更し、WM_SETTINGCHANGEとSHChangeNotifyで通知".to_owned(),
            resources: METADATA.resource_keys.iter().map(|v| (*v).to_owned()).collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "2値を逆順で元の欠如・型・raw bytesへ戻します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.color_mode.some_apps_need_restart",
            opens_official_settings: true,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{
        backup::RegistryLocation,
        windows::{delete_key_if_empty, delete_value},
    };

    struct IsolatedCompositeCleanup(Vec<RegistryLocation>);

    impl Drop for IsolatedCompositeCleanup {
        fn drop(&mut self) {
            for location in &self.0 {
                if let Err(error) = delete_value(location) {
                    eprintln!("isolated color-mode test value cleanup failed: {error}");
                }
            }
            if let Some(location) = self.0.first() {
                match delete_key_if_empty(location) {
                    Ok(true) => {}
                    Ok(false) => eprintln!("isolated color-mode test key was not empty"),
                    Err(error) => {
                        eprintln!("isolated color-mode test key cleanup failed: {error}")
                    }
                }
            }
        }
    }

    fn isolated_targets() -> (RegistryTarget, RegistryTarget) {
        let key = Box::leak(
            format!(
                r"Software\Totonoe\IntegrationTests\ColorMode\{}",
                Uuid::new_v4()
            )
            .into_boxed_str(),
        );
        (
            RegistryTarget::current_user_64(key, "AppsUseLightTheme"),
            RegistryTarget::current_user_64(key, "SystemUsesLightTheme"),
        )
    }

    #[test]
    fn composite_storage_apply_detect_rollback_detect_is_isolated() {
        let (apps_target, system_target) = isolated_targets();
        let apps_location = apps_target.location();
        let system_location = system_target.location();
        let _cleanup = IsolatedCompositeCleanup(vec![
            apps_location.clone(),
            system_location.clone(),
        ]);
        write_raw_value(&apps_location, REG_DWORD_TYPE, &dword_bytes(1))
            .expect("seed isolated app theme value");
        write_raw_value(&system_location, REG_DWORD_TYPE, &dword_bytes(1))
            .expect("seed isolated system theme value");

        let desired = dword_bytes(0);
        let entries = vec![
            prepare_registry_backup(
                apps_target,
                REG_DWORD_TYPE,
                desired.clone(),
                METADATA.action_version,
                26_100,
            )
            .expect("prepare isolated app theme backup"),
            prepare_registry_backup(
                system_target,
                REG_DWORD_TYPE,
                desired,
                METADATA.action_version,
                26_100,
            )
            .expect("prepare isolated system theme backup"),
        ];

        ColorModeAction::apply_registry_entries(&entries)
            .expect("apply isolated composite color-mode storage");
        assert_eq!(
            ColorModeAction::read_theme_value(apps_target, ActionStage::Detect)
                .expect("detect isolated app theme"),
            Some(0)
        );
        assert_eq!(
            ColorModeAction::read_theme_value(system_target, ActionStage::Detect)
                .expect("detect isolated system theme"),
            Some(0)
        );

        for entry in entries.iter().rev() {
            assert_eq!(
                restore_registry_backup(entry).expect("rollback isolated theme entry"),
                RegistryRestoreOutcome::Restored
            );
        }
        assert!(entries.iter().all(|entry| {
            verify_registry_backup_restored(entry)
                .expect("verify isolated composite rollback")
        }));
        assert_eq!(
            ColorModeAction::read_theme_value(apps_target, ActionStage::Detect)
                .expect("detect restored app theme"),
            Some(1)
        );
        assert_eq!(
            ColorModeAction::read_theme_value(system_target, ActionStage::Detect)
                .expect("detect restored system theme"),
            Some(1)
        );
    }
}
