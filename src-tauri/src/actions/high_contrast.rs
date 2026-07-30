use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, HighContrastBackup},
    windows::{read_high_contrast, replace_high_contrast, HighContrastSnapshot},
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct HighContrastAction;
pub static HIGH_CONTRAST_ACTION: HighContrastAction = HighContrastAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::AppearanceHighContrastTrial,
    name: "コントラストテーマを30秒だけ試す",
    description:
        "Windowsの色と文字の組み合わせを30秒だけ切り替えます。時間切れか「元に戻す」で、開始前の値へ戻します。既に使われている場合は出番なしです。",
    category: "appearance",
    tags: &["見た目", "30秒", "自動復元"],
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
    parameter_schema: "{}",
    resource_keys: &["windows:user-appearance:high-contrast"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-highcontrastw",
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfow",
    ],
    compatibility_key: "appearance.high_contrast_trial.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低",
};

impl HighContrastAction {
    fn parameters(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<()> {
        if matches!(parameters, ActionParameters::AppearanceHighContrastTrial {}) {
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

    fn read(stage: ActionStage) -> ActionResult<HighContrastSnapshot> {
        read_high_contrast()
            .map_err(|error| map_windows_error(stage, "action.high_contrast.read_failed", error))
    }

    fn observed_state(
        context: &ActionContext<'_>,
        current: &HighContrastSnapshot,
    ) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::HighContrast {
                enabled: current.enabled(),
                structure_size: current.structure_size,
                flags: current.flags,
                scheme: match &current.scheme {
                    crate::windows::HighContrastScheme::Null => None,
                    crate::windows::HighContrastScheme::Name(name) => Some(name.clone()),
                },
            },
            evidence: evidence(context, "SystemParametersInfoW HIGHCONTRASTW"),
        }
    }

    fn payload(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&HighContrastBackup> {
        let BackupPayload::HighContrast(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.high_contrast.backup_kind_mismatch",
            ));
        };
        Ok(payload)
    }

    fn ensure_round_trip_start(
        current: &HighContrastSnapshot,
        stage: ActionStage,
    ) -> ActionResult<()> {
        if current.enabled()
            || matches!(
                &current.scheme,
                crate::windows::HighContrastScheme::Name(name) if !name.is_empty()
            )
        {
            Ok(())
        } else {
            // 実測で NULL の scheme は有効化後に名前へ正規化され、NULL へ戻らなかった。
            // 復元できない開始状態では、一度も書かない。
            Err(ActionError::new(
                ActionErrorCode::CompatibilityBlocked,
                stage,
                false,
                "action.high_contrast.scheme_not_round_trippable",
            ))
        }
    }

    fn matches_applied(
        current: &HighContrastSnapshot,
        payload: &HighContrastBackup,
        envelope: &BackupEnvelope,
    ) -> bool {
        envelope.applied_fingerprint.map_or_else(
            || *current == payload.intended,
            |saved| current.fingerprint() == saved,
        )
    }
}

