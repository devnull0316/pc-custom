//! ウィンドウの色（DWM colorization）を、決められた色から選んで変える。
//!
//! この Action だけは「反映されること」を実測で確認してから実装した。
//! `ui_probe` の往復検証で、`ColorizationColor` への書き込みは
//! `DwmGetColorizationColor` が返す**実効色**を実際に変えることを確認している
//! （タスクバー配置は同じ手順で反映されず、そちらは設定アプリへの案内に留めた）。
//!
//! 安全契約:
//! - 変更するのは固定の2値（`ColorizationColor` と `ColorizationAfterglow`）だけ。
//! - 色は下の固定プリセットのみ。任意のARGBを受け取らない。
//! - 2値は1トランザクションとして扱い、片方だけ変わった状態を残さない。
//! - 元の値・型・有無を型付きで退避し、第三者変更時は上書きしない。

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionId, ActionKind, ActionMetadata, ActionParameters,
        ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence, ChangeExplanation,
        DetectedState, MethodClass, ObservedValue, RollbackEvidence, TroubleshootingStep,
        ValidationReport, Verification, WindowColorPreset, WindowsReleaseFamily,
    },
    backup::{
        classify_registry_backup, prepare_registry_backup, read_registry_state,
        restore_registry_backup, verify_registry_backup_restored, BackupDraft, BackupEnvelope,
        BackupPayload, CompositeBackup, Fingerprint, RegistryBackup, RegistryClassification,
        RegistryRestoreOutcome, RegistryTarget,
    },
    windows::{notify_theme_changed, system_accent_color, write_raw_value},
};

use super::common::{
    dword_bytes, evidence, map_windows_error, validate_backup, validate_backup_for_apply,
    validate_base, REG_DWORD_TYPE,
};

const DWM_SUBKEY: &str = r"Software\Microsoft\Windows\DWM";
const COLOR_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(DWM_SUBKEY, "ColorizationColor");
const AFTERGLOW_TARGET: RegistryTarget =
    RegistryTarget::current_user_64(DWM_SUBKEY, "ColorizationAfterglow");

/// 固定プリセット（ARGB）。Windows既定と同じ不透明度 0xC4 を使う。
const fn preset_argb(preset: WindowColorPreset) -> u32 {
    match preset {
        WindowColorPreset::WindowsBlue => 0xC400_78D4,
        WindowColorPreset::Teal => 0xC400_B7C3,
        WindowColorPreset::Purple => 0xC474_4DA9,
        WindowColorPreset::Green => 0xC410_893E,
        WindowColorPreset::Amber => 0xC4FF_B900,
        WindowColorPreset::Red => 0xC4E8_1123,
        WindowColorPreset::Graphite => 0xC44C_4A48,
    }
}

const fn preset_label(preset: WindowColorPreset) -> &'static str {
    match preset {
        WindowColorPreset::WindowsBlue => "Windowsの青",
        WindowColorPreset::Teal => "青緑",
        WindowColorPreset::Purple => "紫",
        WindowColorPreset::Green => "緑",
        WindowColorPreset::Amber => "橙",
        WindowColorPreset::Red => "赤",
        WindowColorPreset::Graphite => "グラファイト",
    }
}

pub struct WindowColorAction;
pub static WINDOW_COLOR_ACTION: WindowColorAction = WindowColorAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::AppearanceWindowColor,
    name: "ウィンドウの色を変える",
    description:
        "タイトルバーなどに使われる色を、決められた色から選んで変えます。元の色は正確に戻せます。",
    category: "appearance",
    tags: &["見た目", "色", "DWM"],
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
    parameter_schema: r#"{"color":"windows_blue|teal|purple|green|amber|red|graphite"}"#,
    resource_keys: &[
        "registry:hkcu:64:software/microsoft/windows/dwm:colorizationcolor",
        "registry:hkcu:64:software/microsoft/windows/dwm:colorizationafterglow",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/dwmapi/nf-dwmapi-dwmgetcolorizationcolor",
    ],
    compatibility_key: "appearance.window_color.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "中。色の保存先が変わった場合はAction固有の再確認が必要です。",
};

