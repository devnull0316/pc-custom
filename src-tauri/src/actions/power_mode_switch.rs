use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, PowerModeChoice,
        RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, PowerModeBackup},
    windows::{
        read_ac_mode, read_dc_mode, read_effective_mode, write_ac_mode_raw, write_dc_mode_raw,
        EffectiveMode, PowerMode, PowerModeReading,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct PowerModeSwitchAction;
pub static POWER_MODE_SWITCH_ACTION: PowerModeSwitchAction = PowerModeSwitchAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::PowerModeSwitch,
    name: "電源モードを選ぶ",
    description: "電源接続時と電池使用時の電源モードを、まとめて選びます。速くなるとは言えません。選べない値があるPCもあり、その場合はそう表示して何も変更しません。",
    category: "power",
    tags: &["電源", "モード"],
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
    parameter_schema: "{\"mode\":\"best_efficiency|balanced|best_performance\"}",
    resource_keys: &["windows:power:user-configured-mode"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/powrprof/nf-powrprof-powergetuserconfiguredacpowermode",
        "https://learn.microsoft.com/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications",
    ],
    compatibility_key: "power.mode_switch.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低",
};

/// 画面に出す言葉。API 名も GUID も出さない。
fn mode_label(mode: PowerMode) -> &'static str {
    match mode {
        PowerMode::BestEfficiency => "電池優先",
        PowerMode::Balanced => "バランス",
        PowerMode::BestPerformance => "パフォーマンス優先",
    }
}

fn effective_label(mode: EffectiveMode) -> String {
    match mode {
        EffectiveMode::BatterySaver => "バッテリー節約".to_owned(),
        EffectiveMode::BetterBattery => "電池長持ち".to_owned(),
        EffectiveMode::Balanced => "バランス".to_owned(),
        EffectiveMode::HighPerformance => "高パフォーマンス".to_owned(),
        EffectiveMode::MaxPerformance => "最大パフォーマンス".to_owned(),
        EffectiveMode::GameMode => "ゲームモード".to_owned(),
        EffectiveMode::MixedReality => "Mixed Reality".to_owned(),
        // 知らない値を既知の名前に丸めない。
        EffectiveMode::Unrecognised(_) => "この表示では判別できないモード".to_owned(),
    }
}

fn reading_label(reading: PowerModeReading) -> Option<String> {
    match reading {
        PowerModeReading::Known(mode) => Some(mode_label(mode).to_owned()),
        PowerModeReading::Unrecognised(_) => Some("この表示では判別できないモード".to_owned()),
        PowerModeReading::Unavailable => None,
    }
}

/// 戻すときに必要なのは「見た目の名前」ではなく元のバイト列。
fn reading_bytes(reading: PowerModeReading) -> Option<[u8; 16]> {
    match reading {
        PowerModeReading::Known(mode) => Some(mode.bytes()),
        PowerModeReading::Unrecognised(raw) => Some(raw),
        PowerModeReading::Unavailable => None,
    }
}

fn choice_to_mode(choice: PowerModeChoice) -> PowerMode {
    match choice {
        PowerModeChoice::BestEfficiency => PowerMode::BestEfficiency,
        PowerModeChoice::Balanced => PowerMode::Balanced,
        PowerModeChoice::BestPerformance => PowerMode::BestPerformance,
    }
}