impl Action for HighContrastAction {
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
        let current = Self::read(ActionStage::Detect)?;
        Ok(Self::observed_state(context, &current))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        Self::parameters(parameters, ActionStage::Validate)?;
        let current = Self::read(ActionStage::Validate)?;
        Self::ensure_round_trip_start(&current, ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        Self::parameters(parameters, ActionStage::Backup)?;
        let original = Self::read(ActionStage::Backup)?;
        Self::ensure_round_trip_start(&original, ActionStage::Backup)?;
        let intended = if original.enabled() {
            original.clone()
        } else {
            original.with_enabled()
        };
        Ok(BackupDraft {
            precondition_fingerprint: original.fingerprint(),
            intended_fingerprint: intended.fingerprint(),
            payload: BackupPayload::HighContrast(HighContrastBackup { original, intended }),
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
        if !payload.original.enabled() {
            replace_high_contrast(&payload.original, &payload.intended).map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.high_contrast.apply_failed",
                    error,
                )
            })?;
            // Windows が scheme 名を正規化する遷移が終わってから「自分の適用値」を記録する。
            std::thread::sleep(std::time::Duration::from_millis(2_500));
        }
        let applied = Self::read(ActionStage::Apply)?;
        if !payload.original.enabled() && !applied.enabled() {
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                ActionStage::Apply,
                false,
                "action.high_contrast.apply_not_observed",
            ));
        }
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
        Self::parameters(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        let current = Self::read(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: if payload.original.enabled() {
                current == payload.original
            } else {
                current.enabled() && Self::matches_applied(&current, payload, envelope)
            },
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
        Self::parameters(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;
        let current = Self::read(ActionStage::Rollback)?;
        if current == payload.original {
            // 既に開始前の全フィールドへ戻っている。
        } else if Self::matches_applied(&current, payload, envelope) {
            replace_high_contrast(&current, &payload.original).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.high_contrast.rollback_failed",
                    error,
                )
            })?;
        } else {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }
        let restored = Self::read(ActionStage::Rollback)?;
        if restored != payload.original {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.high_contrast.exact_restore_failed",
            ));
        }
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
        Self::parameters(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        let current = Self::read(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current == payload.original,
            observed: Self::observed_state(context, &current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::parameters(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "Windowsの見え方を30秒だけ切り替えます。時間切れか「元に戻す」で、開始前の値へ戻します。既に使われている場合は変更しません。".to_owned(),
            method: "Windowsが公開しているコントラストテーマ設定".to_owned(),
            resources: vec!["コントラストテーマの全設定".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope:
                "開始前の構造体サイズ・全フラグ・scheme名を保存し、自分の適用値のままなら、その値へ正確に戻します。"
                    .to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.high_contrast.open_windows_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_or_empty_scheme_is_never_written_when_disabled() {
        let null = HighContrastSnapshot {
            structure_size: 16,
            flags: 126,
            scheme: crate::windows::HighContrastScheme::Null,
        };
        let empty = HighContrastSnapshot {
            scheme: crate::windows::HighContrastScheme::Name(String::new()),
            ..null.clone()
        };
        assert!(HighContrastAction::ensure_round_trip_start(&null, ActionStage::Validate).is_err());
        assert!(
            HighContrastAction::ensure_round_trip_start(&empty, ActionStage::Validate).is_err()
        );

        let enabled = null.with_enabled();
        assert!(
            HighContrastAction::ensure_round_trip_start(&enabled, ActionStage::Validate).is_ok(),
            "既に有効なら書かないためscheme名を要求しない"
        );
    }

    #[test]
    fn screen_copy_has_no_unmeasured_claims() {
        let explanation = HIGH_CONTRAST_ACTION
            .explain_changes(&ActionParameters::AppearanceHighContrastTrial {})
            .expect("explain");
        let visible_copy = format!(
            "{} {} {} {} {}",
            METADATA.name,
            METADATA.description,
            explanation.result,
            explanation.method,
            explanation.rollback_scope
        );
        for prohibited in [
            "読みやすく",
            "見やすく",
            "改善",
            "最適",
            "おすすめ",
            "目に優しい",
            "アクセシビリティ診断",
        ] {
            assert!(
                !visible_copy.contains(prohibited),
                "screen copy contains prohibited claim: {prohibited}"
            );
        }
    }

    #[test]
    fn static_catalog_copy_has_no_unmeasured_claims() {
        let source = include_str!("../../../src/catalog.ts");
        let start = source
            .find(r#"id: "appearance.high_contrast_trial""#)
            .expect("high-contrast catalog entry");
        let remainder = &source[start..];
        let end = remainder[1..]
            .find("\n  {")
            .map_or(remainder.len(), |offset| offset + 1);
        let entry = &remainder[..end];
        for prohibited in [
            "読みやすく",
            "見やすく",
            "改善",
            "最適",
            "おすすめ",
            "目に優しい",
            "アクセシビリティ診断",
        ] {
            assert!(
                !entry.contains(prohibited),
                "catalog copy contains prohibited claim: {prohibited}"
            );
        }
    }
}