impl WindowColorAction {
    fn preset(parameters: &ActionParameters) -> ActionResult<WindowColorPreset> {
        match parameters {
            ActionParameters::AppearanceWindowColor { color } => Ok(*color),
            _ => Err(ActionError::new(
                crate::action::ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn composite(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&CompositeBackup> {
        let BackupPayload::Composite(composite) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.window_color.backup_kind_mismatch",
            ));
        };
        if composite.registry_entries.len() != 2
            || composite.registry_entries[0].location != COLOR_TARGET.location()
            || composite.registry_entries[1].location != AFTERGLOW_TARGET.location()
        {
            return Err(ActionError::recovery_required(
                stage,
                "action.window_color.backup_target_mismatch",
            ));
        }
        Ok(composite)
    }

    /// 現在の実効色。保存値ではなくDWMが実際に使っている色を返す。
    fn state(context: &ActionContext<'_>) -> ActionResult<DetectedState> {
        match system_accent_color() {
            Ok(colour) => Ok(DetectedState::Known {
                value: ObservedValue::AccentColor {
                    hex: format!("#{:02X}{:02X}{:02X}", colour.red, colour.green, colour.blue),
                    opaque_blend: colour.opaque_blend,
                },
                evidence: evidence(context, "DwmGetColorizationColor effective colour"),
            }),
            Err(_) => Ok(DetectedState::Unknown {
                reason: "Windowsから現在の色を取得できませんでした。".to_owned(),
            }),
        }
    }

    /// 2値を順に書き込む。途中で失敗したら、書けた分を元へ戻してから中断する。
    fn apply_entries(entries: &[RegistryBackup]) -> ActionResult<()> {
        // written は「書き込みに成功した件数」で、ループ位置ではない。理由は color_mode と同じ。
        let mut written = 0usize;
        #[allow(clippy::explicit_counter_loop)]
        for entry in entries {
            let current = read_registry_state(&entry.location).map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.window_color.precondition_read_failed",
                    error,
                )
            })?;
            if current != entry.original {
                if !Self::compensate(&entries[..written]) {
                    return Err(ActionError::recovery_required(
                        ActionStage::Apply,
                        "action.window_color.partial_write_not_compensated",
                    ));
                }
                return Err(ActionError::new(
                    crate::action::ActionErrorCode::ExternalConflict,
                    ActionStage::Apply,
                    false,
                    "action.apply.stale_preview",
                ));
            }
            if let Err(error) =
                write_raw_value(&entry.location, entry.intended_type, &entry.intended_raw)
            {
                if !Self::compensate(&entries[..written]) {
                    return Err(ActionError::recovery_required(
                        ActionStage::Apply,
                        "action.window_color.partial_write_not_compensated",
                    ));
                }
                return Err(map_windows_error(
                    ActionStage::Apply,
                    "action.window_color.apply_failed",
                    error,
                ));
            }
            written += 1;
        }
        Ok(())
    }

    /// 途中まで書いた分を巻き戻す。**巻き戻せたかを返す。**
    ///
    /// 以前は `let _ =` で結果を捨てていた。巻き戻しに失敗しても
    /// 呼び出し側には元のエラーだけが返り、**「適用に失敗した」＝「何も変わっていない」**
    /// と読める。実際には片方のレジストリ値が変わったまま残る。
    ///
    /// 書き戻したあと読み直して、元の値に戻っていることまで確かめる。
    /// 確かめられなければ、それは失敗ではなく復旧が要る事態。
    fn compensate(applied: &[RegistryBackup]) -> bool {
        let mut all_restored = true;
        for entry in applied.iter().rev() {
            if restore_registry_backup(entry).is_err() {
                all_restored = false;
                continue;
            }
            match verify_registry_backup_restored(entry) {
                Ok(true) => {}
                _ => all_restored = false,
            }
        }
        all_restored
    }
}

