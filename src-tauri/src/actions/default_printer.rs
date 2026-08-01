use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DefaultPrinterObservation, DetectedState, InstalledPrinterObservation,
        MethodClass, ObservedValue, RollbackEvidence, TroubleshootingStep, ValidationReport,
        Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, DefaultPrinterBackup, Fingerprint},
    windows::{
        read_default_printer_inventory, replace_default_printer, DefaultPrinterInventory,
        PrinterName,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct DefaultPrinterAction;
pub static DEFAULT_PRINTER_ACTION: DefaultPrinterAction = DefaultPrinterAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RollbackPlan {
    RestoreOriginal,
    ExternalConflict,
    OriginalMissing,
}

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SessionDefaultPrinter,
    name: "場面ごとの既定プリンター",
    description: "自宅や職場など利用者が入力した場面で、既にインストール済みの1台を今回の既定にします。終了時は直前の既定へ戻します。印刷は行いません。",
    category: "setup",
    tags: &["プリンター", "場面", "一時変更"],
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
    kind: ActionKind::Session,
    parameter_schema: r#"{"scene":"string(1..64)","printer":"installed-printer-name"}"#,
    resource_keys: &["windows:printing:default-printer"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/printdocs/getdefaultprinter",
        "https://learn.microsoft.com/windows/win32/printdocs/enumprinters",
        "https://learn.microsoft.com/windows/win32/printdocs/setdefaultprinter",
        "https://learn.microsoft.com/windows/win32/api/commdlg/ns-commdlg-printdlgexw",
    ],
    compatibility_key: "session.default_printer.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低",
};

impl DefaultPrinterAction {
    fn rollback_plan(
        current: &DefaultPrinterInventory,
        payload: &DefaultPrinterBackup,
    ) -> RollbackPlan {
        if current.windows_managed || current.default != payload.intended {
            RollbackPlan::ExternalConflict
        } else if !current.contains(&payload.original) {
            RollbackPlan::OriginalMissing
        } else {
            RollbackPlan::RestoreOriginal
        }
    }

    fn target(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<&PrinterName> {
        let ActionParameters::SessionDefaultPrinter { scene, printer } = parameters else {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                stage,
                false,
                "action.parameters.id_mismatch",
            ));
        };
        if !scene.is_valid() || !printer.is_valid() {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.default_printer.selection_required",
            ));
        }
        Ok(printer)
    }

    fn read_inventory(stage: ActionStage) -> ActionResult<DefaultPrinterInventory> {
        read_default_printer_inventory().map_err(|error| {
            map_windows_error(stage, "action.default_printer.read_inventory_failed", error)
        })
    }

    fn ensure_mutable(
        inventory: &DefaultPrinterInventory,
        target: &PrinterName,
        stage: ActionStage,
    ) -> ActionResult<()> {
        if inventory.windows_managed {
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                stage,
                false,
                "action.default_printer.windows_managed",
            ));
        }
        if !inventory.contains(target) {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.default_printer.not_installed",
            ));
        }
        if inventory.default == *target {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.default_printer.already_default",
            ));
        }
        Ok(())
    }

    fn observed_state(
        context: &ActionContext<'_>,
        inventory: &DefaultPrinterInventory,
    ) -> DetectedState {
        if inventory.windows_managed {
            return DetectedState::PolicyManaged {
                authority: Some("Windowsの既定プリンター自動管理".to_owned()),
            };
        }
        DetectedState::Known {
            value: ObservedValue::DefaultPrinter(DefaultPrinterObservation {
                windows_managed: false,
                printers: inventory
                    .printers
                    .iter()
                    .map(|name| InstalledPrinterObservation {
                        name: name.clone(),
                        is_default: name == &inventory.default,
                    })
                    .collect(),
            }),
            evidence: evidence(
                context,
                "GetDefaultPrinterW and EnumPrintersW installed-printer readback",
            ),
        }
    }

    fn intended_fingerprint(target: &PrinterName) -> Fingerprint {
        let managed = [0u8];
        Fingerprint::of_parts([managed.as_slice(), target.as_str().as_bytes()])
    }

    fn payload(
        envelope: &BackupEnvelope,
        stage: ActionStage,
    ) -> ActionResult<&DefaultPrinterBackup> {
        let BackupPayload::DefaultPrinter(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.default_printer.backup_kind_mismatch",
            ));
        };
        if !payload.original.is_valid()
            || !payload.intended.is_valid()
            || payload.original == payload.intended
        {
            return Err(ActionError::recovery_required(
                stage,
                "action.default_printer.backup_contract_mismatch",
            ));
        }
        Ok(payload)
    }
}