impl PowerModeSwitchAction {
    fn parameters(
        parameters: &ActionParameters,
        stage: ActionStage,
    ) -> ActionResult<PowerModeChoice> {
        match parameters {
            ActionParameters::PowerModeSwitch { mode } => Ok(*mode),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                stage,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn read_pair(stage: ActionStage) -> ActionResult<(PowerModeReading, PowerModeReading)> {
        let ac = read_ac_mode()
            .map_err(|error| map_windows_error(stage, "action.power_mode.read_failed", error))?;
        let dc = read_dc_mode()
            .map_err(|error| map_windows_error(stage, "action.power_mode.read_failed", error))?;
        Ok((ac, dc))
    }

    /// 実効モードは参考。読めなくても失敗にしない。
    ///
    /// **読めないことを「バランス」と読み替えない。** 読めなければ出さない。
    fn read_effective() -> Option<String> {
        read_effective_mode().ok().flatten().map(effective_label)
    }

    fn observed_state(
        context: &ActionContext<'_>,
        stage: ActionStage,
    ) -> ActionResult<DetectedState> {
        let (ac, dc) = Self::read_pair(stage)?;
        if ac == PowerModeReading::Unavailable && dc == PowerModeReading::Unavailable {
            return Ok(DetectedState::Unsupported {
                reason: "action.power_mode.not_available_here".to_owned(),
            });
        }
        Ok(DetectedState::Known {
            value: ObservedValue::PowerMode {
                requested_ac: reading_label(ac),
                requested_dc: reading_label(dc),
                effective: Self::read_effective(),
            },
            evidence: evidence(context, "PowerGetUserConfigured{AC,DC}PowerMode"),
        })
    }

    fn payload(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&PowerModeBackup> {
        let BackupPayload::PowerMode(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.power_mode.backup_kind_mismatch",
            ));
        };
        Ok(payload)
    }

    /// この PC が受け付けない値だったときの伝え方。
    ///
    /// 一般的な失敗として出すと「壊れた」と読まれる。
    /// **拒否は壊れたのではなく、この PC にその選択肢が無いということ。**
    fn rejected(stage: ActionStage) -> ActionError {
        ActionError::new(
            ActionErrorCode::InvalidParameters,
            stage,
            false,
            "action.power_mode.mode_not_offered_here",
        )
    }

    fn write_pair(
        ac: [u8; 16],
        dc: [u8; 16],
        stage: ActionStage,
        message_key: &'static str,
    ) -> ActionResult<()> {
        // どちらの書き込みも、失敗を返しながら値だけ変えた可能性を捨てない。
        // 先に両方を保存し、片方でも失敗したら両方を戻して読み直す。
        let before_ac =
            reading_bytes(read_ac_mode().map_err(|error| {
                map_windows_error(stage, "action.power_mode.read_failed", error)
            })?);
        let before_dc =
            reading_bytes(read_dc_mode().map_err(|error| {
                map_windows_error(stage, "action.power_mode.read_failed", error)
            })?);
        write_ac_mode_raw(ac).map_err(|error| {
            if error.os_code == Some(87) {
                Self::rejected(stage)
            } else {
                map_windows_error(stage, message_key, error)
            }
        })?;
        if let Err(error) = write_dc_mode_raw(dc) {
            // 片方だけ書けた状態で「何も変えていません」とは言えない。
            // 巻き戻しに成功したときだけ、元の失敗として返す。
            // 巻き戻しも失敗したら、それは復旧が要る事態であって、単なる失敗ではない。
            let compensated = match (before_ac, before_dc) {
                (Some(original_ac), Some(original_dc)) => {
                    let _ = write_ac_mode_raw(original_ac);
                    let _ = write_dc_mode_raw(original_dc);
                    read_ac_mode()
                        .ok()
                        .and_then(reading_bytes)
                        .zip(read_dc_mode().ok().and_then(reading_bytes))
                        .is_some_and(|(current_ac, current_dc)| {
                            current_ac == original_ac && current_dc == original_dc
                        })
                }
                _ => false,
            };
            if !compensated {
                return Err(ActionError::recovery_required(
                    stage,
                    "action.power_mode.partial_write_not_compensated",
                ));
            }
            return Err(if error.os_code == Some(87) {
                Self::rejected(stage)
            } else {
                map_windows_error(stage, message_key, error)
            });
        }
        Ok(())
    }
}

impl Action for PowerModeSwitchAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        Self::parameters(parameters, ActionStage::Detect)?;
        Self::observed_state(context, ActionStage::Detect)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::parameters(parameters, ActionStage::Validate)?;
        let (ac, dc) = Self::read_pair(ActionStage::Validate)?;
        if ac == PowerModeReading::Unavailable || dc == PowerModeReading::Unavailable {
            return Err(ActionError::new(
                ActionErrorCode::UnsupportedPlatform,
                ActionStage::Validate,
                false,
                "action.power_mode.not_available_here",
            ));
        }
        // 選んだ値をこの PC が受け付けるかは、書いてみるまで分からない。
        // 事前に確かめる公開手段が無いので、ここでは「選べます」と約束しない。
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        let choice = Self::parameters(parameters, ActionStage::Backup)?;
        let (ac, dc) = Self::read_pair(ActionStage::Backup)?;
        let (Some(original_ac), Some(original_dc)) = (reading_bytes(ac), reading_bytes(dc)) else {
            return Err(ActionError::new(
                ActionErrorCode::UnsupportedPlatform,
                ActionStage::Backup,
                false,
                "action.power_mode.not_available_here",
            ));
        };
        let intended = choice_to_mode(choice);
        Ok(BackupDraft {
            precondition_fingerprint: crate::backup::Fingerprint::of_bytes(
                &[original_ac, original_dc].concat(),
            ),
            intended_fingerprint: crate::backup::Fingerprint::of_bytes(
                &[intended.bytes(), intended.bytes()].concat(),
            ),
            payload: BackupPayload::PowerMode(PowerModeBackup {
                original_ac,
                original_dc,
                intended: choice,
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
        Self::parameters(parameters, ActionStage::Apply)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let payload = Self::payload(envelope, ActionStage::Apply)?;
        let (ac, dc) = Self::read_pair(ActionStage::Apply)?;
        if reading_bytes(ac) != Some(payload.original_ac)
            || reading_bytes(dc) != Some(payload.original_dc)
        {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Apply,
                false,
                "action.apply.stale_preview",
            ));
        }
        let intended = choice_to_mode(payload.intended).bytes();
        Self::write_pair(
            intended,
            intended,
            ActionStage::Apply,
            "action.power_mode.apply_failed",
        )?;
        let state = Self::observed_state(context, ActionStage::Apply)?;
        let (applied_ac, applied_dc) = Self::read_pair(ActionStage::Apply)?;
        Ok(AppliedEvidence {
            state,
            applied_fingerprint: crate::backup::Fingerprint::of_bytes(
                &[
                    reading_bytes(applied_ac).unwrap_or_default(),
                    reading_bytes(applied_dc).unwrap_or_default(),
                ]
                .concat(),
            ),
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        Self::parameters(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        let (ac, dc) = Self::read_pair(ActionStage::VerifyApplied)?;
        let intended = choice_to_mode(payload.intended).bytes();
        Ok(Verification {
            verified: reading_bytes(ac) == Some(intended) && reading_bytes(dc) == Some(intended),
            observed: Self::observed_state(context, ActionStage::VerifyApplied)?,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Rollback)?;
        Self::parameters(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;
        let (ac, dc) = Self::read_pair(ActionStage::Rollback)?;
        let (current_ac, current_dc) = (reading_bytes(ac), reading_bytes(dc));
        let intended = choice_to_mode(payload.intended).bytes();

        if current_ac == Some(payload.original_ac) && current_dc == Some(payload.original_dc) {
            // すでに元の値。何も書かない。
        } else if current_ac == Some(intended) && current_dc == Some(intended) {
            Self::write_pair(
                payload.original_ac,
                payload.original_dc,
                ActionStage::Rollback,
                "action.power_mode.rollback_failed",
            )?;
        } else {
            // 適用値でも元の値でもない。誰かが変えている。上書きしない。
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }

        let (restored_ac, restored_dc) = Self::read_pair(ActionStage::Rollback)?;
        Ok(RollbackEvidence {
            state: Self::observed_state(context, ActionStage::Rollback)?,
            restored_fingerprint: crate::backup::Fingerprint::of_bytes(
                &[
                    reading_bytes(restored_ac).unwrap_or_default(),
                    reading_bytes(restored_dc).unwrap_or_default(),
                ]
                .concat(),
            ),
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        Self::parameters(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        let (ac, dc) = Self::read_pair(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: reading_bytes(ac) == Some(payload.original_ac)
                && reading_bytes(dc) == Some(payload.original_dc),
            observed: Self::observed_state(context, ActionStage::VerifyRolledBack)?,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let choice = Self::parameters(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: format!(
                "電源接続時と電池使用時の電源モードを、どちらも「{}」にします。速くなるとは言えません。",
                mode_label(choice_to_mode(choice))
            ),
            method: "Windowsが公開している電源モードの設定".to_owned(),
            resources: vec![
                "電源接続時の電源モード".to_owned(),
                "電池使用時の電源モード".to_owned(),
            ],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "変更前の2つの値をそのまま保存し、同じ値へ戻します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.power_mode.mode_not_offered_here",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_maps_to_one_mode_and_back() {
        for (choice, mode) in [
            (PowerModeChoice::BestEfficiency, PowerMode::BestEfficiency),
            (PowerModeChoice::Balanced, PowerMode::Balanced),
            (PowerModeChoice::BestPerformance, PowerMode::BestPerformance),
        ] {
            assert_eq!(choice_to_mode(choice), mode);
        }
    }

    #[test]
    fn an_unreadable_supply_is_not_labelled_as_a_mode() {
        assert_eq!(reading_label(PowerModeReading::Unavailable), None);
        assert_eq!(reading_bytes(PowerModeReading::Unavailable), None);
    }

    #[test]
    fn an_unknown_guid_keeps_its_bytes_for_rollback() {
        // 知らない値でも、戻すときに書き戻せるようバイト列は残す。
        let mut raw = [0u8; 16];
        raw[3] = 0x7f;
        assert_eq!(
            reading_bytes(PowerModeReading::Unrecognised(raw)),
            Some(raw)
        );
        // ただし名前は付けない。
        assert_eq!(
            reading_label(PowerModeReading::Unrecognised(raw)).as_deref(),
            Some("この表示では判別できないモード")
        );
    }

    /// Action の一往復を実機で通す。
    ///
    /// 単体テストは対応表しか見ていない。**書けたか、戻ったかは Windows に聞かないと分からない。**
    /// 触るのは利用者の設定なので、panic しても戻るよう `Drop` で戻す。
    struct RestorePowerMode {
        ac: [u8; 16],
        dc: [u8; 16],
    }

    impl Drop for RestorePowerMode {
        fn drop(&mut self) {
            let _ = write_ac_mode_raw(self.ac);
            let _ = write_dc_mode_raw(self.dc);
        }
    }

    #[test]
    #[ignore = "実機の電源モードを一時的に変える"]
    fn reports_apply_and_rollback_when_both_supply_modes_are_readable() {
        use crate::{backup::BackupEnvelope, compatibility::OsIdentity};
        use uuid::Uuid;

        let before_ac = read_ac_mode().expect("read ac");
        let before_dc = read_dc_mode().expect("read dc");
        let (Some(original_ac), Some(original_dc)) =
            (reading_bytes(before_ac), reading_bytes(before_dc))
        else {
            println!(
                "EVIDENCE: power_mode_action measured=false reason=この環境では両方の値を読めない"
            );
            return;
        };
        let _restore = RestorePowerMode {
            ac: original_ac,
            dc: original_dc,
        };
        println!(
            "EVIDENCE: power_mode_action measured=true before ac={before_ac:?} dc={before_dc:?}"
        );

        // 今と違い、かつこの PC が受け付ける値を選ぶ。
        // 受け付けない値で試すと、拒否の経路を測ることになって往復が測れない。
        let choice = [PowerModeChoice::Balanced, PowerModeChoice::BestPerformance]
            .into_iter()
            .find(|candidate| {
                let mode = choice_to_mode(*candidate);
                before_ac != PowerModeReading::Known(mode)
                    && write_ac_mode_raw(mode.bytes()).is_ok()
            })
            .expect("受け付ける値が1つはあるはず");
        // 探索で AC を書いてしまったので、往復の出発点へ戻す。
        write_ac_mode_raw(original_ac).expect("reset ac before the round trip");

        let os = OsIdentity::load().expect("load real Windows identity");
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: os.observed_at_unix_ms,
            is_elevated: false,
        };
        let parameters = ActionParameters::PowerModeSwitch { mode: choice };
        let draft = POWER_MODE_SWITCH_ACTION
            .create_backup(&context, &parameters)
            .expect("create power mode backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );

        let applied = POWER_MODE_SWITCH_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("apply power mode");
        envelope.record_applied(applied.applied_fingerprint);

        // Action の戻り値ではなく、Windows へもう一度問い合わせる。
        let after_ac = read_ac_mode().expect("re-read ac");
        let after_dc = read_dc_mode().expect("re-read dc");
        println!("EVIDENCE: power_mode_action applied={choice:?} ac={after_ac:?} dc={after_dc:?}");
        let intended = choice_to_mode(choice);
        assert_eq!(after_ac, PowerModeReading::Known(intended));
        assert_eq!(after_dc, PowerModeReading::Known(intended));
        assert!(
            POWER_MODE_SWITCH_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify applied")
                .verified
        );

        POWER_MODE_SWITCH_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("rollback power mode");
        // 三度目の問い合わせ。
        let restored_ac = read_ac_mode().expect("re-read ac after rollback");
        let restored_dc = read_dc_mode().expect("re-read dc after rollback");
        println!("EVIDENCE: power_mode_action restored ac={restored_ac:?} dc={restored_dc:?}");
        assert_eq!(restored_ac, before_ac, "AC を変更前の値へ戻す");
        assert_eq!(restored_dc, before_dc, "DC を変更前の値へ戻す");
        assert!(
            POWER_MODE_SWITCH_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify rolled back")
                .verified
        );
    }

    #[test]
    fn the_wording_never_promises_speed() {
        let explanation = POWER_MODE_SWITCH_ACTION
            .explain_changes(&ActionParameters::PowerModeSwitch {
                mode: PowerModeChoice::BestPerformance,
            })
            .expect("explain");
        // 禁じたいのは効果の約束であって、効果を否定する文ではない。
        for promise in ["速くなります", "高速化", "FPSが上がる", "軽くなります"]
        {
            assert!(
                !explanation.result.contains(promise),
                "効果を約束する言葉が入っている: {promise}"
            );
        }
        // 否定は必ず入れる。黙って何も言わないのは約束したのと近い。
        assert!(explanation.result.contains("速くなるとは言えません"));
        assert!(METADATA.description.contains("速くなるとは言えません"));
    }
}