impl Action for WindowColorAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let _ = Self::preset(parameters)?;
        Self::state(context)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let _ = Self::preset(parameters)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        let argb = preset_argb(Self::preset(parameters)?);
        let mut entries = Vec::with_capacity(2);
        for target in [COLOR_TARGET, AFTERGLOW_TARGET] {
            entries.push(
                prepare_registry_backup(
                    target,
                    REG_DWORD_TYPE,
                    dword_bytes(argb),
                    METADATA.action_version,
                    context.os_identity.base_build,
                )
                .map_err(|error| {
                    map_windows_error(
                        ActionStage::Backup,
                        "action.window_color.backup_failed",
                        error,
                    )
                })?,
            );
        }
        let precondition_fingerprint = Fingerprint::of_parts([
            entries[0]
                .original
                .fingerprint(&entries[0].location)
                .0
                .as_slice(),
            entries[1]
                .original
                .fingerprint(&entries[1].location)
                .0
                .as_slice(),
        ]);
        let intended_fingerprint = Fingerprint::of_parts([
            entries[0]
                .intended_state()
                .fingerprint(&entries[0].location)
                .0
                .as_slice(),
            entries[1]
                .intended_state()
                .fingerprint(&entries[1].location)
                .0
                .as_slice(),
        ]);
        Ok(BackupDraft {
            precondition_fingerprint,
            intended_fingerprint,
            payload: BackupPayload::Composite(CompositeBackup {
                registry_entries: entries,
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
        let desired = dword_bytes(preset_argb(Self::preset(parameters)?));
        let composite = Self::composite(envelope, ActionStage::Apply)?;
        if composite
            .registry_entries
            .iter()
            .any(|entry| entry.intended_type != REG_DWORD_TYPE || entry.intended_raw != desired)
        {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.window_color.backup_parameter_mismatch",
            ));
        }
        Self::apply_entries(&composite.registry_entries)?;
        let _broadcast = notify_theme_changed();
        let applied_fingerprint = Fingerprint::of_parts([
            composite.registry_entries[0]
                .applied_state()
                .fingerprint(&composite.registry_entries[0].location)
                .0
                .as_slice(),
            composite.registry_entries[1]
                .applied_state()
                .fingerprint(&composite.registry_entries[1].location)
                .0
                .as_slice(),
        ]);
        Ok(AppliedEvidence {
            applied_fingerprint,
            state: Self::state(context)?,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        let composite = Self::composite(envelope, ActionStage::VerifyApplied)?;
        let mut verified = true;
        for entry in &composite.registry_entries {
            let current = read_registry_state(&entry.location).map_err(|error| {
                map_windows_error(
                    ActionStage::VerifyApplied,
                    "action.window_color.verify_failed",
                    error,
                )
            })?;
            verified &= current == entry.applied_state();
        }
        let _ = parameters;
        Ok(Verification {
            verified,
            observed: Self::state(context)?,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let composite = Self::composite(envelope, ActionStage::Rollback)?;

        // **1バイトも書く前に、全部の値を先に見る。**
        //
        // 以前はここで書きながら判定していた。2つ目が第三者に変えられていると、
        // 1つ目だけ元に戻したところでエラーを返し、**半分だけ戻った状態が残った。**
        // 戻せると言っている機能が、途中で止まって混ざった状態を作るのが一番悪い。
        //
        // 同じ形の `color_mode` は最初からこうしている。片方だけ違っていた。
        let mut classifications = Vec::with_capacity(composite.registry_entries.len());
        for entry in &composite.registry_entries {
            classifications.push(classify_registry_backup(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.window_color.rollback_preflight_failed",
                    error,
                )
            })?);
        }
        if classifications.contains(&RegistryClassification::Third) {
            // 1つでも第三者の値なら、**何も書かずに**止める。
            return Err(ActionError::new(
                crate::action::ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }

        for entry in composite.registry_entries.iter().rev() {
            match restore_registry_backup(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.window_color.rollback_failed",
                    error,
                )
            })? {
                RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal => {}
                RegistryRestoreOutcome::RestoredValueKeyRetained => {
                    return Err(ActionError::recovery_required(
                        ActionStage::Rollback,
                        "action.window_color.rollback_key_retained",
                    ));
                }
                RegistryRestoreOutcome::ExternalConflict => {
                    return Err(ActionError::new(
                        crate::action::ActionErrorCode::ExternalConflict,
                        ActionStage::Rollback,
                        false,
                        "action.rollback.external_change_detected",
                    ));
                }
            }
        }
        let _broadcast = notify_theme_changed();
        Ok(RollbackEvidence {
            restored_fingerprint: envelope.precondition_fingerprint,
            state: Self::state(context)?,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        let composite = Self::composite(envelope, ActionStage::VerifyRolledBack)?;
        let mut verified = true;
        for entry in &composite.registry_entries {
            verified &= verify_registry_backup_restored(entry).map_err(|error| {
                map_windows_error(
                    ActionStage::VerifyRolledBack,
                    "action.window_color.rollback_verify_failed",
                    error,
                )
            })?;
        }
        let _ = parameters;
        Ok(Verification {
            verified,
            observed: Self::state(context)?,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let preset = Self::preset(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: format!("ウィンドウの色を「{}」にします。", preset_label(preset)),
            method: "HKCU DWMの色2値（型付きraw backup）＋テーマ変更通知".to_owned(),
            resources: METADATA
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "元の色・型・有無へ正確に戻します。".to_owned(),
        })
    }

    fn troubleshooting(
        &self,
        _code: crate::action::ActionErrorCode,
    ) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.window_color.open_official_settings_if_refresh_is_delayed",
            opens_official_settings: true,
        }]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn presets_are_fixed_and_opaque_enough_to_be_visible() {
        for preset in [
            WindowColorPreset::WindowsBlue,
            WindowColorPreset::Teal,
            WindowColorPreset::Purple,
            WindowColorPreset::Green,
            WindowColorPreset::Amber,
            WindowColorPreset::Red,
            WindowColorPreset::Graphite,
        ] {
            let argb = preset_argb(preset);
            assert_eq!(argb >> 24, 0xC4, "Windows既定と同じ不透明度を使う");
            assert!(!preset_label(preset).is_empty());
        }
    }

    #[test]
    fn every_preset_is_distinct() {
        let mut seen = Vec::new();
        for preset in [
            WindowColorPreset::WindowsBlue,
            WindowColorPreset::Teal,
            WindowColorPreset::Purple,
            WindowColorPreset::Green,
            WindowColorPreset::Amber,
            WindowColorPreset::Red,
            WindowColorPreset::Graphite,
        ] {
            let argb = preset_argb(preset);
            assert!(!seen.contains(&argb), "色が重複していない: {argb:08X}");
            seen.push(argb);
        }
    }

    #[test]
    fn wrong_parameters_are_rejected_before_touching_the_registry() {
        let result = WindowColorAction::preset(&ActionParameters::PowerActiveSchemeCheck {});
        assert!(result.is_err(), "別Actionのパラメータは受け付けない");
    }

    /// 実機での往復。色を変えて実効色の変化を確認し、元の色へ正確に戻す。
    #[test]
    #[ignore = "実機のウィンドウ色を一時的に変更する"]
    fn real_round_trip_changes_and_restores_the_window_colour() {
        use crate::backup::read_registry_state;
        use std::{thread::sleep, time::Duration};

        let before_reg = read_registry_state(&COLOR_TARGET.location()).expect("read colour");
        let before = system_accent_color().expect("effective colour");
        println!(
            "before #{:02X}{:02X}{:02X}",
            before.red, before.green, before.blue
        );

        // 元の色から離れたプリセットを選ぶ。
        let preset = if before.red > 128 {
            WindowColorPreset::Teal
        } else {
            WindowColorPreset::Amber
        };
        let argb = preset_argb(preset);
        let mut entries = Vec::new();
        for target in [COLOR_TARGET, AFTERGLOW_TARGET] {
            entries.push(
                prepare_registry_backup(target, REG_DWORD_TYPE, dword_bytes(argb), 1, 26_200)
                    .expect("prepare"),
            );
        }
        WindowColorAction::apply_entries(&entries).expect("apply both values");
        let _ = notify_theme_changed();

        let mut changed = None;
        for _ in 0..25 {
            sleep(Duration::from_millis(200));
            if let Ok(now) = system_accent_color() {
                if now.red != before.red || now.green != before.green || now.blue != before.blue {
                    changed = Some(now);
                    break;
                }
            }
        }

        for entry in entries.iter().rev() {
            restore_registry_backup(entry).expect("restore");
        }
        let _ = notify_theme_changed();

        match changed {
            Some(now) => println!(
                "applied {} -> effective #{:02X}{:02X}{:02X}",
                preset_label(preset),
                now.red,
                now.green,
                now.blue
            ),
            None => println!("実効色の変化を検出できなかった"),
        }
        let after_reg = read_registry_state(&COLOR_TARGET.location()).expect("read back");
        assert_eq!(after_reg, before_reg, "元の値・型・有無へ正確に戻す");
        assert!(changed.is_some(), "この設定は実際に反映される");
    }
}