impl Action for DefaultPrinterAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let ActionParameters::SessionDefaultPrinter { .. } = parameters else {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Detect,
                false,
                "action.parameters.id_mismatch",
            ));
        };
        let inventory = Self::read_inventory(ActionStage::Detect)?;
        Ok(Self::observed_state(context, &inventory))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let target = Self::target(parameters, ActionStage::Validate)?;
        let inventory = Self::read_inventory(ActionStage::Validate)?;
        Self::ensure_mutable(&inventory, target, ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        let target = Self::target(parameters, ActionStage::Backup)?;
        let inventory = Self::read_inventory(ActionStage::Backup)?;
        Self::ensure_mutable(&inventory, target, ActionStage::Backup)?;
        Ok(BackupDraft {
            precondition_fingerprint: inventory.fingerprint(),
            intended_fingerprint: Self::intended_fingerprint(target),
            payload: BackupPayload::DefaultPrinter(DefaultPrinterBackup {
                original: inventory.default,
                intended: target.clone(),
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
        let target = Self::target(parameters, ActionStage::Apply)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let payload = Self::payload(envelope, ActionStage::Apply)?;
        if target != &payload.intended {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.default_printer.parameter_backup_mismatch",
            ));
        }
        let applied =
            replace_default_printer(&payload.original, &payload.intended).map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.default_printer.apply_failed",
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
        Self::target(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        let current = Self::read_inventory(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: !current.windows_managed && current.default == payload.intended,
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
        Self::target(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;
        let current = Self::read_inventory(ActionStage::Rollback)?;
        match Self::rollback_plan(&current, payload) {
            RollbackPlan::ExternalConflict => {
                return Err(ActionError::new(
                    ActionErrorCode::ExternalConflict,
                    ActionStage::Rollback,
                    false,
                    "action.rollback.external_change_detected",
                ))
            }
            RollbackPlan::OriginalMissing => {
                return Err(ActionError::recovery_required(
                    ActionStage::Rollback,
                    "action.default_printer.original_missing",
                ))
            }
            RollbackPlan::RestoreOriginal => {}
        }
        let restored =
            replace_default_printer(&payload.intended, &payload.original).map_err(|error| {
                map_windows_error(
                    ActionStage::Rollback,
                    "action.default_printer.rollback_failed",
                    error,
                )
            })?;
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
        Self::target(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        let current = Self::read_inventory(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: !current.windows_managed && current.default == payload.original,
            observed: Self::observed_state(context, &current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::target(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "利用者が選んだ場面の間だけ、選択したインストール済みプリンターを既定にします。実際の印刷は行いません。".to_owned(),
            method: "Windowsの公開プリンターAPI".to_owned(),
            resources: vec!["現在の利用者の既定プリンター1件".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "開始前の既定プリンター名へだけ戻します。途中で既定が変わった場合や元のプリンターが無くなった場合は上書きせず、戻せていない状態を残します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.default_printer.check_windows_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::SceneLabel;

    fn name(value: &str) -> PrinterName {
        PrinterName::new(value.to_owned()).expect("valid test printer")
    }

    fn parameters(target: PrinterName) -> ActionParameters {
        ActionParameters::SessionDefaultPrinter {
            scene: SceneLabel::new("test scene".to_owned()),
            printer: target,
        }
    }

    #[test]
    fn sensitive_names_are_redacted_from_parameter_and_backup_debug() {
        let private = "Private Person's Office Printer";
        let target = name(private);
        let parameters = parameters(target.clone());
        let backup = DefaultPrinterBackup {
            original: name("original"),
            intended: target,
        };
        assert!(!format!("{parameters:?}").contains(private));
        assert!(!format!("{backup:?}").contains(private));
    }

    #[test]
    fn windows_management_and_uninstalled_targets_fail_closed() {
        let original = name("original");
        let target = name("target");
        let managed = DefaultPrinterInventory {
            default: original.clone(),
            printers: vec![original.clone(), target.clone()],
            windows_managed: true,
        };
        assert_eq!(
            DefaultPrinterAction::ensure_mutable(&managed, &target, ActionStage::Validate)
                .expect_err("Windows management must block")
                .code,
            ActionErrorCode::StateUnknown
        );
        let missing = DefaultPrinterInventory {
            default: original.clone(),
            printers: vec![original],
            windows_managed: false,
        };
        assert_eq!(
            DefaultPrinterAction::ensure_mutable(&missing, &target, ActionStage::Validate)
                .expect_err("uninstalled target must block")
                .code,
            ActionErrorCode::InvalidParameters
        );
    }

    #[test]
    fn backup_contract_requires_two_distinct_valid_names() {
        let same = name("same");
        let draft = BackupDraft {
            precondition_fingerprint: same.fingerprint(),
            intended_fingerprint: same.fingerprint(),
            payload: BackupPayload::DefaultPrinter(DefaultPrinterBackup {
                original: same.clone(),
                intended: same,
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
        assert_eq!(
            DefaultPrinterAction::payload(&envelope, ActionStage::Rollback)
                .expect_err("same-name backup must fail")
                .code,
            ActionErrorCode::RecoveryRequired
        );
    }

    #[test]
    fn rollback_never_overwrites_an_external_default_or_substitutes_a_missing_original() {
        let original = name("original");
        let intended = name("intended");
        let third = name("third");
        let payload = DefaultPrinterBackup {
            original: original.clone(),
            intended: intended.clone(),
        };
        let inventory = |default, printers, windows_managed| DefaultPrinterInventory {
            default,
            printers,
            windows_managed,
        };
        assert_eq!(
            DefaultPrinterAction::rollback_plan(
                &inventory(
                    third.clone(),
                    vec![original.clone(), intended.clone(), third],
                    false,
                ),
                &payload,
            ),
            RollbackPlan::ExternalConflict
        );
        assert_eq!(
            DefaultPrinterAction::rollback_plan(
                &inventory(intended.clone(), vec![intended.clone()], false),
                &payload,
            ),
            RollbackPlan::OriginalMissing
        );
        assert_eq!(
            DefaultPrinterAction::rollback_plan(
                &inventory(
                    intended,
                    vec![original.clone(), payload.intended.clone()],
                    true,
                ),
                &payload,
            ),
            RollbackPlan::ExternalConflict
        );
    }

    #[cfg(windows)]
    struct RestoreDefaultOnDrop {
        original: PrinterName,
        intended: PrinterName,
        armed: bool,
    }

    #[cfg(windows)]
    impl Drop for RestoreDefaultOnDrop {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            let Ok(current) = read_default_printer_inventory() else {
                return;
            };
            if current.windows_managed
                || current.default != self.intended
                || !current.contains(&self.original)
            {
                return;
            }
            let _ = replace_default_printer(&self.intended, &self.original);
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "実機の既定プリンターを一時的に別のインストール済みプリンターへ変更して戻す"]
    fn real_machine_default_printer_round_trip() {
        use crate::{
            compatibility::OsIdentity,
            windows::{acquire_core_mutation_lock, read_print_dialog_default_in_child},
        };

        let _mutation_lock = acquire_core_mutation_lock().expect("exclusive core mutation lock");
        let before = read_default_printer_inventory().expect("read current default and inventory");
        println!(
            "EVIDENCE: default_printer current_present=true installed_count={} windows_managed={}",
            before.printers.len(),
            before.windows_managed
        );
        if before.windows_managed {
            println!(
                "EVIDENCE: default_printer measured=false reason=windows_manages_default no_change=true"
            );
            return;
        }
        let Some(target) = before
            .printers
            .iter()
            .find(|candidate| *candidate != &before.default)
            .cloned()
        else {
            println!(
                "EVIDENCE: default_printer measured=false reason=no_different_installed_printer no_change=true"
            );
            return;
        };

        let mut cleanup = RestoreDefaultOnDrop {
            original: before.default.clone(),
            intended: target.clone(),
            armed: true,
        };
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
        let parameters = parameters(target.clone());
        let draft = DEFAULT_PRINTER_ACTION
            .create_backup(&context, &parameters)
            .expect("save exact original default");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );

        let applied = DEFAULT_PRINTER_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("set selected installed printer");
        envelope.record_applied(applied.applied_fingerprint);
        let independent_after =
            read_print_dialog_default_in_child().expect("independent PrintDlgEx readback");
        println!(
            "EVIDENCE: default_printer applied readback_same_as_target={} readback_differs_from_before={}",
            independent_after == target.fingerprint(),
            independent_after != before.default.fingerprint()
        );
        assert_eq!(independent_after, target.fingerprint());
        assert_ne!(independent_after, before.default.fingerprint());
        assert!(
            DEFAULT_PRINTER_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify selected default")
                .verified
        );

        DEFAULT_PRINTER_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("restore exact original default");
        let independent_restored =
            read_print_dialog_default_in_child().expect("independent restored readback");
        println!(
            "EVIDENCE: default_printer restored readback_same_as_original={} readback_differs_from_target={}",
            independent_restored == before.default.fingerprint(),
            independent_restored != target.fingerprint()
        );
        assert_eq!(independent_restored, before.default.fingerprint());
        assert_ne!(independent_restored, target.fingerprint());
        assert!(
            DEFAULT_PRINTER_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify exact original default")
                .verified
        );
        cleanup.armed = false;
    }
}
