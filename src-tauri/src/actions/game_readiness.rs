//! Read-only composition of Windows game-readiness signals.
//!
//! Every component is collected independently. An unavailable API or malformed
//! fixed registry value becomes a component-level `Unknown`; it never causes
//! the other observed values to be discarded or guessed.

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, GameReadinessObservation, MethodClass, ObservedValue,
        ReadinessComponent, RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, ObservationBackup, RegistryTarget},
    windows::{
        active_power_scheme_guid, read_active_advanced_color, read_default_render_audio_endpoint,
        read_primary_refresh_rate, read_system_drive_space, read_value_state, WindowsResult,
    },
};

use super::common::{
    evidence, fingerprint_state, validate_backup, validate_backup_for_apply, validate_base,
    REG_DWORD_TYPE,
};

pub struct GameReadinessCheckAction;
pub static GAME_READINESS_CHECK_ACTION: GameReadinessCheckAction = GameReadinessCheckAction;

const BACKUP_SOURCE: &str = "fixed game-readiness observation sources";

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::GamesReadinessCheck,
    name: "ゲーム前の確認情報を表示する",
    description: "表示Hz、Advanced Color、電源構成、システムドライブ空き容量、既定の再生デバイスを公開APIで確認し、Game Modeと通知は固定HKCU登録値を設定の目安として表示します。Advanced ColorをHDRの実効状態とは断定せず、取得できない値も推測しません。",
    category: "games",
    tags: &["game", "readiness", "display", "audio", "storage", "read-only"],
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
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &[
        "display:primary-current-mode:observation",
        "display:active-advanced-color:observation",
        "registry:hkcu:game-mode:observation",
        "power:active-scheme:observation",
        "filesystem:system-volume-space:observation",
        "audio:default-render-endpoint:observation",
        "registry:hkcu:toast-notifications:observation",
    ],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumdisplaysettingsw",
        "https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-displayconfig_get_advanced_color_info",
        "https://learn.microsoft.com/windows/win32/api/powrprof/nf-powrprof-powergetactivescheme",
        "https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw",
        "https://learn.microsoft.com/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint",
        "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11",
    ],
    compatibility_key: "games.readiness_check.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。Windows公開APIと固定HKCU設定値の読み取りだけを行います。",
};

const GAME_MODE_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(r"Software\Microsoft\GameBar", "AutoGameModeEnabled");
const TOAST_TARGET: RegistryTarget = RegistryTarget::current_user_64(
    r"Software\Microsoft\Windows\CurrentVersion\PushNotifications",
    "ToastEnabled",
);

fn component<T>(result: WindowsResult<T>, reason_code: &'static str) -> ReadinessComponent<T> {
    match result {
        Ok(value) => ReadinessComponent::Known { value },
        Err(_) => ReadinessComponent::Unknown {
            reason_code: reason_code.to_owned(),
        },
    }
}

fn read_optional_toggle(
    target: RegistryTarget,
    read_reason: &'static str,
    invalid_reason: &'static str,
) -> ReadinessComponent<bool> {
    let state = match read_value_state(&target.location(), 4) {
        Ok(state) => state,
        Err(_) => {
            return ReadinessComponent::Unknown {
                reason_code: read_reason.to_owned(),
            }
        }
    };
    let Some(value) = state.value else {
        return ReadinessComponent::Unconfigured;
    };
    if value.value_type != REG_DWORD_TYPE || value.bytes.len() != 4 {
        return ReadinessComponent::Unknown {
            reason_code: invalid_reason.to_owned(),
        };
    }
    let raw = u32::from_le_bytes([
        value.bytes[0],
        value.bytes[1],
        value.bytes[2],
        value.bytes[3],
    ]);
    match raw {
        0 => ReadinessComponent::Known { value: false },
        1 => ReadinessComponent::Known { value: true },
        _ => ReadinessComponent::Unknown {
            reason_code: invalid_reason.to_owned(),
        },
    }
}

fn observe_readiness() -> GameReadinessObservation {
    GameReadinessObservation {
        refresh_rate: component(
            read_primary_refresh_rate(),
            "primary_refresh_rate_unavailable",
        ),
        advanced_color: component(read_active_advanced_color(), "advanced_color_unavailable"),
        game_mode: read_optional_toggle(
            GAME_MODE_TARGET,
            "game_mode_registry_unavailable",
            "game_mode_registry_value_invalid",
        ),
        active_power_scheme: component(
            active_power_scheme_guid(),
            "active_power_scheme_unavailable",
        ),
        system_drive_space: component(read_system_drive_space(), "system_drive_space_unavailable"),
        default_render_audio: component(
            read_default_render_audio_endpoint(),
            "default_render_audio_unavailable",
        ),
        toast_notifications: read_optional_toggle(
            TOAST_TARGET,
            "toast_notifications_registry_unavailable",
            "toast_notifications_registry_value_invalid",
        ),
    }
}

