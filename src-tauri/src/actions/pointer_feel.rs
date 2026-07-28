use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, PointerFeelBackup},
    windows::{read_pointer_feel, replace_pointer_feel, PointerFeel},
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct PointerFeelAction;
pub static POINTER_FEEL_ACTION: PointerFeelAction = PointerFeelAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::InputPointerFeel,
    name: "マウスの動き方を切り替える",
    description: "マウスポインターの加速を、このモードの間だけ入り切りします。ゲームが独自にマウスを読み取っている場合、この設定は届きません。狙いが当たるようになるとは言えません。",
    category: "games",
    tags: &["ゲーム", "マウス", "一時的"],
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
    parameter_schema: "{\"acceleration\":\"bool\"}",
    resource_keys: &["windows:user-input:pointer-feel"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfow",
        "https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-rawmouse",
    ],
    compatibility_key: "input.pointer_feel.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: true,
    windows_update_impact: "低",
};

impl PointerFeelAction {
    fn parameters(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<bool> {
        match parameters {
            ActionParameters::InputPointerFeel { acceleration } => Ok(*acceleration),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                stage,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn read(stage: ActionStage) -> ActionResult<PointerFeel> {
        read_pointer_feel()
            .map_err(|error| map_windows_error(stage, "action.pointer_feel.read_failed", error))
    }

    fn observed_state(context: &ActionContext<'_>, current: PointerFeel) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::PointerFeel {
                acceleration_enabled: current.acceleration_enabled(),
                speed: current.speed,
            },
            evidence: evidence(context, "SystemParametersInfoW pointer settings"),
        }
    }

    fn intended(current: PointerFeel, acceleration: bool) -> PointerFeel {
        if acceleration {
            current.with_acceleration()
        } else {
            current.without_acceleration()
        }
    }

    fn payload(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&PointerFeelBackup> {
        let BackupPayload::PointerFeel(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.pointer_feel.backup_kind_mismatch",
            ));
        };
        Ok(payload)
    }
}

