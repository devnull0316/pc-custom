use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppLaunchBundle,
        AppliedEvidence, ChangeExplanation, DetectedState, KnownAppState, MethodClass,
        ObservedValue, RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup},
    windows::{apps_for_bundle, launch_known_apps, observe_known_apps, resolve_known_app},
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup, validate_backup_for_apply,
    validate_base,
};

pub struct LaunchAppsAction;
pub static LAUNCH_APPS_ACTION: LaunchAppsAction = LaunchAppsAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupLaunchApps,
    name: "決めたアプリをまとめて開く",
    description: "コード内の固定リストから選んだ学習・作業用アプリだけを、シェルを介さず直接開きます。起動したアプリは復元時にも終了しません。",
    category: "setup",
    tags: &["apps", "allowlist", "direct-launch", "manual-only"],
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
    kind: ActionKind::OneWay,
    parameter_schema: r#"{"bundle":"study|work|creative|power_toys"}"#,
    resource_keys: &["process:fixed-allowlist-app-launch"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/shell/app-registration",
        "https://learn.microsoft.com/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot",
        "https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-getwindowsdirectoryw",
    ],
    compatibility_key: "setup.launch_apps.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。App Paths登録と固定システムアプリの存在だけを更新後に再確認します。",
};

impl LaunchAppsAction {
    fn bundle(parameters: &ActionParameters) -> ActionResult<AppLaunchBundle> {
        match parameters {
            ActionParameters::SetupLaunchApps { bundle } => Ok(*bundle),
            _ => Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            )),
        }
    }

    fn state(context: &ActionContext<'_>, bundle: AppLaunchBundle) -> ActionResult<DetectedState> {
        let value = observe_known_apps(bundle).map_err(|error| {
            map_windows_error(
                ActionStage::Detect,
                "action.launch_apps.observe_failed",
                error,
            )
        })?;
        Ok(DetectedState::Known {
            value: ObservedValue::KnownApps(value),
            evidence: evidence(context, "Toolhelp process names + fixed App Paths"),
        })
    }
}

impl Action for LaunchAppsAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        Self::state(context, Self::bundle(parameters)?)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let bundle = Self::bundle(parameters)?;
        for app in apps_for_bundle(bundle) {
            resolve_known_app(*app).map_err(|error| {
                map_windows_error(
                    ActionStage::Validate,
                    "action.launch_apps.app_unavailable",
                    error,
                )
                .with_safe_detail(app.name)
            })?;
        }
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let state = Self::state(context, Self::bundle(parameters)?)?;
        let precondition_fingerprint = fingerprint_state(&state, ActionStage::Backup)?;
        let intended = serde_json::to_vec(parameters).map_err(|_| {
            ActionError::new(
                ActionErrorCode::InternalInvariant,
                ActionStage::Backup,
                false,
                "action.launch_apps.serialize_failed",
            )
        })?;
        Ok(BackupDraft {
            precondition_fingerprint,
            intended_fingerprint: Fingerprint::of_bytes(&intended),
            payload: BackupPayload::Observation(ObservationBackup {
                source: "fixed allowlist app launch; no process termination rollback".to_owned(),
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
        if !matches!(&backup.payload, BackupPayload::Observation(saved) if saved.source == "fixed allowlist app launch; no process termination rollback")
        {
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.launch_apps.backup_kind_mismatch",
            ));
        }
        let launched = launch_known_apps(Self::bundle(parameters)?).map_err(|error| {
            map_windows_error(
                ActionStage::Apply,
                "action.launch_apps.launch_failed",
                error,
            )
        })?;
        let state = DetectedState::Known {
            value: ObservedValue::KnownApps(launched),
            evidence: evidence(context, "direct Command::spawn of resolved fixed App Paths"),
        };
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
        let observed = Self::state(context, Self::bundle(parameters)?)?;
        let verified = matches!(observed.known_value(), Some(ObservedValue::KnownApps(value)) if value.apps.iter().all(|app| app.state == KnownAppState::Running));
        Ok(Verification { verified, observed })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_backup(&METADATA, context, backup, ActionStage::Rollback)?;
        let state = Self::state(context, Self::bundle(parameters)?)?;
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
        let observed = Self::state(context, Self::bundle(parameters)?)?;
        Ok(Verification {
            verified: true,
            observed,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let bundle = Self::bundle(parameters)?;
        let names = apps_for_bundle(bundle)
            .iter()
            .map(|app| app.name)
            .collect::<Vec<_>>()
            .join("、");
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: format!("{names}を、起動していない場合だけ開きます。"),
            method: "固定App Paths／Windowsディレクトリ解決後の直接起動（shell非経由、固定空引数）"
                .to_owned(),
            resources: METADATA
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "起動したアプリは勝手に終了しません。このAction自体は元に戻せません。"
                .to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn every_bundle_resolves_only_fixed_known_app_definitions() {
        for bundle in [
            AppLaunchBundle::Study,
            AppLaunchBundle::Work,
            AppLaunchBundle::Creative,
            AppLaunchBundle::PowerToys,
        ] {
            for app in apps_for_bundle(bundle) {
                let result = resolve_known_app(*app);
                if let Ok(path) = result {
                    assert!(path.to_ascii_lowercase().ends_with(".exe"));
                }
            }
        }
    }

    #[test]
    #[ignore = "CC実機確認専用: App Pathsで解決したPowerToys本体を起動し、終了しません"]
    fn powertoys_bundle_launches_only_the_fixed_app_paths_entry() {
        let Some(path) = crate::windows::resolve_powertoys_app_path()
            .expect("read fixed PowerToys App Paths entry")
        else {
            println!("PowerToys is not registered in App Paths; launch was not attempted");
            return;
        };
        assert!(path.to_ascii_lowercase().ends_with("powertoys.exe"));
        let observed =
            launch_known_apps(AppLaunchBundle::PowerToys).expect("launch fixed PowerToys bundle");
        assert_eq!(observed.apps.len(), 1);
        assert_eq!(observed.apps[0].name, "Microsoft PowerToys");
        assert_eq!(observed.apps[0].state, KnownAppState::Running);
    }

    #[test]
    #[ignore = "既知アプリを実際に開き、仕様どおり終了しません。CC実機確認専用"]
    fn creative_bundle_direct_launch_is_observable() {
        use crate::windows::observe_known_apps;

        // 起動前に動いていたものは利用者のものなので触らない。
        let before = observe_known_apps(AppLaunchBundle::Creative)
            .expect("observe creative bundle before launch");
        let already_running: Vec<String> = before
            .apps
            .iter()
            .filter(|app| app.state == KnownAppState::Running)
            .map(|app| app.name.clone())
            .collect();

        let observed = launch_known_apps(AppLaunchBundle::Creative)
            .expect("direct launch fixed creative bundle");
        println!("launched: {:?}", observed.apps);

        // このテストが起動した分だけを閉じる。利用者が開いていたものは残す。
        for app in &observed.apps {
            if already_running.contains(&app.name) {
                continue;
            }
            close_launched_app_for_test(&app.name);
        }

        assert!(observed
            .apps
            .iter()
            .all(|app| app.state != KnownAppState::Unavailable));
    }

    /// テストが起動した分だけを閉じる。固定名のみを扱い、任意の文字列は受け取らない。
    fn close_launched_app_for_test(name: &str) {
        let image = match name {
            "メモ帳" => "notepad.exe",
            "電卓" => "CalculatorApp.exe",
            "ペイント" => "mspaint.exe",
            "Microsoft Edge" => return, // 利用者の常用ブラウザなので触らない
            _ => return,
        };
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", image, "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}
