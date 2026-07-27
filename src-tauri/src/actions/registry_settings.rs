//! Evidence-pending, fixed per-user DWORD storage observations.
//!
//! Every target is compiled into this binary. Neither the UI nor a profile can
//! supply a registry path, value name, type, or arbitrary numeric value.
//! These candidates are deliberately non-mutable until primary setter evidence
//! and target-build UI smoke results are approved. Rollback remains available
//! only for recovery of a previously durable backup.

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{
        read_registry_state, restore_registry_backup, verify_registry_backup_restored, BackupDraft,
        BackupEnvelope, BackupPayload, RegistryRestoreOutcome, RegistryTarget,
    },
    windows::notify_explorer_settings_changed,
};

use super::common::{decode_dword, evidence, map_windows_error, validate_backup, validate_base};

const WINDOWS_SETTINGS_REFERENCE: &str =
    "https://learn.microsoft.com/windows/apps/develop/settings/settings-windows-11";
const WINDOWS_COMMON_SETTINGS_REFERENCE: &str =
    "https://learn.microsoft.com/windows/apps/develop/settings/settings-common";
/// `ActionMetadata` predates nullable build evidence. Zero is serialized to
/// `null` by the presentation layer and means that no mutation build has been
/// approved. It is never interpreted as a Windows build number.
const UNTESTED_BUILD_SENTINEL: u32 = 0;

type DesiredValue = fn(&ActionParameters) -> ActionResult<u32>;
type ValidValue = fn(u32) -> bool;

pub struct DwordRegistryAction {
    metadata: &'static ActionMetadata,
    target: RegistryTarget,
    desired: DesiredValue,
    valid: ValidValue,
    result: &'static str,
}

impl DwordRegistryAction {
    const fn new(
        metadata: &'static ActionMetadata,
        target: RegistryTarget,
        desired: DesiredValue,
        valid: ValidValue,
        result: &'static str,
    ) -> Self {
        Self {
            metadata,
            target,
            desired,
            valid,
            result,
        }
    }
}

// Action のメタデータをまとめて組み立てる定数関数。分割すると呼び出し側の
// マクロが読みにくくなるだけで、引数の数は素直に項目数を反映している。
#[allow(clippy::too_many_arguments)]
const fn registry_metadata(
    id: ActionId,
    name: &'static str,
    _description: &'static str,
    category: &'static str,
    _tags: &'static [&'static str],
    parameter_schema: &'static str,
    resource_keys: &'static [&'static str],
    _risk_level: ActionRiskLevel,
    _evidence_urls: &'static [&'static str],
    compatibility_key: &'static str,
    update_impact: &'static str,
) -> ActionMetadata {
    ActionMetadata {
        id,
        name,
        description: "候補となる固定HKCU DWORDの保存値だけを読み取ります。setterの一次資料と対象buildの実機試験が揃うまで変更しません。",
        category,
        tags: &["HKCU", "storage-observation", "evidence-pending"],
        supportedWindowsVersions: &[
            WindowsReleaseFamily::Windows11_24H2,
            WindowsReleaseFamily::Windows11_25H2,
        ],
        minimumBuild: 26_100,
        maximumTestedBuild: UNTESTED_BUILD_SENTINEL,
        riskLevel: ActionRiskLevel::Experimental,
        requiresAdmin: false,
        requiresRestart: false,
        requiresExplorerRestart: false,
        conflicts: &[],
        dependencies: &[],
        action_version: 1,
        kind: ActionKind::Guided,
        parameter_schema,
        resource_keys,
        method_class: MethodClass::UnverifiedStorage,
        evidence_urls: &[],
        compatibility_key,
        backup_codec_version: 1,
        rollback_decoder_versions: &[1],
        // Evidence-pending candidates can never enter a game profile.
        auto_apply_eligible: false,
        windows_update_impact: update_impact,
    }
}

fn wrong_parameters() -> ActionError {
    ActionError::new(
        ActionErrorCode::WrongParameters,
        ActionStage::Validate,
        false,
        "action.parameters.id_mismatch",
    )
}

fn boolean_value_is_valid(value: u32) -> bool {
    value <= 1
}

fn three_value_is_valid(value: u32) -> bool {
    value <= 2
}

fn four_value_is_valid(value: u32) -> bool {
    value <= 3
}

fn explorer_launch_target_is_valid(value: u32) -> bool {
    (1..=3).contains(&value)
}

fn setter_evidence_pending(stage: ActionStage) -> ActionError {
    ActionError::new(
        ActionErrorCode::CompatibilityBlocked,
        stage,
        false,
        "action.registry_setting.setter_evidence_pending",
    )
}

fn detect_setting(
    action: &DwordRegistryAction,
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    target: RegistryTarget,
) -> ActionResult<DetectedState> {
    validate_base(
        action.metadata,
        context,
        parameters,
        false,
        ActionStage::Detect,
    )?;
    let _ = (action.desired)(parameters)?;
    let state = read_registry_state(&target.location()).map_err(|error| {
        map_windows_error(
            ActionStage::Detect,
            "action.registry_setting.detect_failed",
            error,
        )
    })?;
    let configured = if state.value_existed {
        match decode_dword(state.value_type, &state.raw_bytes) {
            Some(value) if (action.valid)(value) => Some(value),
            _ => return Ok(DetectedState::Unknown {
                reason:
                    "保存値の型または値域が候補schema外のため、Windows UI状態として解釈しません。"
                        .to_owned(),
            }),
        }
    } else {
        None
    };
    Ok(DetectedState::Known {
        value: ObservedValue::RegistryDword { configured },
        evidence: evidence(
            context,
            "unverified fixed HKCU storage value (64-bit view; not UI state)",
        ),
    })
}

impl Action for DwordRegistryAction {
    fn metadata(&self) -> &'static ActionMetadata {
        self.metadata
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        detect_setting(self, context, parameters, self.target)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let _report = validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Validate,
        )?;
        let _ = (self.desired)(parameters)?;
        Err(setter_evidence_pending(ActionStage::Validate))
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Backup,
        )?;
        let _ = (self.desired)(parameters)?;
        Err(setter_evidence_pending(ActionStage::Backup))
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        _envelope: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Apply,
        )?;
        let _ = (self.desired)(parameters)?;
        Err(setter_evidence_pending(ActionStage::Apply))
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(self.metadata, context, envelope, ActionStage::VerifyApplied)?;
        let expected = (self.desired)(parameters)?;
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
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Rollback,
        )?;
        validate_backup(self.metadata, context, envelope, ActionStage::Rollback)?;
        let BackupPayload::Registry(backup) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.registry_setting.backup_kind_mismatch",
            ));
        };
        if backup.location != self.target.location() {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.registry_setting.backup_target_mismatch",
            ));
        }
        let outcome = restore_registry_backup(backup).map_err(|error| {
            map_windows_error(
                ActionStage::Rollback,
                "action.registry_setting.rollback_failed",
                error,
            )
        })?;
        match outcome {
            RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal => {}
            RegistryRestoreOutcome::RestoredValueKeyRetained => {
                return Err(ActionError::recovery_required(
                    ActionStage::Rollback,
                    "action.registry_setting.rollback_key_retained",
                ));
            }
            RegistryRestoreOutcome::ExternalConflict => {
                return Err(ActionError::new(
                    ActionErrorCode::ExternalConflict,
                    ActionStage::Rollback,
                    false,
                    "action.rollback.external_change_detected",
                ));
            }
        }
        let _broadcast = notify_explorer_settings_changed();
        let state = self.detect_current_state(context, parameters)?;
        Ok(RollbackEvidence {
            restored_fingerprint: backup.original.fingerprint(&backup.location),
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
            self.metadata,
            context,
            envelope,
            ActionStage::VerifyRolledBack,
        )?;
        let BackupPayload::Registry(backup) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                ActionStage::VerifyRolledBack,
                "action.registry_setting.backup_kind_mismatch",
            ));
        };
        let verified = verify_registry_backup_restored(backup).map_err(|error| {
            map_windows_error(
                ActionStage::VerifyRolledBack,
                "action.registry_setting.rollback_verify_failed",
                error,
            )
        })?;
        let observed = self.detect_current_state(context, parameters)?;
        Ok(Verification { verified, observed })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let _ = (self.desired)(parameters)?;
        let _candidate_result = self.result;
        Ok(ChangeExplanation {
            action_id: self.metadata.id,
            result: "setter根拠と対象buildの実機試験が承認されるまで、保存値の観測だけを行い変更しません。".to_owned(),
            method: "未立証の固定HKCU保存値を読み取り（変更処理はrelease quarantine）".to_owned(),
            resources: self
                .metadata
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: self.metadata.windows_update_impact.to_owned(),
            rollback_scope: "新規変更を作らないため通常のrollback対象はありません。旧durable backupの復旧経路だけを保持します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.registry_setting.open_official_settings_if_refresh_is_delayed",
            opens_official_settings: true,
        }]
    }
}