impl GameReadinessCheckAction {
    fn ensure_parameters(parameters: &ActionParameters) -> ActionResult<()> {
        if !matches!(parameters, ActionParameters::GamesReadinessCheck {}) {
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

impl Action for GameReadinessCheckAction {
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
        Ok(DetectedState::Known {
            value: ObservedValue::GameReadiness(observe_readiness()),
            evidence: evidence(
                context,
                "Windows display, DisplayConfig, Power, storage, Core Audio, and fixed HKCU observations",
            ),
        })
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, false, ActionStage::Validate)?;
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
                source: BACKUP_SOURCE.to_owned(),
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
        match &backup.payload {
            BackupPayload::Observation(observation) if observation.source == BACKUP_SOURCE => {}
            _ => {
                return Err(ActionError::recovery_required(
                    ActionStage::Apply,
                    "action.games.readiness.backup_kind_mismatch",
                ))
            }
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
                Some(ObservedValue::GameReadiness(_))
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
                Some(ObservedValue::GameReadiness(_))
            ),
            observed,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::ensure_parameters(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "ゲーム前の各確認項目を独立して表示します。Game Modeと通知の登録値は設定の目安として示し、取得不能な項目は推測せず不明として残します。".to_owned(),
            method: "公開Windows APIとコンパイル済みの固定HKCU設定値を使う読み取り専用確認。".to_owned(),
            resources: METADATA.resource_keys.iter().map(|value| (*value).to_owned()).collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "OS設定を変更しないため、rollbackも再観測のみです。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.games.readiness.open_windows_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    fn readiness(state: &DetectedState) -> &GameReadinessObservation {
        match state.known_value() {
            Some(ObservedValue::GameReadiness(value)) => value,
            other => panic!("expected game readiness observation, got {other:?}"),
        }
    }

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
        let parameters = ActionParameters::GamesReadinessCheck {};

        let before = GAME_READINESS_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect readiness before round trip");
        let draft = GAME_READINESS_CHECK_ACTION
            .create_backup(&context, &parameters)
            .expect("create read-only readiness backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let applied = GAME_READINESS_CHECK_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("perform read-only readiness apply");
        envelope.record_applied(applied.applied_fingerprint);
        let detected = GAME_READINESS_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect readiness after apply");
        assert!(matches!(
            detected.known_value(),
            Some(ObservedValue::GameReadiness(_))
        ));

        GAME_READINESS_CHECK_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("perform read-only readiness rollback");
        let after = GAME_READINESS_CHECK_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect readiness after rollback");

        let before = readiness(&before);
        let after = readiness(&after);
        assert_eq!(before.refresh_rate, after.refresh_rate);
        assert_eq!(before.advanced_color, after.advanced_color);
        assert_eq!(before.game_mode, after.game_mode);
        assert_eq!(before.active_power_scheme, after.active_power_scheme);
        assert_eq!(before.default_render_audio, after.default_render_audio);
        assert_eq!(before.toast_notifications, after.toast_notifications);
        match (&before.system_drive_space, &after.system_drive_space) {
            (
                ReadinessComponent::Known { value: before },
                ReadinessComponent::Known { value: after },
            ) => {
                assert_eq!(before.volume, after.volume);
                assert_eq!(before.total_bytes, after.total_bytes);
            }
            (before, after) => assert_eq!(before, after),
        }
        assert!(
            GAME_READINESS_CHECK_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify read-only readiness rollback")
                .verified
        );
    }

    #[test]
    fn apply_rejects_a_mismatched_observation_source() {
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
        let parameters = ActionParameters::GamesReadinessCheck {};
        let mut draft = GAME_READINESS_CHECK_ACTION
            .create_backup(&context, &parameters)
            .expect("create read-only readiness backup");
        draft.payload = BackupPayload::Observation(ObservationBackup {
            source: "different observation source".to_owned(),
        });
        let envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let error = GAME_READINESS_CHECK_ACTION
            .apply(&context, &parameters, &envelope)
            .expect_err("mismatched source must be rejected");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
    }
}

#[cfg(all(test, windows))]
mod readiness_items_tests {
    use super::*;
    use crate::compatibility::OsIdentity;

    /// ゲーム前の準備確認は、要約1行ではなく**項目ごと**に並べて出す（BRIEF §4）。
    /// UIはこの items をそのまま一覧表示するので、7項目そろうことを実機で確かめる。
    #[test]
    fn readiness_items_list_all_seven_checks() {
        let os = OsIdentity::from_test_build(26_200);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: uuid::Uuid::new_v4(),
            item_id: uuid::Uuid::new_v4(),
            is_elevated: false,
            observed_at_unix_ms: 0,
        };
        let state = GAME_READINESS_CHECK_ACTION
            .detect_current_state(&context, &ActionParameters::GamesReadinessCheck {})
            .expect("readiness detection runs on this machine");
        let Some(value) = state.known_value() else {
            panic!("readiness should report a known value");
        };
        let view = crate::presentation::observed_items_for_test(value);
        assert_eq!(view.len(), 7, "7項目そろう: {view:?}");
        for line in &view {
            assert!(line.contains(" — "), "項目名と値が並ぶ: {line}");
        }
        println!("readiness items:");
        for line in &view { println!("  {line}"); }
    }
}