impl Action for PointerFeelAction {
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
        Ok(Self::observed_state(
            context,
            Self::read(ActionStage::Detect)?,
        ))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::parameters(parameters, ActionStage::Validate)?;
        Self::read(ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        let acceleration = Self::parameters(parameters, ActionStage::Backup)?;
        let original = Self::read(ActionStage::Backup)?;
        let intended = Self::intended(original, acceleration);
        Ok(BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: intended.fingerprint(),
            payload: BackupPayload::PointerFeel(PointerFeelBackup { original, intended }),
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
        // 現在値が控えと違えば書かない。判定は `replace_pointer_feel` の中でも行う。
        replace_pointer_feel(payload.original, payload.intended).map_err(|error| {
            map_windows_error(
                ActionStage::Apply,
                "action.pointer_feel.apply_failed",
                error,
            )
        })?;
        let applied = Self::read(ActionStage::Apply)?;
        Ok(AppliedEvidence {
            state: Self::observed_state(context, applied),
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
        Self::parameters(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        let current = Self::read(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: current == payload.intended,
            observed: Self::observed_state(context, current),
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
        let current = Self::read(ActionStage::Rollback)?;
        if current == payload.original {
            // すでに元の値。何も書かない。
        } else if current == payload.intended {
            replace_pointer_feel(payload.intended, payload.original).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.pointer_feel.rollback_failed",
                    error,
                )
            })?;
        } else {
            // 適用値でも元の値でもない。別のマウスユーティリティが触っている。上書きしない。
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }
        let restored = Self::read(ActionStage::Rollback)?;
        Ok(RollbackEvidence {
            state: Self::observed_state(context, restored),
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
        Self::parameters(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        let current = Self::read(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current == payload.original,
            observed: Self::observed_state(context, current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let acceleration = Self::parameters(parameters, ActionStage::Validate)?;
        let result = if acceleration {
            "マウスポインターの加速を入れます。速く動かすほど大きく進むようになります。\
             加速の効きはじめる値が未設定のときは、Windowsの既定値を入れます。"
                .to_owned()
        } else {
            "マウスポインターの加速を切ります。動かした距離とポインターの進む量が一定になります。"
                .to_owned()
        };
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result,
            method: "Windowsが公開しているポインター設定".to_owned(),
            resources: vec!["マウスポインターの加速".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope:
                "加速・2つのしきい値・速さの4つを保存し、他の変更が無い場合だけ元へ戻します。"
                    .to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.pointer_feel.other_mouse_software",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feel(threshold_one: i32, threshold_two: i32, acceleration: i32, speed: i32) -> PointerFeel {
        PointerFeel {
            threshold_one,
            threshold_two,
            acceleration,
            speed,
        }
    }

    #[test]
    fn turning_it_off_changes_only_the_acceleration() {
        let original = feel(6, 10, 1, 12);
        let intended = PointerFeelAction::intended(original, false);
        assert_eq!(intended, feel(6, 10, 0, 12));
    }

    #[test]
    fn the_speed_is_never_changed_by_this_action() {
        // 速さは保存も復元もするが、こちらから動かすことはない。
        for original in [feel(0, 0, 0, 1), feel(6, 10, 1, 20), feel(3, 7, 0, 11)] {
            for acceleration in [true, false] {
                assert_eq!(
                    PointerFeelAction::intended(original, acceleration).speed,
                    original.speed
                );
            }
        }
    }

    #[test]
    fn the_wording_never_claims_better_aim() {
        for acceleration in [true, false] {
            let explanation = POINTER_FEEL_ACTION
                .explain_changes(&ActionParameters::InputPointerFeel { acceleration })
                .expect("explain");
            for promise in ["狙いが", "エイム", "上手く", "FPSが上がる", "有利"] {
                assert!(
                    !explanation.result.contains(promise),
                    "効果を約束する言葉が入っている: {promise}"
                );
            }
        }
        // 届かない相手がいることは、説明のどこかに必ず書く。
        assert!(METADATA.description.contains("届きません"));
        assert!(METADATA
            .description
            .contains("当たるようになるとは言えません"));
    }

    /// Action の一往復を実機で通す。
    ///
    /// 単体テストは対応表しか見ていない。**書けたか、戻ったかは Windows に聞く。**
    struct RestorePointerFeel(PointerFeel);

    impl Drop for RestorePointerFeel {
        fn drop(&mut self) {
            if let Ok(current) = read_pointer_feel() {
                let _ = replace_pointer_feel(current, self.0);
            }
        }
    }

    #[test]
    #[ignore = "実機のマウス設定を一時的に変える"]
    fn applying_and_rolling_back_is_observed_through_windows_each_time() {
        use crate::compatibility::OsIdentity;
        use uuid::Uuid;

        let before = read_pointer_feel().expect("read pointer settings");
        let _restore = RestorePointerFeel(before);
        println!("EVIDENCE: pointer_feel_action before={before:?}");

        // 今と違う状態になる向きを選ぶ。同じ値を書いても何も測れない。
        let acceleration = !before.acceleration_enabled();
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
        let parameters = ActionParameters::InputPointerFeel { acceleration };
        let draft = POINTER_FEEL_ACTION
            .create_backup(&context, &parameters)
            .expect("create pointer settings backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );

        let applied = POINTER_FEEL_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("apply pointer settings");
        envelope.record_applied(applied.applied_fingerprint);

        // Action の戻り値ではなく、Windows へもう一度問い合わせる。
        let after = read_pointer_feel().expect("re-read pointer settings");
        println!("EVIDENCE: pointer_feel_action acceleration={acceleration} after={after:?}");
        assert_eq!(after.acceleration_enabled(), acceleration);
        assert_eq!(after.speed, before.speed, "速さはこちらから動かさない");
        assert!(
            POINTER_FEEL_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify applied")
                .verified
        );

        POINTER_FEEL_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("rollback pointer settings");
        // 三度目の問い合わせ。
        let restored = read_pointer_feel().expect("re-read after rollback");
        println!("EVIDENCE: pointer_feel_action restored={restored:?}");
        assert_eq!(restored, before, "4つの値すべてを元へ戻す");
        assert!(
            POINTER_FEEL_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify rolled back")
                .verified
        );
    }

    #[test]
    fn a_backup_belonging_to_another_action_is_refused() {
        // 中身が別 Action のバックアップなら、読み出しの時点で止まること。
        let draft = BackupDraft {
            precondition_fingerprint: crate::backup::Fingerprint::of_bytes(b"a"),
            intended_fingerprint: crate::backup::Fingerprint::of_bytes(b"b"),
            payload: BackupPayload::PowerMode(crate::backup::PowerModeBackup {
                original_ac: [0; 16],
                original_dc: [0; 16],
                intended: crate::action::PowerModeChoice::Balanced,
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
        let error = PointerFeelAction::payload(&envelope, ActionStage::Rollback)
            .expect_err("別 Action のバックアップを受け入れない");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
    }
}