macro_rules! bool_parameter {
    ($function:ident, $variant:ident, $field:ident) => {
        fn $function(parameters: &ActionParameters) -> ActionResult<u32> {
            match parameters {
                ActionParameters::$variant { $field } => Ok(u32::from(*$field)),
                _ => Err(wrong_parameters()),
            }
        }
    };
}

macro_rules! inverted_bool_parameter {
    ($function:ident, $variant:ident, $field:ident) => {
        fn $function(parameters: &ActionParameters) -> ActionResult<u32> {
            match parameters {
                ActionParameters::$variant { $field } => Ok(u32::from(!*$field)),
                _ => Err(wrong_parameters()),
            }
        }
    };
}

fn taskbar_search_mode_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    use crate::action::TaskbarSearchMode;
    match parameters {
        ActionParameters::TaskbarSearchMode { mode } => Ok(match mode {
            TaskbarSearchMode::Hidden => 0,
            TaskbarSearchMode::Icon => 1,
            TaskbarSearchMode::IconAndLabel => 2,
            TaskbarSearchMode::SearchBox => 3,
        }),
        _ => Err(wrong_parameters()),
    }
}

fn taskbar_alignment_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    use crate::action::TaskbarAlignment;
    match parameters {
        ActionParameters::TaskbarAlignment { alignment } => Ok(match alignment {
            TaskbarAlignment::Left => 0,
            TaskbarAlignment::Center => 1,
        }),
        _ => Err(wrong_parameters()),
    }
}

fn start_layout_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    use crate::action::StartLayout;
    match parameters {
        ActionParameters::StartLayout { layout } => Ok(match layout {
            StartLayout::Default => 0,
            StartLayout::MorePins => 1,
            StartLayout::MoreRecommendations => 2,
        }),
        _ => Err(wrong_parameters()),
    }
}

fn explorer_launch_target_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    use crate::action::ExplorerLaunchTarget;
    match parameters {
        ActionParameters::ExplorerLaunchTarget { target } => Ok(match target {
            ExplorerLaunchTarget::ThisPc => 1,
            ExplorerLaunchTarget::Home => 2,
            ExplorerLaunchTarget::Downloads => 3,
        }),
        _ => Err(wrong_parameters()),
    }
}

fn grouping_mode_value(mode: crate::action::TaskbarGroupingMode) -> u32 {
    use crate::action::TaskbarGroupingMode;
    match mode {
        TaskbarGroupingMode::Always => 0,
        TaskbarGroupingMode::WhenFull => 1,
        TaskbarGroupingMode::Never => 2,
    }
}

fn taskbar_grouping_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    match parameters {
        ActionParameters::TaskbarButtonGrouping { mode } => Ok(grouping_mode_value(*mode)),
        _ => Err(wrong_parameters()),
    }
}

fn taskbar_secondary_grouping_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    match parameters {
        ActionParameters::TaskbarSecondaryButtonGrouping { mode } => Ok(grouping_mode_value(*mode)),
        _ => Err(wrong_parameters()),
    }
}

fn taskbar_multi_monitor_mode_desired(parameters: &ActionParameters) -> ActionResult<u32> {
    use crate::action::TaskbarMultiMonitorMode;
    match parameters {
        ActionParameters::TaskbarMultiMonitorMode { mode } => Ok(match mode {
            TaskbarMultiMonitorMode::AllTaskbars => 0,
            TaskbarMultiMonitorMode::MainAndWindow => 1,
            TaskbarMultiMonitorMode::WindowMonitor => 2,
        }),
        _ => Err(wrong_parameters()),
    }
}

bool_parameter!(start_recommendations_desired, StartRecommendations, enabled);
bool_parameter!(explorer_recent_files_desired, ExplorerRecentFiles, show);
bool_parameter!(taskbar_flashing_desired, TaskbarFlashing, enabled);
bool_parameter!(taskbar_share_window_desired, TaskbarShareWindow, enabled);
bool_parameter!(taskbar_show_desktop_desired, TaskbarShowDesktop, enabled);
bool_parameter!(search_recent_hover_desired, SearchRecentOnHover, enabled);
bool_parameter!(taskbar_multi_monitor_desired, TaskbarMultiMonitor, enabled);
bool_parameter!(start_show_all_pins_desired, StartShowAllPins, enabled);
bool_parameter!(start_recent_apps_desired, StartRecentApps, show);
bool_parameter!(
    accent_start_taskbar_desired,
    AppearanceAccentStartTaskbar,
    enabled
);
bool_parameter!(
    accent_title_bars_desired,
    AppearanceAccentTitleBars,
    enabled
);
bool_parameter!(auto_accent_desired, AppearanceAutoAccent, enabled);
bool_parameter!(game_mode_desired, GamesGameMode, enabled);
bool_parameter!(controller_game_bar_desired, GamesControllerGameBar, enabled);
inverted_bool_parameter!(autoplay_desired, DevicesAutoplay, enabled);
bool_parameter!(usb_errors_desired, NotificationsUsbErrors, enabled);
bool_parameter!(weak_charger_desired, NotificationsWeakCharger, enabled);
bool_parameter!(autocorrect_desired, InputAutocorrect, enabled);
bool_parameter!(double_space_desired, InputDoubleSpacePeriod, enabled);
bool_parameter!(auto_shift_desired, InputAutoShift, enabled);
bool_parameter!(voice_typing_key_desired, InputVoiceTypingKey, enabled);
bool_parameter!(multilingual_desired, InputMultilingualSuggestions, enabled);
bool_parameter!(status_bar_desired, ExplorerStatusBar, show);
bool_parameter!(info_tips_desired, ExplorerInfoTips, show);
bool_parameter!(hide_empty_drives_desired, ExplorerHideEmptyDrives, hide);
bool_parameter!(
    nav_expand_current_desired,
    ExplorerNavExpandCurrent,
    enabled
);
bool_parameter!(nav_show_all_desired, ExplorerNavShowAll, enabled);
bool_parameter!(separate_process_desired, ExplorerSeparateProcess, enabled);
bool_parameter!(icons_only_desired, ExplorerIconsOnly, enabled);
bool_parameter!(drive_letters_desired, ExplorerDriveLetters, show);
bool_parameter!(preview_handlers_desired, ExplorerPreviewHandlers, enabled);
bool_parameter!(sharing_wizard_desired, ExplorerSharingWizard, enabled);
bool_parameter!(always_show_menus_desired, ExplorerAlwaysShowMenus, enabled);
bool_parameter!(
    taskbar_animations_desired,
    AppearanceTaskbarAnimations,
    enabled
);
bool_parameter!(toast_banners_desired, NotificationsToastBanners, enabled);

const ADVANCED_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const EXPLORER_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer";
const START_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Start";
const PERSONALIZE_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

macro_rules! action_metadata {
    (
        $metadata:ident, $action:ident, $id:ident, $name:literal, $description:literal,
        $category:literal, $tags:expr, $schema:literal, $resource:literal,
        $risk:ident, $evidence:expr, $compatibility:literal, $impact:literal,
        $subkey:expr, $value_name:literal, $desired:ident, $valid:ident, $result:literal
    ) => {
        static $metadata: ActionMetadata = registry_metadata(
            ActionId::$id,
            $name,
            $description,
            $category,
            $tags,
            $schema,
            &[$resource],
            ActionRiskLevel::$risk,
            $evidence,
            $compatibility,
            $impact,
        );
        pub static $action: DwordRegistryAction = DwordRegistryAction::new(
            &$metadata,
            RegistryTarget::current_user_64($subkey, $value_name),
            $desired,
            $valid,
            $result,
        );
    };
}

action_metadata!(
    TASKBAR_SEARCH_MODE_METADATA,
    TASKBAR_SEARCH_MODE_ACTION,
    TaskbarSearchMode,
    "タスクバー検索の表示方法を選ぶ",
    "検索を隠す、アイコン、ラベル付き、検索ボックスから選びます。固定HKCU値だけを変更します。",
    "appearance",
    &["taskbar", "search", "explicit-only"],
    r#"{"mode":"hidden|icon|icon_and_label|search_box"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/search:searchboxtaskbarmode",
    Caution,
    &["https://learn.microsoft.com/windows/client-management/mdm/policy-csp-search"],
    "taskbar.search_mode.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    r"Software\Microsoft\Windows\CurrentVersion\Search",
    "SearchboxTaskbarMode",
    taskbar_search_mode_desired,
    four_value_is_valid,
    "タスクバー検索を選択した表示方法へ変更します。"
);

action_metadata!(
    TASKBAR_ALIGNMENT_METADATA,
    TASKBAR_ALIGNMENT_ACTION,
    TaskbarAlignment,
    "タスクバーを左寄せ／中央寄せにする",
    "Windows 11のタスクバー配置を選びます。タスクバーを終了せず設定変更通知だけを送ります。",
    "appearance",
    &["taskbar", "alignment", "explicit-only"],
    r#"{"alignment":"left|center"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbaral",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "taskbar.alignment.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarAl",
    taskbar_alignment_desired,
    boolean_value_is_valid,
    "タスクバーを選択した配置へ変更します。"
);

action_metadata!(
    START_LAYOUT_METADATA,
    START_LAYOUT_ACTION,
    StartLayout,
    "スタートのピンとおすすめの比率を選ぶ",
    "既定、ピンを増やす、おすすめを増やすから選びます。Microsoft公開の0〜2値だけを使用します。",
    "appearance",
    &["start", "layout", "documented"],
    r#"{"layout":"default|more_pins|more_recommendations"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:start_layout",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "start.layout.v1",
    "中。Windows更新後にスタート画面の実機スモークを再実施します。",
    ADVANCED_SUBKEY,
    "Start_Layout",
    start_layout_desired,
    three_value_is_valid,
    "スタートのピンとおすすめの比率を選択状態へ変更します。"
);

action_metadata!(
    START_RECOMMENDATIONS_METADATA,
    START_RECOMMENDATIONS_ACTION,
    StartRecommendations,
    "スタートのヒントや新しいアプリのおすすめを減らす",
    "おすすめ機能の表示だけを切り替え、ピンやインストール済みアプリは変更しません。",
    "appearance",
    &["start", "recommendations", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:start_irisrecommendations",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "start.recommendations.v1",
    "中。Windows更新後にスタート画面の実機スモークを再実施します。",
    ADVANCED_SUBKEY,
    "Start_IrisRecommendations",
    start_recommendations_desired,
    boolean_value_is_valid,
    "スタートのおすすめ表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_LAUNCH_TARGET_METADATA,
    EXPLORER_LAUNCH_TARGET_ACTION,
    ExplorerLaunchTarget,
    "Explorerを開いたときの場所を選ぶ",
    "ホーム、PC、ダウンロードから開始場所を選びます。既存ウィンドウの位置は変更しません。",
    "explorer",
    &["explorer", "launch", "explicit-only"],
    r#"{"target":"home|this_pc|downloads"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:launchto",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.launch_target.v1",
    "中。Explorer更新後に各選択肢の実機スモークを再実施します。",
    ADVANCED_SUBKEY,
    "LaunchTo",
    explorer_launch_target_desired,
    explorer_launch_target_is_valid,
    "Explorerの開始場所を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_RECENT_FILES_METADATA,
    EXPLORER_RECENT_FILES_ACTION,
    ExplorerRecentFiles,
    "Explorerに最近使ったファイルを表示する",
    "Explorerの最近使ったファイル欄だけを切り替え、履歴ファイル自体は削除しません。",
    "explorer",
    &["explorer", "recent", "privacy"],
    r#"{"show":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer:showrecent",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.recent_files.v1",
    "中。Explorer更新後に表示範囲の実機スモークを再実施します。",
    EXPLORER_SUBKEY,
    "ShowRecent",
    explorer_recent_files_desired,
    boolean_value_is_valid,
    "Explorerの最近使ったファイル表示を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_BUTTON_GROUPING_METADATA,
    TASKBAR_BUTTON_GROUPING_ACTION,
    TaskbarButtonGrouping,
    "タスクバーのボタン結合方法を選ぶ",
    "常に結合、いっぱいのとき、結合しないから選びます。",
    "appearance",
    &["taskbar", "grouping", "explicit-only"],
    r#"{"mode":"always|when_full|never"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbarglomlevel",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "taskbar.button_grouping.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarGlomLevel",
    taskbar_grouping_desired,
    three_value_is_valid,
    "タスクバーボタンの結合方法を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_FLASHING_METADATA,
    TASKBAR_FLASHING_ACTION,
    TaskbarFlashing,
    "タスクバーアプリの点滅通知を切り替える",
    "注意が必要なアプリがタスクバーで点滅する表示だけを切り替えます。通知自体は削除しません。",
    "appearance",
    &["taskbar", "flashing", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbarflashing",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "taskbar.flashing.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarFlashing",
    taskbar_flashing_desired,
    boolean_value_is_valid,
    "タスクバーアプリの点滅表示を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_SHARE_WINDOW_METADATA,
    TASKBAR_SHARE_WINDOW_ACTION,
    TaskbarShareWindow,
    "タスクバーからウィンドウを共有できるようにする",
    "対応する会議アプリで、タスクバーからウィンドウ共有を選べる表示を切り替えます。",
    "appearance",
    &["taskbar", "sharing", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbarsn",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "taskbar.share_window.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarSn",
    taskbar_share_window_desired,
    boolean_value_is_valid,
    "タスクバーのウィンドウ共有表示を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_SHOW_DESKTOP_METADATA,
    TASKBAR_SHOW_DESKTOP_ACTION,
    TaskbarShowDesktop,
    "タスクバー右端のデスクトップ表示を切り替える",
    "タスクバー右端を選んでデスクトップを表示する操作を切り替えます。",
    "appearance",
    &["taskbar", "desktop", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbarsd",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "taskbar.show_desktop.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarSd",
    taskbar_show_desktop_desired,
    boolean_value_is_valid,
    "タスクバー右端のデスクトップ表示操作を選択状態へ変更します。"
);

action_metadata!(
    SEARCH_RECENT_ON_HOVER_METADATA,
    SEARCH_RECENT_ON_HOVER_ACTION,
    SearchRecentOnHover,
    "検索アイコンに触れたときの最近の検索表示を切り替える",
    "検索アイコンへポインターを置いたときに最近の検索を開く動作を切り替えます。",
    "appearance",
    &["search", "history", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/feeds/dsb:openonhover",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "search.recent_on_hover.v1",
    "高。検索UI更新後はAction固有の実機確認まで自動適用しません。",
    r"Software\Microsoft\Windows\CurrentVersion\Feeds\DSB",
    "OpenOnHover",
    search_recent_hover_desired,
    boolean_value_is_valid,
    "検索アイコンのホバー動作を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_MULTI_MONITOR_METADATA,
    TASKBAR_MULTI_MONITOR_ACTION,
    TaskbarMultiMonitor,
    "すべてのモニターにタスクバーを表示する",
    "複数ディスプレイでタスクバーを表示するかを切り替えます。単一ディスプレイでは効果がありません。",
    "appearance",
    &["taskbar", "multi-monitor", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:mmtaskbarenabled",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "taskbar.multi_monitor.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "MMTaskbarEnabled",
    taskbar_multi_monitor_desired,
    boolean_value_is_valid,
    "複数モニターのタスクバー表示を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_MULTI_MONITOR_MODE_METADATA,
    TASKBAR_MULTI_MONITOR_MODE_ACTION,
    TaskbarMultiMonitorMode,
    "複数モニターでアプリボタンを出す場所を選ぶ",
    "すべて、メインとウィンドウのある画面、ウィンドウのある画面から選びます。",
    "appearance",
    &["taskbar", "multi-monitor", "explicit-only"],
    r#"{"mode":"all_taskbars|main_and_window|window_monitor"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:mmtaskbarmode",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "taskbar.multi_monitor_mode.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "MMTaskbarMode",
    taskbar_multi_monitor_mode_desired,
    three_value_is_valid,
    "複数モニターのアプリボタン表示先を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_SECONDARY_GROUPING_METADATA,
    TASKBAR_SECONDARY_BUTTON_GROUPING_ACTION,
    TaskbarSecondaryButtonGrouping,
    "別モニターのタスクバーボタン結合方法を選ぶ",
    "セカンダリーモニターのボタンを、常に結合、いっぱいのとき、結合しないから選びます。",
    "appearance",
    &["taskbar", "multi-monitor", "grouping"],
    r#"{"mode":"always|when_full|never"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:mmtaskbarglomlevel",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "taskbar.secondary_button_grouping.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "MMTaskbarGlomLevel",
    taskbar_secondary_grouping_desired,
    three_value_is_valid,
    "別モニターのタスクバーボタン結合を選択状態へ変更します。"
);

action_metadata!(
    START_SHOW_ALL_PINS_METADATA,
    START_SHOW_ALL_PINS_ACTION,
    StartShowAllPins,
    "スタートですべてのピンを最初に表示する",
    "スタートを開いたとき、すべてのピン一覧を先に表示するかを切り替えます。ピン内容は変更しません。",
    "appearance",
    &["start", "pins", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/start:showallpinslist",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "start.show_all_pins.v1",
    "中。Windows更新後にスタート画面の実機スモークを再実施します。",
    START_SUBKEY,
    "ShowAllPinsList",
    start_show_all_pins_desired,
    boolean_value_is_valid,
    "スタートですべてのピンを最初に表示する状態を切り替えます。"
);

action_metadata!(
    START_RECENT_APPS_METADATA,
    START_RECENT_APPS_ACTION,
    StartRecentApps,
    "スタートに最近追加したアプリを表示する",
    "最近インストールしたアプリの一覧表示だけを切り替え、アプリ自体は変更しません。",
    "appearance",
    &["start", "recent-apps", "documented"],
    r#"{"show":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/start:showrecentlist",
    Safe,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "start.recent_apps.v1",
    "中。Windows更新後にスタート画面の実機スモークを再実施します。",
    START_SUBKEY,
    "ShowRecentList",
    start_recent_apps_desired,
    boolean_value_is_valid,
    "スタートの最近追加したアプリ表示を選択状態へ変更します。"
);

action_metadata!(
    ACCENT_START_TASKBAR_METADATA,
    ACCENT_START_TASKBAR_ACTION,
    AppearanceAccentStartTaskbar,
    "スタートとタスクバーにアクセント色を表示する",
    "選択済みのWindowsアクセント色をスタートとタスクバーへ表示するかを切り替えます。色そのものは変更しません。",
    "appearance",
    &["accent", "start", "taskbar"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/themes/personalize:colorprevalence",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "appearance.accent_start_taskbar.v1",
    "中。テーマ更新後に高コントラストを含む実機スモークを再実施します。",
    PERSONALIZE_SUBKEY,
    "ColorPrevalence",
    accent_start_taskbar_desired,
    boolean_value_is_valid,
    "現在のアクセント色をスタートとタスクバーに表示する状態を切り替えます。"
);

action_metadata!(
    ACCENT_TITLE_BARS_METADATA,
    ACCENT_TITLE_BARS_ACTION,
    AppearanceAccentTitleBars,
    "タイトルバーとウィンドウ枠にアクセント色を表示する",
    "選択済みのアクセント色を対応するタイトルバーとウィンドウ枠へ表示するかを切り替えます。",
    "appearance",
    &["accent", "title-bar", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/dwm:colorprevalence",
    Safe,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "appearance.accent_title_bars.v1",
    "中。テーマ更新後に高コントラストを含む実機スモークを再実施します。",
    r"Software\Microsoft\Windows\DWM",
    "ColorPrevalence",
    accent_title_bars_desired,
    boolean_value_is_valid,
    "現在のアクセント色をタイトルバーと枠に表示する状態を切り替えます。"
);

action_metadata!(
    AUTO_ACCENT_METADATA,
    AUTO_ACCENT_ACTION,
    AppearanceAutoAccent,
    "背景に合わせてアクセント色を自動選択する",
    "デスクトップ背景に合わせたWindowsの自動アクセント選択を切り替えます。任意色の直接書込みは行いません。",
    "appearance",
    &["accent", "wallpaper", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:control panel/desktop:autocolorization",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "appearance.auto_accent.v1",
    "中。テーマ更新後に自動色選択の実機スモークを再実施します。",
    r"Control Panel\Desktop",
    "AutoColorization",
    auto_accent_desired,
    boolean_value_is_valid,
    "背景に合わせたアクセント色の自動選択を切り替えます。"
);

action_metadata!(
    GAME_MODE_METADATA,
    GAME_MODE_ACTION,
    GamesGameMode,
    "Windows Game Modeを切り替える",
    "Windows標準のGame Mode設定だけを切り替えます。サービス停止やゲーム改変は行いません。",
    "games",
    &["game-mode", "gaming", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/gamebar:autogamemodeenabled",
    Caution,
    &[WINDOWS_SETTINGS_REFERENCE],
    "games.game_mode.v1",
    "中。Gaming設定更新後に設定UIとの往復を再検証します。",
    r"Software\Microsoft\GameBar",
    "AutoGameModeEnabled",
    game_mode_desired,
    boolean_value_is_valid,
    "Windows標準のGame Modeを選択状態へ変更します。"
);

action_metadata!(
    CONTROLLER_GAME_BAR_METADATA,
    CONTROLLER_GAME_BAR_ACTION,
    GamesControllerGameBar,
    "コントローラーからGame Barを開く操作を切り替える",
    "コントローラーのXboxボタンでGame Barを開く設定だけを切り替えます。入力の注入は行いません。",
    "games",
    &["game-bar", "controller", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/gamebar:usenexusforgamebarenabled",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "games.controller_game_bar.v1",
    "中。Gaming設定更新後に設定UIとの往復を再検証します。",
    r"Software\Microsoft\GameBar",
    "UseNexusForGameBarEnabled",
    controller_game_bar_desired,
    boolean_value_is_valid,
    "コントローラーからGame Barを開く操作を選択状態へ変更します。"
);

action_metadata!(
    AUTOPLAY_METADATA,
    AUTOPLAY_ACTION,
    DevicesAutoplay,
    "メディアとデバイスの自動再生を切り替える",
    "リムーバブルメディアの自動再生マスター設定を切り替えます。個別の既定アプリは変更しません。",
    "setup",
    &["autoplay", "devices", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/autoplayhandlers:disableautoplay",
    Safe,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "devices.autoplay.v1",
    "中。デバイス設定更新後に設定UIとの往復を再検証します。",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers",
    "DisableAutoplay",
    autoplay_desired,
    boolean_value_is_valid,
    "メディアとデバイスの自動再生を選択状態へ変更します。"
);

action_metadata!(
    USB_ERRORS_METADATA,
    USB_ERRORS_ACTION,
    NotificationsUsbErrors,
    "USBエラーの通知を表示する",
    "USB接続エラーのWindows通知だけを切り替えます。USB機能やドライバーは変更しません。",
    "notifications",
    &["usb", "notifications", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/shell/usb:notifyonusberrors",
    Safe,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "notifications.usb_errors.v1",
    "低。通知設定の公開値だけを使用します。",
    r"Software\Microsoft\Shell\USB",
    "NotifyOnUsbErrors",
    usb_errors_desired,
    boolean_value_is_valid,
    "USBエラー通知を選択状態へ変更します。"
);

action_metadata!(
    WEAK_CHARGER_METADATA,
    WEAK_CHARGER_ACTION,
    NotificationsWeakCharger,
    "低出力充電器の通知を表示する",
    "充電器の出力不足を知らせるWindows通知だけを切り替えます。充電制御は変更しません。",
    "notifications",
    &["usb", "charger", "notifications"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/shell/usb:notifyonweakcharger",
    Safe,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "notifications.weak_charger.v1",
    "低。通知設定の公開値だけを使用します。",
    r"Software\Microsoft\Shell\USB",
    "NotifyOnWeakCharger",
    weak_charger_desired,
    boolean_value_is_valid,
    "低出力充電器の通知を選択状態へ変更します。"
);

action_metadata!(
    AUTOCORRECT_METADATA,
    AUTOCORRECT_ACTION,
    InputAutocorrect,
    "タッチキーボードの自動修正を切り替える",
    "Windowsのタッチ入力に対する自動修正だけを切り替えます。物理キーボードの配列は変更しません。",
    "input",
    &["typing", "autocorrect", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/input/settings:enableautocorrection",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "input.autocorrect.v1",
    "低。入力設定の公開値だけを使用します。",
    r"Software\Microsoft\input\Settings",
    "EnableAutocorrection",
    autocorrect_desired,
    boolean_value_is_valid,
    "タッチキーボードの自動修正を選択状態へ変更します。"
);

action_metadata!(
    DOUBLE_SPACE_METADATA,
    DOUBLE_SPACE_ACTION,
    InputDoubleSpacePeriod,
    "スペース2回でピリオドを入力する",
    "タッチキーボードでスペースを2回押したときのピリオド入力を切り替えます。",
    "input",
    &["typing", "touch-keyboard", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/input/settings:enabledoubletapspace",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "input.double_space_period.v1",
    "低。入力設定の公開値だけを使用します。",
    r"Software\Microsoft\input\Settings",
    "EnableDoubleTapSpace",
    double_space_desired,
    boolean_value_is_valid,
    "スペース2回のピリオド入力を選択状態へ変更します。"
);

action_metadata!(
    AUTO_SHIFT_METADATA,
    AUTO_SHIFT_ACTION,
    InputAutoShift,
    "タッチキーボードの自動Shiftを切り替える",
    "文頭などでタッチキーボードがShiftを自動的に有効にする動作を切り替えます。",
    "input",
    &["typing", "touch-keyboard", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/input/settings:enableautoshiftengage",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "input.auto_shift.v1",
    "低。入力設定の公開値だけを使用します。",
    r"Software\Microsoft\input\Settings",
    "EnableAutoShiftEngage",
    auto_shift_desired,
    boolean_value_is_valid,
    "タッチキーボードの自動Shiftを選択状態へ変更します。"
);

action_metadata!(
    VOICE_TYPING_KEY_METADATA,
    VOICE_TYPING_KEY_ACTION,
    InputVoiceTypingKey,
    "音声入力キーを表示する",
    "タッチキーボードの音声入力キー表示だけを切り替えます。マイク権限は変更しません。",
    "input",
    &["typing", "voice", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/input/settings:isvoicetypingkeyenabled",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "input.voice_typing_key.v1",
    "低。入力設定の公開値だけを使用します。",
    r"Software\Microsoft\input\Settings",
    "IsVoiceTypingKeyEnabled",
    voice_typing_key_desired,
    boolean_value_is_valid,
    "タッチキーボードの音声入力キーを選択状態へ変更します。"
);

action_metadata!(
    MULTILINGUAL_METADATA,
    MULTILINGUAL_SUGGESTIONS_ACTION,
    InputMultilingualSuggestions,
    "多言語の入力候補を切り替える",
    "Windowsの多言語入力候補を切り替えます。インストール済み言語や辞書は変更しません。",
    "input",
    &["typing", "multilingual", "documented"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/input/settings:multilingualenabled",
    Safe,
    &[WINDOWS_SETTINGS_REFERENCE],
    "input.multilingual_suggestions.v1",
    "低。入力設定の公開値だけを使用します。",
    r"Software\Microsoft\input\Settings",
    "MultilingualEnabled",
    multilingual_desired,
    boolean_value_is_valid,
    "多言語入力候補を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_STATUS_BAR_METADATA,
    EXPLORER_STATUS_BAR_ACTION,
    ExplorerStatusBar,
    "Explorerのステータスバーを表示する",
    "Explorer下部の項目数などを示すステータスバー表示を切り替えます。ファイルには触れません。",
    "explorer",
    &["explorer", "status-bar", "explicit-only"],
    r#"{"show":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:showstatusbar",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.status_bar.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "ShowStatusBar",
    status_bar_desired,
    boolean_value_is_valid,
    "Explorerのステータスバー表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_INFO_TIPS_METADATA,
    EXPLORER_INFO_TIPS_ACTION,
    ExplorerInfoTips,
    "Explorerのファイル説明を表示する",
    "ファイルへポインターを置いたときの説明表示を切り替えます。ファイル内容は変更しません。",
    "explorer",
    &["explorer", "info-tip", "explicit-only"],
    r#"{"show":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:showinfotip",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.info_tips.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "ShowInfoTip",
    info_tips_desired,
    boolean_value_is_valid,
    "Explorerのファイル説明表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_HIDE_EMPTY_DRIVES_METADATA,
    EXPLORER_HIDE_EMPTY_DRIVES_ACTION,
    ExplorerHideEmptyDrives,
    "空のリムーバブルドライブを隠す",
    "メディアが入っていないリムーバブルドライブの表示だけを切り替えます。ドライブは無効化しません。",
    "explorer",
    &["explorer", "drives", "explicit-only"],
    r#"{"hide":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:hidedriveswithnomedia",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.hide_empty_drives.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "HideDrivesWithNoMedia",
    hide_empty_drives_desired,
    boolean_value_is_valid,
    "空のリムーバブルドライブ表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_NAV_EXPAND_CURRENT_METADATA,
    EXPLORER_NAV_EXPAND_CURRENT_ACTION,
    ExplorerNavExpandCurrent,
    "ナビゲーションを現在のフォルダーまで展開する",
    "Explorer左側のナビゲーションを現在位置まで自動展開する動作を切り替えます。",
    "explorer",
    &["explorer", "navigation", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:navpaneexpandtocurrentfolder",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.nav_expand_current.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "NavPaneExpandToCurrentFolder",
    nav_expand_current_desired,
    boolean_value_is_valid,
    "ナビゲーションの現在位置までの自動展開を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_NAV_SHOW_ALL_METADATA,
    EXPLORER_NAV_SHOW_ALL_ACTION,
    ExplorerNavShowAll,
    "ナビゲーションにすべてのフォルダーを表示する",
    "Explorer左側のナビゲーションにすべてのフォルダーを出す表示を切り替えます。",
    "explorer",
    &["explorer", "navigation", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:navpaneshowallfolders",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.nav_show_all.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "NavPaneShowAllFolders",
    nav_show_all_desired,
    boolean_value_is_valid,
    "ナビゲーションのすべてのフォルダー表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_SEPARATE_PROCESS_METADATA,
    EXPLORER_SEPARATE_PROCESS_ACTION,
    ExplorerSeparateProcess,
    "フォルダーウィンドウを別プロセスで開く",
    "Explorerのフォルダーウィンドウ分離設定を切り替えます。既存プロセスの強制終了は行いません。",
    "explorer",
    &["explorer", "process", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:separateprocess",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.separate_process.v1",
    "中。次回開くExplorerウィンドウで実機反映を再検証します。",
    ADVANCED_SUBKEY,
    "SeparateProcess",
    separate_process_desired,
    boolean_value_is_valid,
    "フォルダーウィンドウのプロセス分離を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_ICONS_ONLY_METADATA,
    EXPLORER_ICONS_ONLY_ACTION,
    ExplorerIconsOnly,
    "サムネイルを使わず常にアイコンを表示する",
    "Explorerのサムネイル表示をアイコンだけに切り替えます。サムネイルキャッシュは削除しません。",
    "explorer",
    &["explorer", "thumbnails", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:iconsonly",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.icons_only.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "IconsOnly",
    icons_only_desired,
    boolean_value_is_valid,
    "Explorerをアイコンだけで表示する状態を切り替えます。"
);

action_metadata!(
    EXPLORER_DRIVE_LETTERS_METADATA,
    EXPLORER_DRIVE_LETTERS_ACTION,
    ExplorerDriveLetters,
    "Explorerにドライブ文字を表示する",
    "ドライブ名の横にC:などのドライブ文字を表示する設定を切り替えます。割り当て自体は変更しません。",
    "explorer",
    &["explorer", "drives", "explicit-only"],
    r#"{"show":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:showdriveletters",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.drive_letters.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "ShowDriveLetters",
    drive_letters_desired,
    boolean_value_is_valid,
    "Explorerのドライブ文字表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_PREVIEW_HANDLERS_METADATA,
    EXPLORER_PREVIEW_HANDLERS_ACTION,
    ExplorerPreviewHandlers,
    "プレビューウィンドウでファイル内容を表示する",
    "登録済みプレビューハンドラーの利用を切り替えます。ハンドラーの追加や任意DLL実行は行いません。",
    "explorer",
    &["explorer", "preview", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:showpreviewhandlers",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.preview_handlers.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "ShowPreviewHandlers",
    preview_handlers_desired,
    boolean_value_is_valid,
    "登録済みプレビューハンドラーの表示を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_SHARING_WIZARD_METADATA,
    EXPLORER_SHARING_WIZARD_ACTION,
    ExplorerSharingWizard,
    "Explorerの共有ウィザードを使う",
    "ファイル共有の案内ウィザード表示を切り替えます。共有権限やネットワーク設定は変更しません。",
    "explorer",
    &["explorer", "sharing", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:sharingwizardon",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.sharing_wizard.v1",
    "中。Explorer更新後に表示反映を再検証します。",
    ADVANCED_SUBKEY,
    "SharingWizardOn",
    sharing_wizard_desired,
    boolean_value_is_valid,
    "Explorerの共有ウィザード利用を選択状態へ変更します。"
);

action_metadata!(
    EXPLORER_ALWAYS_SHOW_MENUS_METADATA,
    EXPLORER_ALWAYS_SHOW_MENUS_ACTION,
    ExplorerAlwaysShowMenus,
    "Explorerのメニューを常に表示する",
    "従来形式のメニュー表示に対応する固定HKCU値を切り替えます。Windows UIへの反映は実機確認まで明示操作のみです。",
    "explorer",
    &["explorer", "menus", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:alwaysshowmenus",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "explorer.always_show_menus.v1",
    "高。Explorer UI変更の影響を受けやすいため実機再検証が必要です。",
    ADVANCED_SUBKEY,
    "AlwaysShowMenus",
    always_show_menus_desired,
    boolean_value_is_valid,
    "Explorerのメニュー常時表示を選択状態へ変更します。"
);

action_metadata!(
    TASKBAR_ANIMATIONS_METADATA,
    TASKBAR_ANIMATIONS_ACTION,
    AppearanceTaskbarAnimations,
    "タスクバーのアニメーションを切り替える",
    "タスクバーの視覚アニメーションに対応する固定HKCU値だけを切り替えます。DWM、サービス、性能設定は変更しません。",
    "appearance",
    &["taskbar", "animations", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/explorer/advanced:taskbaranimations",
    Caution,
    &[WINDOWS_COMMON_SETTINGS_REFERENCE],
    "appearance.taskbar_animations.v1",
    "高。タスクバー更新後はAction固有の実機確認まで自動適用しません。",
    ADVANCED_SUBKEY,
    "TaskbarAnimations",
    taskbar_animations_desired,
    boolean_value_is_valid,
    "タスクバーのアニメーションを選択状態へ変更します。"
);

action_metadata!(
    TOAST_BANNERS_METADATA,
    TOAST_BANNERS_ACTION,
    NotificationsToastBanners,
    "Windowsの通知バナーを切り替える",
    "ユーザー全体の通知バナー表示を明示的に切り替えます。Windows SecurityやUpdateの機能は停止しません。",
    "notifications",
    &["notifications", "toast", "explicit-only"],
    r#"{"enabled":"boolean"}"#,
    "registry:hkcu:64:software/microsoft/windows/currentversion/pushnotifications:toastenabled",
    Caution,
    &["https://learn.microsoft.com/uwp/api/windows.ui.notifications.toastnotifier.setting"],
    "notifications.toast_banners.v1",
    "高。通知基盤更新後はAction固有の実機確認まで自動適用しません。",
    r"Software\Microsoft\Windows\CurrentVersion\PushNotifications",
    "ToastEnabled",
    toast_banners_desired,
    boolean_value_is_valid,
    "Windowsの通知バナー表示を選択状態へ変更します。"
);

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::actions::common::{dword_bytes, REG_DWORD_TYPE};
    use crate::{
        backup::{BackupEnvelope, RegistryBackup, RegistryLocation},
        compatibility::OsIdentity,
        windows::{delete_key_if_empty, delete_value, write_raw_value},
    };

    struct IsolatedKeyCleanup(RegistryLocation);

    impl Drop for IsolatedKeyCleanup {
        fn drop(&mut self) {
            let _ = delete_value(&self.0);
            let _ = delete_key_if_empty(&self.0);
        }
    }

    fn unique_target(value_name: &'static str) -> RegistryTarget {
        let key = Box::leak(
            format!(
                r"Software\PCカスタム\IntegrationTests\RegistrySettings\{}",
                Uuid::new_v4()
            )
            .into_boxed_str(),
        );
        RegistryTarget::current_user_64(key, value_name)
    }

    fn create_empty_key(location: &RegistryLocation) {
        write_raw_value(location, REG_DWORD_TYPE, &dword_bytes(0))
            .expect("create isolated registry key");
        delete_value(location).expect("leave isolated registry key empty");
    }

    fn assert_mutation_blocked(
        action: &'static DwordRegistryAction,
        parameters: ActionParameters,
        original: Option<u32>,
    ) {
        for build in [26_100, 26_200] {
            let target = unique_target(action.target.value_name);
            let location = target.location();
            let _cleanup = IsolatedKeyCleanup(location.clone());
            let isolated_action = DwordRegistryAction::new(
                action.metadata,
                target,
                action.desired,
                action.valid,
                action.result,
            );
            if let Some(original) = original {
                write_raw_value(&location, REG_DWORD_TYPE, &dword_bytes(original))
                    .expect("seed isolated registry setting value");
            } else {
                create_empty_key(&location);
            }
            let os = OsIdentity::from_test_build(build);
            let transaction_id = Uuid::new_v4();
            let item_id = Uuid::new_v4();
            let context = ActionContext {
                os_identity: &os,
                transaction_id,
                item_id,
                observed_at_unix_ms: 1,
                is_elevated: false,
            };

            let before = isolated_action
                .detect_current_state(&context, &parameters)
                .expect("detect isolated storage before blocked mutation");
            let raw_before = read_registry_state(&location)
                .expect("read isolated raw storage before blocked mutation");

            let validate_error = isolated_action
                .validate(&context, &parameters)
                .expect_err("evidence-pending setter must fail validation");
            assert_eq!(validate_error.code, ActionErrorCode::CompatibilityBlocked);
            assert_eq!(validate_error.stage, ActionStage::Validate);

            let backup_error = isolated_action
                .create_backup(&context, &parameters)
                .expect_err("evidence-pending setter must not create a backup");
            assert_eq!(backup_error.code, ActionErrorCode::CompatibilityBlocked);
            assert_eq!(backup_error.stage, ActionStage::Backup);

            // Even a caller-supplied, integrity-valid envelope cannot reach a
            // write path. This exercises the handler guard independently of
            // the engine compatibility classifier.
            let intended_raw = dword_bytes(
                (isolated_action.desired)(&parameters)
                    .expect("derive candidate storage value for rejection test"),
            );
            let backup = RegistryBackup {
                location: location.clone(),
                original: raw_before.clone(),
                intended_type: REG_DWORD_TYPE,
                intended_raw: intended_raw.clone(),
                applied_type: REG_DWORD_TYPE,
                applied_raw: intended_raw,
                action_version: action.metadata.action_version,
                windows_build: build,
            };
            let draft = BackupDraft {
                precondition_fingerprint: backup.original.fingerprint(&location),
                intended_fingerprint: backup.intended_state().fingerprint(&location),
                payload: BackupPayload::Registry(backup),
            };
            let envelope = BackupEnvelope::from_draft(
                draft,
                transaction_id,
                item_id,
                action.metadata.id,
                action.metadata.action_version,
                1,
                os.base_build,
            );
            let apply_error = isolated_action
                .apply(&context, &parameters, &envelope)
                .expect_err("evidence-pending setter must reject direct apply");
            assert_eq!(apply_error.code, ActionErrorCode::CompatibilityBlocked);
            assert_eq!(apply_error.stage, ActionStage::Apply);

            let after = isolated_action
                .detect_current_state(&context, &parameters)
                .expect("detect isolated storage after blocked mutation");
            let raw_after = read_registry_state(&location)
                .expect("read isolated raw storage after blocked mutation");
            assert_eq!(before.known_value(), after.known_value());
            assert_eq!(raw_before, raw_after);
        }
    }

    #[test]
    fn missing_registry_key_is_rejected_before_backup_or_mutation() {
        let target = unique_target("MissingKey");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        let action = DwordRegistryAction::new(
            START_RECOMMENDATIONS_ACTION.metadata,
            target,
            START_RECOMMENDATIONS_ACTION.desired,
            START_RECOMMENDATIONS_ACTION.valid,
            START_RECOMMENDATIONS_ACTION.result,
        );
        let os = OsIdentity::from_test_build(26_100);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let parameters = ActionParameters::StartRecommendations { enabled: true };

        let error = action
            .create_backup(&context, &parameters)
            .expect_err("a missing containing key must fail closed");
        assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
        assert_eq!(error.stage, ActionStage::Backup);
        let after = read_registry_state(&location).expect("read missing key after rejection");
        assert!(!after.key_existed);
        assert!(!after.value_existed);
    }

    // Each candidate proves that validate/backup/apply are blocked and raw
    // storage is unchanged. No apply/rollback round trip is executed.
    macro_rules! blocked_mutation_test {
        ($name:ident, $action:ident, $parameters:expr, $original:expr) => {
            #[test]
            fn $name() {
                assert_mutation_blocked(&$action, $parameters, $original);
            }
        };
    }

    #[test]
    fn wrong_registry_type_is_unknown_and_never_overwritten() {
        let target = unique_target("WrongType");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        let original_raw = vec![b'n', 0, b'o', 0, b't', 0, 0, 0];
        write_raw_value(&location, 1, &original_raw).expect("seed REG_SZ raw value");
        let action = DwordRegistryAction::new(
            START_RECOMMENDATIONS_ACTION.metadata,
            target,
            START_RECOMMENDATIONS_ACTION.desired,
            START_RECOMMENDATIONS_ACTION.valid,
            START_RECOMMENDATIONS_ACTION.result,
        );
        let os = OsIdentity::from_test_build(26_100);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let parameters = ActionParameters::StartRecommendations { enabled: false };

        assert!(matches!(
            action
                .detect_current_state(&context, &parameters)
                .expect("detect wrong type without mutation"),
            DetectedState::Unknown { .. }
        ));
        let error = action
            .create_backup(&context, &parameters)
            .expect_err("wrong type must fail before backup or mutation");
        assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
        assert_eq!(error.stage, ActionStage::Backup);
        let after = read_registry_state(&location).expect("read wrong type after rejection");
        assert_eq!(after.value_type, Some(1));
        assert_eq!(after.raw_bytes, original_raw);
    }

    #[test]
    fn explorer_launch_target_zero_is_unknown_and_never_overwritten() {
        let target = unique_target("LaunchToZero");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        write_raw_value(&location, REG_DWORD_TYPE, &dword_bytes(0))
            .expect("seed unsupported LaunchTo value");
        let action = DwordRegistryAction::new(
            EXPLORER_LAUNCH_TARGET_ACTION.metadata,
            target,
            EXPLORER_LAUNCH_TARGET_ACTION.desired,
            EXPLORER_LAUNCH_TARGET_ACTION.valid,
            EXPLORER_LAUNCH_TARGET_ACTION.result,
        );
        let os = OsIdentity::from_test_build(26_100);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let parameters = ActionParameters::ExplorerLaunchTarget {
            target: crate::action::ExplorerLaunchTarget::ThisPc,
        };

        assert!(matches!(
            action
                .detect_current_state(&context, &parameters)
                .expect("detect unsupported LaunchTo value"),
            DetectedState::Unknown { .. }
        ));
        let error = action
            .create_backup(&context, &parameters)
            .expect_err("unsupported LaunchTo value must fail closed");
        assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
        assert_eq!(error.stage, ActionStage::Backup);
        let after = read_registry_state(&location).expect("read LaunchTo after rejection");
        assert_eq!(after.value_type, Some(REG_DWORD_TYPE));
        assert_eq!(after.raw_bytes, dword_bytes(0));
    }

    #[test]
    fn rollback_refuses_third_party_value_and_preserves_it() {
        let target = unique_target("ExternalConflict");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        write_raw_value(&location, REG_DWORD_TYPE, &dword_bytes(0)).expect("seed original DWORD");
        let action = DwordRegistryAction::new(
            START_RECOMMENDATIONS_ACTION.metadata,
            target,
            START_RECOMMENDATIONS_ACTION.desired,
            START_RECOMMENDATIONS_ACTION.valid,
            START_RECOMMENDATIONS_ACTION.result,
        );
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
        let parameters = ActionParameters::StartRecommendations { enabled: true };
        let original = read_registry_state(&location).expect("read original DWORD");
        let applied_raw = dword_bytes(1);
        let backup = RegistryBackup {
            location: location.clone(),
            original: original.clone(),
            intended_type: REG_DWORD_TYPE,
            intended_raw: applied_raw.clone(),
            applied_type: REG_DWORD_TYPE,
            applied_raw: applied_raw.clone(),
            action_version: action.metadata.action_version,
            windows_build: os.base_build,
        };
        let applied_fingerprint = backup.applied_state().fingerprint(&location);
        let draft = BackupDraft {
            precondition_fingerprint: original.fingerprint(&location),
            intended_fingerprint: backup.intended_state().fingerprint(&location),
            payload: BackupPayload::Registry(backup),
        };
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            action.metadata.id,
            action.metadata.action_version,
            1,
            os.base_build,
        );
        envelope.record_applied(applied_fingerprint);
        write_raw_value(&location, REG_DWORD_TYPE, &applied_raw)
            .expect("seed a previously applied durable value");

        let third_party_raw = vec![b'x', 0, 0, 0];
        write_raw_value(&location, 1, &third_party_raw)
            .expect("simulate third-party registry change");
        let error = action
            .rollback(&context, &parameters, &envelope)
            .expect_err("rollback must not overwrite third-party state");
        assert_eq!(error.code, ActionErrorCode::ExternalConflict);
        let after = read_registry_state(&location).expect("read preserved third-party state");
        assert_eq!(after.value_type, Some(1));
        assert_eq!(after.raw_bytes, third_party_raw);
    }

    #[test]
    fn legacy_missing_key_rollback_is_reported_as_recovery_required() {
        let target = unique_target("LegacyMissingKey");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        let mut sibling = location.clone();
        sibling.value_name = "SiblingValue".to_owned();
        let applied_raw = dword_bytes(1);
        let backup = RegistryBackup {
            location: location.clone(),
            original: crate::backup::RegistryValueState {
                key_existed: false,
                value_existed: false,
                value_type: None,
                raw_bytes: Vec::new(),
            },
            intended_type: REG_DWORD_TYPE,
            intended_raw: applied_raw.clone(),
            applied_type: REG_DWORD_TYPE,
            applied_raw: applied_raw.clone(),
            action_version: START_RECOMMENDATIONS_ACTION.metadata.action_version,
            windows_build: 26_100,
        };
        let applied_fingerprint = backup.applied_state().fingerprint(&location);
        let draft = BackupDraft {
            precondition_fingerprint: backup.original.fingerprint(&location),
            intended_fingerprint: backup.intended_state().fingerprint(&location),
            payload: BackupPayload::Registry(backup),
        };
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
        let parameters = ActionParameters::StartRecommendations { enabled: true };
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            START_RECOMMENDATIONS_ACTION.metadata.id,
            START_RECOMMENDATIONS_ACTION.metadata.action_version,
            1,
            os.base_build,
        );
        envelope.record_applied(applied_fingerprint);
        write_raw_value(&location, REG_DWORD_TYPE, &applied_raw)
            .expect("seed legacy applied target");
        let sibling_raw = vec![0xaa, 0xbb, 0xcc];
        write_raw_value(&sibling, 3, &sibling_raw).expect("seed sibling value");

        let action = DwordRegistryAction::new(
            START_RECOMMENDATIONS_ACTION.metadata,
            target,
            START_RECOMMENDATIONS_ACTION.desired,
            START_RECOMMENDATIONS_ACTION.valid,
            START_RECOMMENDATIONS_ACTION.result,
        );
        let error = action
            .rollback(&context, &parameters, &envelope)
            .expect_err("legacy key retention must require recovery acknowledgement");
        assert_eq!(error.code, ActionErrorCode::RecoveryRequired);
        let target_after =
            read_registry_state(&location).expect("read legacy target after rollback");
        assert!(target_after.key_existed);
        assert!(!target_after.value_existed);
        let sibling_after = read_registry_state(&sibling).expect("read preserved sibling");
        assert_eq!(sibling_after.value_type, Some(3));
        assert_eq!(sibling_after.raw_bytes, sibling_raw);

        delete_value(&sibling).expect("remove isolated sibling");
    }

    #[test]
    fn legacy_missing_key_backup_cannot_be_applied() {
        let target = unique_target("LegacyApplyBlocked");
        let location = target.location();
        let _cleanup = IsolatedKeyCleanup(location.clone());
        let applied_raw = dword_bytes(1);
        let backup = RegistryBackup {
            location: location.clone(),
            original: crate::backup::RegistryValueState {
                key_existed: false,
                value_existed: false,
                value_type: None,
                raw_bytes: Vec::new(),
            },
            intended_type: REG_DWORD_TYPE,
            intended_raw: applied_raw.clone(),
            applied_type: REG_DWORD_TYPE,
            applied_raw,
            action_version: 1,
            windows_build: 26_100,
        };

        let draft = BackupDraft {
            precondition_fingerprint: backup.original.fingerprint(&location),
            intended_fingerprint: backup.intended_state().fingerprint(&location),
            payload: BackupPayload::Registry(backup),
        };
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
        let envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            START_RECOMMENDATIONS_ACTION.metadata.id,
            START_RECOMMENDATIONS_ACTION.metadata.action_version,
            1,
            os.base_build,
        );
        let error = START_RECOMMENDATIONS_ACTION
            .apply(
                &context,
                &ActionParameters::StartRecommendations { enabled: true },
                &envelope,
            )
            .expect_err("an evidence-pending setter must reject even a legacy envelope");
        assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
        assert_eq!(error.stage, ActionStage::Apply);
        let after = read_registry_state(&location).expect("read key after blocked legacy apply");
        assert!(!after.key_existed);
    }

    #[test]
    fn grouping_actions_reject_each_others_parameter_variant() {
        let secondary = ActionParameters::TaskbarSecondaryButtonGrouping {
            mode: crate::action::TaskbarGroupingMode::Never,
        };
        let primary = ActionParameters::TaskbarButtonGrouping {
            mode: crate::action::TaskbarGroupingMode::Never,
        };
        assert_eq!(
            TASKBAR_BUTTON_GROUPING_ACTION
                .explain_changes(&secondary)
                .expect_err("primary grouping must reject secondary parameters")
                .code,
            ActionErrorCode::WrongParameters
        );
        assert_eq!(
            TASKBAR_SECONDARY_BUTTON_GROUPING_ACTION
                .explain_changes(&primary)
                .expect_err("secondary grouping must reject primary parameters")
                .code,
            ActionErrorCode::WrongParameters
        );
    }

    #[test]
    fn enum_and_inverted_boolean_storage_mappings_are_fixed_oracles() {
        use crate::action::{
            ExplorerLaunchTarget, TaskbarAlignment, TaskbarGroupingMode, TaskbarMultiMonitorMode,
            TaskbarSearchMode,
        };

        for (mode, expected) in [
            (TaskbarSearchMode::Hidden, 0),
            (TaskbarSearchMode::Icon, 1),
            (TaskbarSearchMode::IconAndLabel, 2),
            (TaskbarSearchMode::SearchBox, 3),
        ] {
            assert_eq!(
                taskbar_search_mode_desired(&ActionParameters::TaskbarSearchMode { mode })
                    .expect("map search mode"),
                expected
            );
        }
        assert_eq!(
            taskbar_alignment_desired(&ActionParameters::TaskbarAlignment {
                alignment: TaskbarAlignment::Left,
            })
            .expect("map left alignment"),
            0
        );
        assert_eq!(
            taskbar_alignment_desired(&ActionParameters::TaskbarAlignment {
                alignment: TaskbarAlignment::Center,
            })
            .expect("map center alignment"),
            1
        );
        for (target, expected) in [
            (ExplorerLaunchTarget::ThisPc, 1),
            (ExplorerLaunchTarget::Home, 2),
            (ExplorerLaunchTarget::Downloads, 3),
        ] {
            assert_eq!(
                explorer_launch_target_desired(&ActionParameters::ExplorerLaunchTarget { target })
                    .expect("map Explorer launch target"),
                expected
            );
        }
        for (mode, expected) in [
            (TaskbarGroupingMode::Always, 0),
            (TaskbarGroupingMode::WhenFull, 1),
            (TaskbarGroupingMode::Never, 2),
        ] {
            assert_eq!(
                taskbar_grouping_desired(&ActionParameters::TaskbarButtonGrouping { mode })
                    .expect("map primary grouping"),
                expected
            );
            assert_eq!(
                taskbar_secondary_grouping_desired(
                    &ActionParameters::TaskbarSecondaryButtonGrouping { mode },
                )
                .expect("map secondary grouping"),
                expected
            );
        }
        for (mode, expected) in [
            (TaskbarMultiMonitorMode::AllTaskbars, 0),
            (TaskbarMultiMonitorMode::MainAndWindow, 1),
            (TaskbarMultiMonitorMode::WindowMonitor, 2),
        ] {
            assert_eq!(
                taskbar_multi_monitor_mode_desired(&ActionParameters::TaskbarMultiMonitorMode {
                    mode
                },)
                .expect("map multi-monitor taskbar mode"),
                expected
            );
        }
        assert_eq!(
            autoplay_desired(&ActionParameters::DevicesAutoplay { enabled: true })
                .expect("map enabled AutoPlay"),
            0
        );
        assert_eq!(
            autoplay_desired(&ActionParameters::DevicesAutoplay { enabled: false })
                .expect("map disabled AutoPlay"),
            1
        );
    }

    blocked_mutation_test!(
        taskbar_search_mode_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_SEARCH_MODE_ACTION,
        ActionParameters::TaskbarSearchMode {
            mode: crate::action::TaskbarSearchMode::SearchBox
        },
        Some(1)
    );
    blocked_mutation_test!(
        taskbar_alignment_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_ALIGNMENT_ACTION,
        ActionParameters::TaskbarAlignment {
            alignment: crate::action::TaskbarAlignment::Left
        },
        Some(1)
    );
    blocked_mutation_test!(
        start_layout_mutation_is_blocked_and_storage_is_unchanged,
        START_LAYOUT_ACTION,
        ActionParameters::StartLayout {
            layout: crate::action::StartLayout::MorePins
        },
        Some(0)
    );
    blocked_mutation_test!(
        start_recommendations_mutation_is_blocked_and_storage_is_unchanged,
        START_RECOMMENDATIONS_ACTION,
        ActionParameters::StartRecommendations { enabled: false },
        Some(1)
    );
    blocked_mutation_test!(
        explorer_launch_target_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_LAUNCH_TARGET_ACTION,
        ActionParameters::ExplorerLaunchTarget {
            target: crate::action::ExplorerLaunchTarget::ThisPc
        },
        Some(2)
    );
    blocked_mutation_test!(
        explorer_recent_files_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_RECENT_FILES_ACTION,
        ActionParameters::ExplorerRecentFiles { show: true },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_grouping_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_BUTTON_GROUPING_ACTION,
        ActionParameters::TaskbarButtonGrouping {
            mode: crate::action::TaskbarGroupingMode::WhenFull
        },
        Some(2)
    );
    blocked_mutation_test!(
        taskbar_flashing_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_FLASHING_ACTION,
        ActionParameters::TaskbarFlashing { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_share_window_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_SHARE_WINDOW_ACTION,
        ActionParameters::TaskbarShareWindow { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_show_desktop_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_SHOW_DESKTOP_ACTION,
        ActionParameters::TaskbarShowDesktop { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        search_recent_hover_mutation_is_blocked_and_storage_is_unchanged,
        SEARCH_RECENT_ON_HOVER_ACTION,
        ActionParameters::SearchRecentOnHover { enabled: false },
        Some(1)
    );
    blocked_mutation_test!(
        taskbar_multi_monitor_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_MULTI_MONITOR_ACTION,
        ActionParameters::TaskbarMultiMonitor { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_multi_monitor_mode_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_MULTI_MONITOR_MODE_ACTION,
        ActionParameters::TaskbarMultiMonitorMode {
            mode: crate::action::TaskbarMultiMonitorMode::WindowMonitor
        },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_secondary_grouping_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_SECONDARY_BUTTON_GROUPING_ACTION,
        ActionParameters::TaskbarSecondaryButtonGrouping {
            mode: crate::action::TaskbarGroupingMode::WhenFull
        },
        Some(2)
    );
    blocked_mutation_test!(
        start_show_all_pins_mutation_is_blocked_and_storage_is_unchanged,
        START_SHOW_ALL_PINS_ACTION,
        ActionParameters::StartShowAllPins { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        start_recent_apps_mutation_is_blocked_and_storage_is_unchanged,
        START_RECENT_APPS_ACTION,
        ActionParameters::StartRecentApps { show: true },
        None
    );
    blocked_mutation_test!(
        accent_start_taskbar_mutation_is_blocked_and_storage_is_unchanged,
        ACCENT_START_TASKBAR_ACTION,
        ActionParameters::AppearanceAccentStartTaskbar { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        accent_title_bars_mutation_is_blocked_and_storage_is_unchanged,
        ACCENT_TITLE_BARS_ACTION,
        ActionParameters::AppearanceAccentTitleBars { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        auto_accent_mutation_is_blocked_and_storage_is_unchanged,
        AUTO_ACCENT_ACTION,
        ActionParameters::AppearanceAutoAccent { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        game_mode_mutation_is_blocked_and_storage_is_unchanged,
        GAME_MODE_ACTION,
        ActionParameters::GamesGameMode { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        controller_game_bar_mutation_is_blocked_and_storage_is_unchanged,
        CONTROLLER_GAME_BAR_ACTION,
        ActionParameters::GamesControllerGameBar { enabled: false },
        Some(1)
    );
    blocked_mutation_test!(
        autoplay_mutation_is_blocked_and_storage_is_unchanged,
        AUTOPLAY_ACTION,
        ActionParameters::DevicesAutoplay { enabled: true },
        Some(1)
    );
    blocked_mutation_test!(
        usb_errors_mutation_is_blocked_and_storage_is_unchanged,
        USB_ERRORS_ACTION,
        ActionParameters::NotificationsUsbErrors { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        weak_charger_mutation_is_blocked_and_storage_is_unchanged,
        WEAK_CHARGER_ACTION,
        ActionParameters::NotificationsWeakCharger { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        autocorrect_mutation_is_blocked_and_storage_is_unchanged,
        AUTOCORRECT_ACTION,
        ActionParameters::InputAutocorrect { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        double_space_mutation_is_blocked_and_storage_is_unchanged,
        DOUBLE_SPACE_ACTION,
        ActionParameters::InputDoubleSpacePeriod { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        auto_shift_mutation_is_blocked_and_storage_is_unchanged,
        AUTO_SHIFT_ACTION,
        ActionParameters::InputAutoShift { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        voice_typing_key_mutation_is_blocked_and_storage_is_unchanged,
        VOICE_TYPING_KEY_ACTION,
        ActionParameters::InputVoiceTypingKey { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        multilingual_mutation_is_blocked_and_storage_is_unchanged,
        MULTILINGUAL_SUGGESTIONS_ACTION,
        ActionParameters::InputMultilingualSuggestions { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        status_bar_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_STATUS_BAR_ACTION,
        ActionParameters::ExplorerStatusBar { show: true },
        Some(0)
    );
    blocked_mutation_test!(
        info_tips_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_INFO_TIPS_ACTION,
        ActionParameters::ExplorerInfoTips { show: true },
        Some(0)
    );
    blocked_mutation_test!(
        hide_empty_drives_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_HIDE_EMPTY_DRIVES_ACTION,
        ActionParameters::ExplorerHideEmptyDrives { hide: true },
        Some(0)
    );
    blocked_mutation_test!(
        nav_expand_current_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_NAV_EXPAND_CURRENT_ACTION,
        ActionParameters::ExplorerNavExpandCurrent { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        nav_show_all_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_NAV_SHOW_ALL_ACTION,
        ActionParameters::ExplorerNavShowAll { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        separate_process_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_SEPARATE_PROCESS_ACTION,
        ActionParameters::ExplorerSeparateProcess { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        icons_only_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_ICONS_ONLY_ACTION,
        ActionParameters::ExplorerIconsOnly { enabled: false },
        Some(1)
    );
    blocked_mutation_test!(
        drive_letters_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_DRIVE_LETTERS_ACTION,
        ActionParameters::ExplorerDriveLetters { show: true },
        Some(0)
    );
    blocked_mutation_test!(
        preview_handlers_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_PREVIEW_HANDLERS_ACTION,
        ActionParameters::ExplorerPreviewHandlers { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        sharing_wizard_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_SHARING_WIZARD_ACTION,
        ActionParameters::ExplorerSharingWizard { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        always_show_menus_mutation_is_blocked_and_storage_is_unchanged,
        EXPLORER_ALWAYS_SHOW_MENUS_ACTION,
        ActionParameters::ExplorerAlwaysShowMenus { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        taskbar_animations_mutation_is_blocked_and_storage_is_unchanged,
        TASKBAR_ANIMATIONS_ACTION,
        ActionParameters::AppearanceTaskbarAnimations { enabled: true },
        Some(0)
    );
    blocked_mutation_test!(
        toast_banners_mutation_is_blocked_and_storage_is_unchanged,
        TOAST_BANNERS_ACTION,
        ActionParameters::NotificationsToastBanners { enabled: true },
        Some(0)
    );
}

#[cfg(test)]
mod compatibility_tests {
    use uuid::Uuid;

    use super::*;
    use crate::compatibility::OsIdentity;

    #[test]
    fn evidence_pending_setter_is_blocked_even_when_build_detection_is_available() {
        let os = OsIdentity::from_test_build(99_999);
        let context = ActionContext {
            os_identity: &os,
            transaction_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let error = START_LAYOUT_ACTION
            .validate(
                &context,
                &ActionParameters::StartLayout {
                    layout: crate::action::StartLayout::MorePins,
                },
            )
            .expect_err("unverified setter must fail closed on every build");
        assert_eq!(error.code, ActionErrorCode::CompatibilityBlocked);
        assert_eq!(error.stage, ActionStage::Validate);
    }
}
