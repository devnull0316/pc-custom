use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    action::{
        ActionId, ActionKind, ActionMetadata, ActionParameters, ActionRiskLevel, AppLaunchBundle,
        ChangeExplanation, DetectedState, ExplorerLaunchTarget, GameReadinessObservation,
        MethodClass, ObservedValue, PowerModeChoice, PowerScheme, ReadinessComponent, StartLayout,
        StartupEntrySource, StartupEntryStatus, StartupInventoryObservation, TaskbarAlignment,
        TaskbarGroupingMode, TaskbarMultiMonitorMode, TaskbarSearchMode, ThemeColorMode,
        ThemeObservation,
    },
    compatibility::{CompatibilityDecision, CompatibilityMode, OsIdentity},
    error::{CoreError, CoreResult},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub mode: String,
    pub os_label: String,
    pub build: Option<u32>,
    pub message: String,
    pub recovery_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiIntegrationState {
    pub installed: bool,
    pub version: Option<String>,
    pub launch_available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiActionState {
    pub kind: String,
    pub label: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<UiIntegrationState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResponse {
    pub action_id: String,
    pub state: UiActionState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPresentation {
    pub id: String,
    pub action_version: u32,
    pub name: String,
    pub description: String,
    pub audience: String,
    pub category: String,
    pub tags: Vec<String>,
    pub supported_windows_versions: Vec<String>,
    pub minimum_build: u32,
    pub maximum_tested_build: Option<u32>,
    pub risk_level: String,
    pub requires_admin: bool,
    pub requires_restart: bool,
    pub requires_explorer_restart: bool,
    pub update_impact: String,
    pub reversible: bool,
    pub kind: String,
    pub auto_apply_eligible: bool,
    pub availability: String,
    pub method_class: String,
    pub method_summary: String,
    pub desired_state: String,
    pub current_state: Option<UiActionState>,
    pub detail_points: Vec<String>,
    /// Windows設定アプリの該当ページ（無ければ None）。UIの案内ボタン用。
    pub settings_page: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewActionsRequest {
    pub actions: Vec<PreviewActionRequest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewActionRequest {
    pub action_id: String,
    #[serde(default)]
    pub parameters: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitPreviewRequest {
    pub preview_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackItemRequest {
    pub item_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    pub preview_token: String,
    pub expires_at: String,
    pub os_build: u32,
    pub changes: Vec<PreviewChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewChange {
    pub action_id: String,
    pub title: String,
    pub before: String,
    pub after: String,
    pub method: String,
    pub resource_label: String,
    pub risk_level: String,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub transaction_id: Uuid,
    pub status: String,
    pub message: String,
    pub details: Vec<String>,
    /// 適用できた項目。**その場で元へ戻すために必要**。
    /// これが無かったため、利用者は戻すのにタイムラインまで移動して項目を探す必要があった。
    pub items: Vec<CommitItem>,
}

/// 適用済み1件。`item_id` をそのまま `rollback_item` へ渡せる。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitItem {
    pub item_id: Uuid,
    pub action_id: String,
    pub name: String,
}

pub fn action_presentation(
    metadata: &ActionMetadata,
    compatibility: CompatibilityDecision,
    current_state: Option<DetectedState>,
) -> ActionPresentation {
    let kind = match metadata.kind {
        ActionKind::Persistent => "persistent",
        ActionKind::Session => "session",
        ActionKind::OneWay => "one_way",
        ActionKind::Observation => "observation",
        ActionKind::Guided => "guided",
    };
    let availability = match compatibility.mode {
        CompatibilityMode::TestedMutable
            if matches!(
                metadata.kind,
                ActionKind::Persistent | ActionKind::Session | ActionKind::OneWay
            ) =>
        {
            "mutable"
        }
        CompatibilityMode::TestedMutable | CompatibilityMode::TestedDetectOnly
            if matches!(metadata.kind, ActionKind::Observation | ActionKind::Guided) =>
        {
            "read_only"
        }
        CompatibilityMode::TestedDetectOnly | CompatibilityMode::UnknownBuild => "detect_only",
        CompatibilityMode::Unsupported => "blocked",
        _ => "blocked",
    };
    let mut detail_points = if metadata.kind == ActionKind::OneWay {
        vec![
            "固定allowlistの既知アプリだけを、シェルを介さず直接起動します。".to_owned(),
            "既に起動中のアプリは二重起動しません。".to_owned(),
            "起動したアプリを勝手に終了しないため、このActionは元に戻せません。".to_owned(),
            "ゲームプロファイルの自動適用対象にはしません。".to_owned(),
        ]
    } else if metadata.kind == ActionKind::Guided
        && metadata.method_class == MethodClass::UnverifiedStorage
    {
        vec![
            "setterの一次資料と対象buildの実機UI試験が未承認のため、変更処理を実行しません。"
                .to_owned(),
            "表示するのは固定HKCU DWORDの保存値であり、Windows UIの有効状態を示しません。"
                .to_owned(),
            "validate・backup・applyの各handlerで明示的に変更を拒否します。".to_owned(),
            "ゲームプロファイルの自動適用対象にはしません。".to_owned(),
        ]
    } else if metadata.kind == ActionKind::Guided {
        vec![
            "Windowsが公開している変更APIがないため、PCカスタムから設定値を変更しません。"
                .to_owned(),
            "Windows自身の設定画面を開き、利用者がそこで選択します。".to_owned(),
            "validate・backup・applyの各handlerで明示的に変更を拒否します。".to_owned(),
            "ゲームプロファイルの自動適用対象にはしません。".to_owned(),
        ]
    } else if metadata.kind == ActionKind::Observation {
        vec![
            "読み取り専用で、Windowsの設定やファイルは変更しません。".to_owned(),
            "取得できない値は推測せず、不明または未設定として扱います。".to_owned(),
            "ゲームプロファイルの自動適用対象にはしません。".to_owned(),
        ]
    } else {
        vec![
            "適用前の状態を耐久記録へ保存してから変更します。".to_owned(),
            "適用直後と復元直後に、Windowsから状態を再取得して検証します。".to_owned(),
            "外部変更を検出した場合は、自動で上書きしません。".to_owned(),
        ]
    };
    // 実測（ui_probe）: 外部プロセスからの変更は、すでに開いているExplorerウィンドウを
    // 更新しない。レジストリ直書きでも文書化APIでも同じだった。黙っていると
    // 「適用したのに変わらない」と受け取られるため、先に伝える。
    if matches!(
        metadata.id,
        ActionId::ExplorerShowExtensions
            | ActionId::ExplorerShowHidden
            | ActionId::ExplorerItemCheckboxes
            | ActionId::ExplorerCompactView
    ) {
        detail_points.push(
            "すでに開いているエクスプローラーの窓は自動で更新されません。窓を開き直すか、F5キーで更新してください。"
                .to_owned(),
        );
    }
    if metadata.method_class == MethodClass::DocumentedRegistry
        && metadata.kind == ActionKind::Persistent
    {
        detail_points
            .push("対象key自体が存在しない場合は、新しく作らず変更を停止します。".to_owned());
        if !metadata.auto_apply_eligible {
            detail_points.push(
                "自動検証の対象は固定HKCU値です。Windows UIへの反映は別途実機確認が必要です。"
                    .to_owned(),
            );
        }
    }
    if metadata.id == ActionId::InputShiftInterruptionGuard {
        detail_points.extend([
            "変えるのは、Shiftの連打・長押しから確認画面を開く動作だけです。".to_owned(),
            "入力を補助する機能そのものと、キーの待ち時間などは変更しません。".to_owned(),
            "入力の補助機能を現在使用中の場合は、安全のため適用しません。".to_owned(),
        ]);
    }
    if metadata.id == ActionId::AudioCommsMicMute {
        detail_points.extend([
            "対象は、その時点でWindowsが既定の通話用入力としている1台だけです。".to_owned(),
            "別の入力デバイスを指定したアプリや、排他モードの音声には効かないことがあります。無音は保証しません。".to_owned(),
            "復元は保存したdevice IDだけを対象にし、既定端末が変わっても新しい端末は触りません。".to_owned(),
        ]);
    }
    ActionPresentation {
        id: metadata.id.as_str().to_owned(),
        action_version: metadata.action_version,
        name: if metadata.kind == ActionKind::Guided
            && metadata.method_class == MethodClass::UnverifiedStorage
        {
            format!("設計候補: {}", metadata.name)
        } else {
            metadata.name.to_owned()
        },
        description: metadata.description.to_owned(),
        audience: audience_for(metadata.id).to_owned(),
        category: category_for(metadata.id).to_owned(),
        tags: metadata
            .tags
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        supported_windows_versions: metadata
            .supportedWindowsVersions
            .iter()
            .map(|release| format!("{:?}", release).replace("Windows11_", "Windows 11 "))
            .collect(),
        minimum_build: metadata.minimumBuild,
        maximum_tested_build: (metadata.maximumTestedBuild != 0)
            .then_some(metadata.maximumTestedBuild),
        risk_level: risk_name(metadata.riskLevel).to_owned(),
        requires_admin: metadata.requiresAdmin,
        requires_restart: metadata.requiresRestart,
        requires_explorer_restart: metadata.requiresExplorerRestart,
        update_impact: update_impact(metadata.windows_update_impact).to_owned(),
        reversible: matches!(metadata.kind, ActionKind::Persistent | ActionKind::Session),
        kind: kind.to_owned(),
        auto_apply_eligible: metadata.auto_apply_eligible,
        availability: availability.to_owned(),
        method_class: method_key(metadata.method_class).to_owned(),
        method_summary: method_summary_for(metadata.id, metadata.method_class).to_owned(),
        desired_state: desired_state_for(metadata).to_owned(),
        current_state: current_state.map(|state| state_to_ui(metadata, state)),
        detail_points,
        settings_page: crate::settings_link::settings_page_for(metadata.id)
            .map(|page| page.to_owned()),
    }
}

pub fn state_to_ui(metadata: &ActionMetadata, state: DetectedState) -> UiActionState {
    let action_id = metadata.id;
    if metadata.method_class == MethodClass::UnverifiedStorage {
        if let DetectedState::Known {
            value: ObservedValue::RegistryDword { configured },
            evidence,
        } = &state
        {
            return UiActionState {
                kind: "known".to_owned(),
                label: (*configured)
                    .map(|value| format!("HKCU保存値: DWORD {value}"))
                    .unwrap_or_else(|| "HKCU保存値: 未設定".to_owned()),
                detail: "固定位置の保存値だけを読み取りました。Windows UIの有効状態を示すものではありません。".to_owned(),
                items: Vec::new(),
                observed_at: Some(format_timestamp(evidence.observed_at_unix_ms)),
                integration: None,
            };
        }
    }
    match state {
        DetectedState::Known { value, evidence } => UiActionState {
            kind: "known".to_owned(),
            label: observed_label(action_id, &value),
            detail: observed_detail(&value),
            items: observed_items(&value),
            observed_at: Some(format_timestamp(evidence.observed_at_unix_ms)),
            integration: integration_state(&value),
        },
        DetectedState::NeedsRestart { value, evidence } => UiActionState {
            kind: "known".to_owned(),
            label: observed_label(action_id, &value),
            detail: "設定値は確認できました。反映にはアプリ側の再読込が必要です。".to_owned(),
            items: observed_items(&value),
            observed_at: Some(format_timestamp(evidence.observed_at_unix_ms)),
            integration: integration_state(&value),
        },
        DetectedState::Unknown { reason } => UiActionState {
            kind: "unknown".to_owned(),
            label: "確認できません".to_owned(),
            detail: bounded(reason),
            items: Vec::new(),
            observed_at: None,
            integration: None,
        },
        DetectedState::Unsupported { reason } => UiActionState {
            kind: "unsupported".to_owned(),
            label: "この環境では利用できません".to_owned(),
            detail: bounded(reason),
            items: Vec::new(),
            observed_at: None,
            integration: None,
        },
        DetectedState::PolicyManaged { authority } => UiActionState {
            kind: "policy_managed".to_owned(),
            label: "組織のポリシーで管理されています".to_owned(),
            detail: authority
                .map(bounded)
                .unwrap_or_else(|| "PCカスタムからは上書きしません。".to_owned()),
            items: Vec::new(),
            observed_at: None,
            integration: None,
        },
        DetectedState::Conflict { .. } => UiActionState {
            kind: "unknown".to_owned(),
            label: "別の変更を検出しました".to_owned(),
            detail: "保存した適用値と現在値が異なるため、自動操作を止めています。".to_owned(),
            items: Vec::new(),
            observed_at: None,
            integration: None,
        },
        DetectedState::Error { code, reason } => UiActionState {
            kind: "error".to_owned(),
            label: "状態確認に失敗しました".to_owned(),
            detail: format!("{} ({})", bounded(reason), bounded(code)),
            items: Vec::new(),
            observed_at: None,
            integration: None,
        },
    }
}

fn integration_state(value: &ObservedValue) -> Option<UiIntegrationState> {
    match value {
        ObservedValue::PowerToysInstallation(observed) => Some(UiIntegrationState {
            installed: observed.installed,
            version: observed.version.clone(),
            launch_available: observed.launch_available,
        }),
        _ => None,
    }
}

pub fn preview_change(
    metadata: &ActionMetadata,
    before: &DetectedState,
    explanation: &ChangeExplanation,
) -> PreviewChange {
    PreviewChange {
        action_id: metadata.id.as_str().to_owned(),
        title: metadata.name.to_owned(),
        before: state_to_ui(metadata, before.clone()).label,
        after: desired_state_for(metadata).to_owned(),
        method: explanation.method.clone(),
        resource_label: explanation.resources.join(" / "),
        risk_level: risk_name(metadata.riskLevel).to_owned(),
        reversible: matches!(metadata.kind, ActionKind::Persistent | ActionKind::Session),
    }
}

pub fn parse_action_request(request: PreviewActionRequest) -> CoreResult<ActionParameters> {
    let action_id = ActionId::from_str(&request.action_id)
        .map_err(|_| CoreError::invalid_request("登録されていないAction IDは実行できません。"))?;
    let mut parameters = request.parameters;
    if let Some(value) = parameters.remove("keepDisplayOn") {
        if parameters
            .insert("keep_display_on".to_owned(), value)
            .is_some()
        {
            return Err(CoreError::invalid_request(
                "同じパラメーターを重複して指定できません。",
            ));
        }
    }
    let tagged = serde_json::json!({
        "action_id": action_id.as_str(),
        "parameters": Value::Object(parameters),
    });
    serde_json::from_value(tagged)
        .map_err(|_| CoreError::invalid_request("Actionの指定内容がschemaに一致しません。"))
}

pub fn default_parameters(action_id: ActionId) -> Option<ActionParameters> {
    Some(match action_id {
        ActionId::SessionPreventSleep => ActionParameters::SessionPreventSleep {
            keep_display_on: false,
        },
        ActionId::InputShiftInterruptionGuard => ActionParameters::InputShiftInterruptionGuard {},
        ActionId::PowerActiveSchemeCheck => ActionParameters::PowerActiveSchemeCheck {},
        ActionId::PowerActiveSchemeSwitch => ActionParameters::PowerActiveSchemeSwitch {
            scheme: PowerScheme::Balanced,
        },
        ActionId::PowerModeSwitch => ActionParameters::PowerModeSwitch {
            mode: PowerModeChoice::Balanced,
        },
        ActionId::InputPointerFeel => ActionParameters::InputPointerFeel {
            acceleration: false,
        },
        ActionId::AudioCommsMicMute => ActionParameters::AudioCommsMicMute {},
        ActionId::ExplorerShowExtensions => ActionParameters::ExplorerShowExtensions { show: true },
        ActionId::ExplorerShowHidden => ActionParameters::ExplorerShowHidden { show: true },
        ActionId::ExplorerClockSeconds => ActionParameters::ExplorerClockSeconds { show: true },
        ActionId::AppearanceTransparency => {
            ActionParameters::AppearanceTransparency { enabled: true }
        }
        ActionId::TaskbarTaskView => ActionParameters::TaskbarTaskView { show: true },
        ActionId::TaskbarWidgets => ActionParameters::TaskbarWidgets { show: true },
        ActionId::ExplorerItemCheckboxes => ActionParameters::ExplorerItemCheckboxes { show: true },
        ActionId::ExplorerCompactView => ActionParameters::ExplorerCompactView { enabled: true },
        ActionId::ThemeColorMode => ActionParameters::ThemeColorMode {
            mode: ThemeColorMode::Dark,
        },
        ActionId::GamesProcessWatch => return None,
        ActionId::GamesReadinessCheck => ActionParameters::GamesReadinessCheck {},
        ActionId::TaskbarSearchMode => ActionParameters::TaskbarSearchMode {
            mode: TaskbarSearchMode::SearchBox,
        },
        ActionId::TaskbarAlignment => ActionParameters::TaskbarAlignment {
            alignment: TaskbarAlignment::Left,
        },
        ActionId::StartLayout => ActionParameters::StartLayout {
            layout: StartLayout::MorePins,
        },
        ActionId::StartRecommendations => ActionParameters::StartRecommendations { enabled: false },
        ActionId::ExplorerLaunchTarget => ActionParameters::ExplorerLaunchTarget {
            target: ExplorerLaunchTarget::ThisPc,
        },
        ActionId::ExplorerRecentFiles => ActionParameters::ExplorerRecentFiles { show: true },
        ActionId::TaskbarButtonGrouping => ActionParameters::TaskbarButtonGrouping {
            mode: TaskbarGroupingMode::WhenFull,
        },
        ActionId::TaskbarFlashing => ActionParameters::TaskbarFlashing { enabled: true },
        ActionId::TaskbarShareWindow => ActionParameters::TaskbarShareWindow { enabled: true },
        ActionId::TaskbarShowDesktop => ActionParameters::TaskbarShowDesktop { enabled: true },
        ActionId::SearchRecentOnHover => ActionParameters::SearchRecentOnHover { enabled: false },
        ActionId::TaskbarMultiMonitor => ActionParameters::TaskbarMultiMonitor { enabled: true },
        ActionId::TaskbarMultiMonitorMode => ActionParameters::TaskbarMultiMonitorMode {
            mode: TaskbarMultiMonitorMode::WindowMonitor,
        },
        ActionId::TaskbarSecondaryButtonGrouping => {
            ActionParameters::TaskbarSecondaryButtonGrouping {
                mode: TaskbarGroupingMode::WhenFull,
            }
        }
        ActionId::StartShowAllPins => ActionParameters::StartShowAllPins { enabled: true },
        ActionId::StartRecentApps => ActionParameters::StartRecentApps { show: true },
        ActionId::AppearanceAccentStartTaskbar => {
            ActionParameters::AppearanceAccentStartTaskbar { enabled: true }
        }
        ActionId::AppearanceAccentTitleBars => {
            ActionParameters::AppearanceAccentTitleBars { enabled: true }
        }
        ActionId::AppearanceAutoAccent => ActionParameters::AppearanceAutoAccent { enabled: true },
        ActionId::GamesGameMode => ActionParameters::GamesGameMode { enabled: true },
        ActionId::GamesControllerGameBar => {
            ActionParameters::GamesControllerGameBar { enabled: false }
        }
        ActionId::DevicesAutoplay => ActionParameters::DevicesAutoplay { enabled: true },
        ActionId::NotificationsUsbErrors => {
            ActionParameters::NotificationsUsbErrors { enabled: true }
        }
        ActionId::NotificationsWeakCharger => {
            ActionParameters::NotificationsWeakCharger { enabled: true }
        }
        ActionId::InputAutocorrect => ActionParameters::InputAutocorrect { enabled: true },
        ActionId::InputDoubleSpacePeriod => {
            ActionParameters::InputDoubleSpacePeriod { enabled: true }
        }
        ActionId::InputAutoShift => ActionParameters::InputAutoShift { enabled: true },
        ActionId::InputVoiceTypingKey => ActionParameters::InputVoiceTypingKey { enabled: true },
        ActionId::InputMultilingualSuggestions => {
            ActionParameters::InputMultilingualSuggestions { enabled: true }
        }
        ActionId::ExplorerStatusBar => ActionParameters::ExplorerStatusBar { show: true },
        ActionId::ExplorerInfoTips => ActionParameters::ExplorerInfoTips { show: true },
        ActionId::ExplorerHideEmptyDrives => {
            ActionParameters::ExplorerHideEmptyDrives { hide: true }
        }
        ActionId::ExplorerNavExpandCurrent => {
            ActionParameters::ExplorerNavExpandCurrent { enabled: true }
        }
        ActionId::ExplorerNavShowAll => ActionParameters::ExplorerNavShowAll { enabled: true },
        ActionId::ExplorerSeparateProcess => {
            ActionParameters::ExplorerSeparateProcess { enabled: true }
        }
        ActionId::ExplorerIconsOnly => ActionParameters::ExplorerIconsOnly { enabled: false },
        ActionId::ExplorerDriveLetters => ActionParameters::ExplorerDriveLetters { show: true },
        ActionId::ExplorerPreviewHandlers => {
            ActionParameters::ExplorerPreviewHandlers { enabled: true }
        }
        ActionId::ExplorerSharingWizard => {
            ActionParameters::ExplorerSharingWizard { enabled: true }
        }
        ActionId::ExplorerAlwaysShowMenus => {
            ActionParameters::ExplorerAlwaysShowMenus { enabled: true }
        }
        ActionId::AppearanceTaskbarAnimations => {
            ActionParameters::AppearanceTaskbarAnimations { enabled: true }
        }
        ActionId::NotificationsToastBanners => {
            ActionParameters::NotificationsToastBanners { enabled: true }
        }
        ActionId::SetupStartupInventory => ActionParameters::SetupStartupInventory {},
        ActionId::StorageFreeSpaceCheck => ActionParameters::StorageFreeSpaceCheck {},
        ActionId::StorageTempFilesCheck => ActionParameters::StorageTempFilesCheck {},
        ActionId::AppearanceAccentColorCheck => ActionParameters::AppearanceAccentColorCheck {},
        ActionId::AppearanceWindowColor => ActionParameters::AppearanceWindowColor {
            color: crate::action::WindowColorPreset::WindowsBlue,
        },
        ActionId::SetupPowerToysStatus => ActionParameters::SetupPowerToysStatus {},
        ActionId::SetupLaunchApps => ActionParameters::SetupLaunchApps {
            bundle: AppLaunchBundle::Study,
        },
        ActionId::SetupWindowsUpdateStatus => ActionParameters::SetupWindowsUpdateStatus {},
        ActionId::SetupDefaultApps => ActionParameters::SetupDefaultApps {},
        ActionId::SetupWindowLayout => return None,
        ActionId::SetupAudioOutput => ActionParameters::SetupAudioOutput {},
    })
}

/// Catalog snapshots stay fast and side-effect free: filesystem walks and the
/// multi-source readiness probe run only after the user explicitly requests a
/// fresh observation.
pub fn listing_parameters(action_id: ActionId) -> Option<ActionParameters> {
    if matches!(
        action_id,
        ActionId::SetupStartupInventory
            | ActionId::StorageFreeSpaceCheck
            | ActionId::StorageTempFilesCheck
            | ActionId::AppearanceAccentColorCheck
            | ActionId::GamesReadinessCheck
            | ActionId::SetupWindowsUpdateStatus
            | ActionId::SetupAudioOutput
            | ActionId::SetupWindowLayout
    ) {
        None
    } else {
        default_parameters(action_id)
    }
}

pub fn os_label(identity: &OsIdentity) -> String {
    match identity.base_build {
        26_100 => "Windows 11 24H2".to_owned(),
        26_200 => "Windows 11 25H2".to_owned(),
        28_000 => "Windows 11 26H1".to_owned(),
        build => format!("Windows build {build}"),
    }
}

pub fn risk_name(risk: ActionRiskLevel) -> &'static str {
    match risk {
        ActionRiskLevel::Safe => "safe",
        ActionRiskLevel::Caution => "caution",
        ActionRiskLevel::Experimental => "experimental",
    }
}

fn category_for(action_id: ActionId) -> &'static str {
    match action_id {
        ActionId::SessionPreventSleep => "session",
        ActionId::PowerActiveSchemeCheck
        | ActionId::PowerActiveSchemeSwitch
        | ActionId::PowerModeSwitch => "power",
        ActionId::InputPointerFeel => "games",
        ActionId::ExplorerShowExtensions
        | ActionId::ExplorerShowHidden
        | ActionId::ExplorerClockSeconds
        | ActionId::ExplorerItemCheckboxes
        | ActionId::ExplorerCompactView => "explorer",
        ActionId::ThemeColorMode
        | ActionId::AppearanceTransparency
        | ActionId::TaskbarTaskView
        | ActionId::TaskbarWidgets
        | ActionId::TaskbarSearchMode
        | ActionId::TaskbarAlignment
        | ActionId::StartLayout
        | ActionId::StartRecommendations
        | ActionId::TaskbarButtonGrouping
        | ActionId::TaskbarFlashing
        | ActionId::TaskbarShareWindow
        | ActionId::TaskbarShowDesktop
        | ActionId::SearchRecentOnHover
        | ActionId::TaskbarMultiMonitor
        | ActionId::TaskbarMultiMonitorMode
        | ActionId::TaskbarSecondaryButtonGrouping
        | ActionId::StartShowAllPins
        | ActionId::StartRecentApps
        | ActionId::AppearanceAccentStartTaskbar
        | ActionId::AppearanceAccentTitleBars
        | ActionId::AppearanceAutoAccent
        | ActionId::AppearanceTaskbarAnimations => "appearance",
        ActionId::ExplorerLaunchTarget
        | ActionId::ExplorerRecentFiles
        | ActionId::ExplorerStatusBar
        | ActionId::ExplorerInfoTips
        | ActionId::ExplorerHideEmptyDrives
        | ActionId::ExplorerNavExpandCurrent
        | ActionId::ExplorerNavShowAll
        | ActionId::ExplorerSeparateProcess
        | ActionId::ExplorerIconsOnly
        | ActionId::ExplorerDriveLetters
        | ActionId::ExplorerPreviewHandlers
        | ActionId::ExplorerSharingWizard
        | ActionId::ExplorerAlwaysShowMenus => "explorer",
        ActionId::InputShiftInterruptionGuard
        | ActionId::GamesProcessWatch
        | ActionId::GamesReadinessCheck
        | ActionId::GamesGameMode
        | ActionId::GamesControllerGameBar => "games",
        ActionId::DevicesAutoplay
        | ActionId::SetupStartupInventory
        | ActionId::SetupPowerToysStatus
        | ActionId::SetupLaunchApps
        | ActionId::SetupWindowsUpdateStatus
        | ActionId::SetupDefaultApps
        | ActionId::SetupWindowLayout
        | ActionId::SetupAudioOutput => "setup",
        ActionId::StorageFreeSpaceCheck | ActionId::StorageTempFilesCheck => "storage",
        ActionId::AppearanceAccentColorCheck | ActionId::AppearanceWindowColor => "appearance",
        ActionId::NotificationsUsbErrors
        | ActionId::NotificationsWeakCharger
        | ActionId::NotificationsToastBanners => "notifications",
        ActionId::InputAutocorrect
        | ActionId::InputDoubleSpacePeriod
        | ActionId::InputAutoShift
        | ActionId::InputVoiceTypingKey
        | ActionId::InputMultilingualSuggestions
        | ActionId::AudioCommsMicMute => "input",
    }
}

fn audience_for(action_id: ActionId) -> &'static str {
    match action_id {
        ActionId::SessionPreventSleep => "長い作業やゲーム中に、自動スリープを避けたい人向け",
        ActionId::InputShiftInterruptionGuard => {
            "ゲーム中にShift操作で確認画面へ割り込まれたくない人向け。入力の補助機能をShift操作から開く人には向きません"
        }
        ActionId::PowerActiveSchemeCheck => "現在の電源構成を変更せず確認したい人向け",
        ActionId::PowerActiveSchemeSwitch => {
            "Windows公開Power APIで電源プランを明示的に選びたい人向け"
        }
        ActionId::PowerModeSwitch => {
            "電源接続時と電池使用時のモードを、用途に合わせて選びたい人向け。選べない値があるPCもあります"
        }
        ActionId::InputPointerFeel => {
            "ゲームと普段の作業でマウスの動き方を変えたい人向け。ゲーム側が独自にマウスを読んでいる場合は届きません"
        }
        ActionId::AudioCommsMicMute => {
            "会議前にWindowsの既定の通話用入力1台をミュート設定にしたい人向け。別の入力端末を使うアプリには届きません"
        }
        ActionId::ExplorerShowExtensions => "ファイルの種類を見分け、誤操作を減らしたい人向け",
        ActionId::ExplorerShowHidden => "隠しファイルを扱う必要がある人向け",
        ActionId::ExplorerClockSeconds => "タスクバーの時計で秒まで確認したい人向け",
        ActionId::AppearanceTransparency => "透明効果のオン・オフを切り替えたい人向け",
        ActionId::TaskbarTaskView => "タスクビューボタンの表示を切り替えたい人向け",
        ActionId::TaskbarWidgets => "ウィジェットボタンの表示を切り替えたい人向け",
        ActionId::ExplorerItemCheckboxes => "チェックボックスでの複数選択を切り替えたい人向け",
        ActionId::ExplorerCompactView => "一覧の行間（コンパクト表示）を切り替えたい人向け",
        ActionId::ThemeColorMode => "Windowsとアプリの明暗を揃えたい人向け",
        ActionId::GamesProcessWatch => "登録したゲームの起動と終了だけを安全に検知したい人向け",
        ActionId::GamesReadinessCheck => {
            "ゲーム前の表示・電源・容量・音声と設定値の目安を変更せず確認したい人向け"
        }
        ActionId::TaskbarSearchMode => "タスクバーの検索表示を自分の使い方に合わせたい人向け",
        ActionId::TaskbarAlignment => "タスクバーの配置を左または中央から選びたい人向け",
        ActionId::StartLayout => "スタートのピンとおすすめの比率を調整したい人向け",
        ActionId::StartRecommendations => "スタートのおすすめ表示を減らしたい人向け",
        ActionId::ExplorerLaunchTarget => "Explorerを開いた直後の場所を選びたい人向け",
        ActionId::ExplorerRecentFiles => "Explorerの最近使ったファイル表示を管理したい人向け",
        ActionId::TaskbarButtonGrouping | ActionId::TaskbarSecondaryButtonGrouping => {
            "タスクバーボタンの結合方法を選びたい人向け"
        }
        ActionId::TaskbarFlashing => "タスクバーアプリの点滅表示を調整したい人向け",
        ActionId::TaskbarShareWindow => "対応アプリのウィンドウ共有導線を管理したい人向け",
        ActionId::TaskbarShowDesktop => "タスクバー右端のデスクトップ表示操作を管理したい人向け",
        ActionId::SearchRecentOnHover => "検索アイコンに触れたときの動作を選びたい人向け",
        ActionId::TaskbarMultiMonitor | ActionId::TaskbarMultiMonitorMode => {
            "複数モニターのタスクバー表示を変えたい人向け"
        }
        ActionId::StartShowAllPins => "スタートですべてのピンを先に見たい人向け",
        ActionId::StartRecentApps => "スタートの最近追加したアプリ表示を管理したい人向け",
        ActionId::AppearanceAccentStartTaskbar
        | ActionId::AppearanceAccentTitleBars
        | ActionId::AppearanceAutoAccent => "Windowsのアクセント色の使われ方を変えたい人向け",
        ActionId::GamesGameMode => "Windows標準のGame Modeを明示的に管理したい人向け",
        ActionId::GamesControllerGameBar => {
            "コントローラーからGame Barを開く操作を管理したい人向け"
        }
        ActionId::DevicesAutoplay => "メディアやデバイスの自動再生を管理したい人向け",
        ActionId::NotificationsUsbErrors | ActionId::NotificationsWeakCharger => {
            "USB接続や充電器の問題をWindows通知で確認したい人向け"
        }
        ActionId::InputAutocorrect
        | ActionId::InputDoubleSpacePeriod
        | ActionId::InputAutoShift
        | ActionId::InputVoiceTypingKey
        | ActionId::InputMultilingualSuggestions => "タッチキーボードや入力候補を変えたい人向け",
        ActionId::ExplorerStatusBar
        | ActionId::ExplorerInfoTips
        | ActionId::ExplorerHideEmptyDrives
        | ActionId::ExplorerNavExpandCurrent
        | ActionId::ExplorerNavShowAll
        | ActionId::ExplorerSeparateProcess
        | ActionId::ExplorerIconsOnly
        | ActionId::ExplorerDriveLetters
        | ActionId::ExplorerPreviewHandlers
        | ActionId::ExplorerSharingWizard
        | ActionId::ExplorerAlwaysShowMenus => "Explorerの表示や操作を細かく変えたい人向け",
        ActionId::AppearanceTaskbarAnimations => "タスクバーの視覚アニメーションを選びたい人向け",
        ActionId::NotificationsToastBanners => "Windowsの通知バナー表示を管理したい人向け",
        ActionId::SetupStartupInventory => {
            "固定RunキーとStartupフォルダーの登録項目を変更せず把握したい人向け"
        }
        ActionId::StorageFreeSpaceCheck => "システムドライブの空き容量を変更せず確認したい人向け",
        ActionId::StorageTempFilesCheck => "削除前にユーザー一時ファイルの規模だけ確認したい人向け",
        ActionId::AppearanceAccentColorCheck => {
            "いまWindowsが使っている色を、変更せずに確かめたい人向け"
        }
        ActionId::AppearanceWindowColor => {
            "タイトルバーなどの色を、決められた色から選んで変えたい人向け"
        }
        ActionId::SetupPowerToysStatus => "日常のWindows操作をPowerToysで便利にしたい人向け",
        ActionId::SetupLaunchApps => "勉強や作業を始めるアプリを一度に開きたい人向け",
        ActionId::SetupWindowsUpdateStatus => {
            "更新確認日時と再起動保留だけを変更せず確認したい人向け"
        }
        ActionId::SetupDefaultApps => {
            "ファイルやリンクを開く既定アプリをWindows自身の画面で選びたい人向け"
        }
        ActionId::SetupWindowLayout => {
            "いま開いている通常のアプリ窓を、同じ位置と表示状態へ戻したい人向け"
        }
        ActionId::SetupAudioOutput => {
            "現在の音声出力先を確認し、Windows自身の画面で切り替えたい人向け"
        }
    }
}

fn desired_state(action_id: ActionId) -> &'static str {
    match action_id {
        ActionId::SessionPreventSleep => "自動スリープを一時的に防ぐ",
        ActionId::InputShiftInterruptionGuard => {
            "Shift操作による確認画面を、このモードの間だけ出さない"
        }
        ActionId::PowerActiveSchemeCheck => "変更せず、現在の電源設定を確認",
        ActionId::PowerActiveSchemeSwitch => "選択したWindows標準の電源プラン",
        ActionId::PowerModeSwitch => "選択した電源モード（電源接続時と電池使用時の両方）",
        ActionId::InputPointerFeel => "選択したポインターの動き方",
        ActionId::AudioCommsMicMute => "既定の通話用入力デバイス1台をミュート設定にする",
        ActionId::ExplorerShowExtensions => "拡張子を表示",
        ActionId::ExplorerShowHidden => "隠しファイルを表示",
        ActionId::ExplorerClockSeconds => "タスクバーの時計に秒を表示",
        ActionId::AppearanceTransparency => "選択した透明効果の状態",
        ActionId::TaskbarTaskView => "選択したタスクビューボタンの表示",
        ActionId::TaskbarWidgets => "選択したウィジェットボタンの表示",
        ActionId::ExplorerItemCheckboxes => "選択した項目チェックボックスの表示",
        ActionId::ExplorerCompactView => "選択したコンパクト表示の状態",
        ActionId::ThemeColorMode => "選択したライト／ダーク表示",
        ActionId::GamesProcessWatch => "同じ実行ファイルだと確かめられたものだけ見守る",
        ActionId::GamesReadinessCheck => "変更せず、ゲーム前の参考情報を個別に確認",
        ActionId::TaskbarSearchMode => "選択したタスクバー検索の表示方法",
        ActionId::TaskbarAlignment => "選択したタスクバーの配置",
        ActionId::StartLayout => "選択したスタートのピンとおすすめの比率",
        ActionId::StartRecommendations => "選択したスタートのおすすめ表示",
        ActionId::ExplorerLaunchTarget => "選択したExplorerの開始場所",
        ActionId::ExplorerRecentFiles => "選択した最近使ったファイル表示",
        ActionId::TaskbarButtonGrouping | ActionId::TaskbarSecondaryButtonGrouping => {
            "選択したタスクバーボタンの結合方法"
        }
        ActionId::TaskbarFlashing => "選択したタスクバーアプリの点滅表示",
        ActionId::TaskbarShareWindow => "選択したタスクバーの共有導線表示",
        ActionId::TaskbarShowDesktop => "選択したデスクトップ表示操作",
        ActionId::SearchRecentOnHover => "選択した検索アイコンのホバー動作",
        ActionId::TaskbarMultiMonitor => "選択した複数モニターのタスクバー表示",
        ActionId::TaskbarMultiMonitorMode => "選択した複数モニターのアプリボタン表示先",
        ActionId::StartShowAllPins => "選択したすべてのピンの初期表示",
        ActionId::StartRecentApps => "選択した最近追加したアプリ表示",
        ActionId::AppearanceAccentStartTaskbar => "選択したスタートとタスクバーのアクセント表示",
        ActionId::AppearanceAccentTitleBars => "選択したタイトルバーと枠のアクセント表示",
        ActionId::AppearanceAutoAccent => "選択した背景からのアクセント自動選択",
        ActionId::GamesGameMode => "選択したWindows Game Modeの状態",
        ActionId::GamesControllerGameBar => "選択したコントローラーのGame Bar操作",
        ActionId::DevicesAutoplay => "選択したメディアとデバイスの自動再生",
        ActionId::NotificationsUsbErrors => "選択したUSBエラー通知",
        ActionId::NotificationsWeakCharger => "選択した低出力充電器の通知",
        ActionId::InputAutocorrect => "選択したタッチキーボードの自動修正",
        ActionId::InputDoubleSpacePeriod => "選択したスペース2回のピリオド入力",
        ActionId::InputAutoShift => "選択したタッチキーボードの自動Shift",
        ActionId::InputVoiceTypingKey => "選択した音声入力キー表示",
        ActionId::InputMultilingualSuggestions => "選択した多言語入力候補",
        ActionId::ExplorerStatusBar => "選択したExplorerのステータスバー表示",
        ActionId::ExplorerInfoTips => "選択したExplorerのファイル説明表示",
        ActionId::ExplorerHideEmptyDrives => "選択した空のリムーバブルドライブ表示",
        ActionId::ExplorerNavExpandCurrent => "選択した現在位置までのナビゲーション展開",
        ActionId::ExplorerNavShowAll => "選択したナビゲーションの全フォルダー表示",
        ActionId::ExplorerSeparateProcess => "選択したフォルダーウィンドウのプロセス分離",
        ActionId::ExplorerIconsOnly => "選択したアイコンのみの表示",
        ActionId::ExplorerDriveLetters => "選択したドライブ文字表示",
        ActionId::ExplorerPreviewHandlers => "選択した登録済みプレビュー表示",
        ActionId::ExplorerSharingWizard => "選択したExplorerの共有ウィザード利用",
        ActionId::ExplorerAlwaysShowMenus => "選択したExplorerのメニュー常時表示",
        ActionId::AppearanceTaskbarAnimations => "選択したタスクバーのアニメーション",
        ActionId::NotificationsToastBanners => "選択したWindows通知バナー表示",
        ActionId::SetupStartupInventory => "変更せず、固定された起動元の項目を一覧化",
        ActionId::StorageFreeSpaceCheck => "変更せず、システムドライブ容量を確認",
        ActionId::StorageTempFilesCheck => "削除せず、ユーザー一時ファイルを上限付き集計",
        ActionId::AppearanceAccentColorCheck => "公開APIで現在の配色を読み取り（変更なし）",
        ActionId::AppearanceWindowColor => "HKCU DWMの色2値を1トランザクションで変更",
        ActionId::SetupPowerToysStatus => "変更せず、PowerToysの導入状況とバージョンを確認",
        ActionId::SetupLaunchApps => "固定allowlistのアプリを、起動中でなければ直接開く",
        ActionId::SetupWindowsUpdateStatus => "Windows Update Agentの状態を変更せず確認",
        ActionId::SetupDefaultApps => "Windowsの既定のアプリ画面で利用者が選択",
        ActionId::SetupWindowLayout => "明示保存した、現在開いている対象ウィンドウの位置と表示状態",
        ActionId::SetupAudioOutput => "公開APIで出力先を確認し、Windowsのサウンド画面で選択",
    }
}

fn desired_state_for(metadata: &ActionMetadata) -> &'static str {
    if metadata.kind == ActionKind::Guided
        && metadata.method_class == MethodClass::UnverifiedStorage
    {
        "setter根拠の承認待ち（変更は実行しません）"
    } else {
        desired_state(metadata.id)
    }
}

fn method_name(method: MethodClass) -> &'static str {
    match method {
        MethodClass::PublicApi => "Windows 公開API",
        MethodClass::MicrosoftCli => "Microsoft 公式CLI",
        MethodClass::WinGet => "WinGet",
        MethodClass::OfficialModule => "Microsoft 公式モジュール",
        MethodClass::DocumentedRegistry => "限定されたHKCU設定",
        MethodClass::LimitedExternal => "検証済みの限定連携",
        MethodClass::UnverifiedStorage => "未立証の固定HKCU保存値（読み取りのみ）",
    }
}

fn method_key(method: MethodClass) -> &'static str {
    match method {
        MethodClass::PublicApi => "public_api",
        MethodClass::MicrosoftCli => "microsoft_cli",
        MethodClass::WinGet => "winget",
        MethodClass::OfficialModule => "official_module",
        MethodClass::DocumentedRegistry => "documented_registry",
        MethodClass::LimitedExternal => "limited_external",
        MethodClass::UnverifiedStorage => "unverified_storage",
    }
}

fn method_summary_for(action_id: ActionId, method: MethodClass) -> &'static str {
    match action_id {
        ActionId::PowerActiveSchemeSwitch => {
            "PowerGetActiveSchemeとPowerSetActiveSchemeによる明示切替"
        }
        ActionId::PowerModeSwitch => {
            "Windowsが公開している電源モードの読み取りと設定。要求値と実効値は別に表示"
        }
        ActionId::InputPointerFeel => {
            "Windowsが公開しているポインター設定の読み取りと設定。速度としきい値も含めて保存"
        }
        ActionId::InputShiftInterruptionGuard => {
            "Windowsが公開している入力設定の機能で、確認画面を開く動作だけを一時停止"
        }
        ActionId::AudioCommsMicMute => {
            "Core AudioでeCommunicationsの既定入力1台を特定し、EndpointVolumeのmute設定だけを変更"
        }
        ActionId::SetupStartupInventory => {
            "固定HKCU/HKLM RunキーとKnown Startup Folderの上限付き読み取り"
        }
        ActionId::StorageFreeSpaceCheck => {
            "GetWindowsDirectoryWとGetDiskFreeSpaceExWによる読み取り"
        }
        ActionId::StorageTempFilesCheck => "GetTempPath2Wとreparse非追跡・上限付きmetadata走査",
        ActionId::AppearanceAccentColorCheck => "公開APIのDwmGetColorizationColorによる読み取り",
        ActionId::GamesReadinessCheck => {
            "Windows公開APIと登録済み固定HKCU設定値による7項目の読み取り"
        }
        ActionId::SetupPowerToysStatus => {
            "文書化されたApp Paths登録とアンインストール登録の読み取り"
        }
        ActionId::SetupLaunchApps => {
            "App Pathsと固定System32候補を検証し、Command::spawnで直接起動"
        }
        ActionId::SetupWindowsUpdateStatus => "Windows Update Agent公開COMプロパティの読み取り",
        ActionId::SetupDefaultApps => "固定ms-settings URIでWindowsの既定のアプリ画面を開く",
        ActionId::SetupWindowLayout => {
            "EnumWindows・GetWindowPlacement・SetWindowPlacementによる明示保存と復元"
        }
        ActionId::SetupAudioOutput => {
            "Windows公開の読み取り機能と、決め打ちURIによる設定画面の案内"
        }
        _ => method_name(method),
    }
}

fn update_impact(value: &str) -> &'static str {
    match value {
        "low" | "低" => "low",
        "high" | "高" => "high",
        _ => "review",
    }
}

fn observed_label(action_id: ActionId, value: &ObservedValue) -> String {
    match value {
        ObservedValue::PointerFeel {
            acceleration_enabled,
            speed,
        } => {
            let acceleration = if *acceleration_enabled {
                "加速あり"
            } else {
                "加速なし"
            };
            format!("{acceleration}、速さ{speed}")
        }
        ObservedValue::CommunicationsMicrophone { muted } => {
            if *muted {
                "既定の通話マイクはミュート設定".to_owned()
            } else {
                "既定の通話マイクはミュート解除設定".to_owned()
            }
        }
        ObservedValue::PowerMode {
            requested_ac,
            requested_dc,
            effective,
        } => {
            // 要求値と実効値を1行に混ぜない。読んだ人が別物だと分かる形で並べる。
            let ac = requested_ac.as_deref().unwrap_or("確認できません");
            let dc = requested_dc.as_deref().unwrap_or("確認できません");
            match effective {
                Some(now) => {
                    format!("電源接続時は{ac}、電池使用時は{dc}（いま効いているのは{now}）")
                }
                None => format!("電源接続時は{ac}、電池使用時は{dc}"),
            }
        }
        ObservedValue::RegistryDword { configured } => registry_dword_label(action_id, *configured),
        ObservedValue::Theme(theme) => match theme {
            ThemeObservation::Light => "ライト表示".to_owned(),
            ThemeObservation::Dark => "ダーク表示".to_owned(),
            ThemeObservation::Mixed => "アプリとシステムで明暗が混在".to_owned(),
            ThemeObservation::Unconfigured => "値は未設定".to_owned(),
        },
        ObservedValue::SleepLease {
            owned,
            owner_count,
            keep_display_on,
        } => {
            if *owned {
                format!(
                    "スリープ防止中（{}件、画面点灯{}）",
                    owner_count,
                    if *keep_display_on { "あり" } else { "なし" }
                )
            } else {
                "PCカスタムのスリープ防止なし".to_owned()
            }
        }
        ObservedValue::ShiftInterruptionGuard {
            shift_five_press_shortcut_enabled,
            shift_five_press_confirmation_enabled,
            right_shift_hold_shortcut_enabled,
            right_shift_hold_confirmation_enabled,
            input_assistance_in_use,
        } => {
            if *input_assistance_in_use {
                "入力の補助機能を使用中（変更しません）".to_owned()
            } else {
                format!(
                    "Shift 5回: {}／右Shift長押し: {}",
                    shift_trigger_state(
                        *shift_five_press_shortcut_enabled,
                        *shift_five_press_confirmation_enabled,
                    ),
                    shift_trigger_state(
                        *right_shift_hold_shortcut_enabled,
                        *right_shift_hold_confirmation_enabled,
                    ),
                )
            }
        }
        ObservedValue::ActivePowerScheme { guid } => {
            format!("有効な電源プラン: {}", power_scheme_display_name(guid))
        }
        ObservedValue::Processes { matches } => {
            format!("同じ実行ファイルだと確かめられたもの {}件", matches.len())
        }
        ObservedValue::StartupInventory(inventory) => format!(
            "確認対象のスタートアップ {}件{}",
            inventory.entries.len(),
            if inventory.truncated {
                "（上限到達）"
            } else {
                ""
            }
        ),
        ObservedValue::SystemDriveSpace(space) => format!(
            "{} 利用可能 {} / 総容量 {}",
            space.volume,
            format_bytes(space.available_bytes),
            format_bytes(space.total_bytes)
        ),
        ObservedValue::TempFiles(temp) => format!(
            "一時ファイル {}件・{}{}",
            temp.file_count,
            format_bytes(temp.total_bytes),
            if temp.truncated {
                "（部分集計）"
            } else {
                ""
            }
        ),
        ObservedValue::GameReadiness(readiness) => {
            let statuses = [
                component_status(&readiness.refresh_rate),
                component_status(&readiness.advanced_color),
                component_status(&readiness.game_mode),
                component_status(&readiness.active_power_scheme),
                component_status(&readiness.system_drive_space),
                component_status(&readiness.default_render_audio),
                component_status(&readiness.toast_notifications),
            ];
            let known = statuses.iter().filter(|status| **status == 0).count();
            let unknown = statuses.iter().filter(|status| **status == 1).count();
            let unconfigured = statuses.iter().filter(|status| **status == 2).count();
            format!("ゲーム準備: 確認 {known} / 不明 {unknown} / 未設定 {unconfigured}")
        }
        ObservedValue::PowerToysInstallation(observed) => {
            if !observed.installed {
                "PowerToys 未導入".to_owned()
            } else if let Some(version) = &observed.version {
                format!("PowerToys 導入済み（v{version}）")
            } else {
                "PowerToys 導入済み（バージョン不明）".to_owned()
            }
        }
        ObservedValue::KnownApps(value) => {
            let running = value
                .apps
                .iter()
                .filter(|app| app.state == crate::action::KnownAppState::Running)
                .count();
            format!("アプリ {running} / {} 起動中", value.apps.len())
        }
        ObservedValue::WindowsUpdateStatus(value) => {
            let known = usize::from(matches!(
                value.last_checked_local,
                ReadinessComponent::Known { .. }
            )) + usize::from(matches!(
                value.restart_pending,
                ReadinessComponent::Known { .. }
            ));
            format!("Windows Update: {known} / 2項目を確認")
        }
        ObservedValue::AudioOutput(value) => {
            let defaults = value
                .endpoints
                .iter()
                .filter(|endpoint| endpoint.is_default)
                .count();
            format!(
                "音声出力 {}件・既定{}",
                value.endpoints.len(),
                if defaults == 1 { "あり" } else { "なし" }
            )
        }
        ObservedValue::WindowLayout(value) => format!(
            "保存{}件・一致{}件・復元{}件",
            value.saved_window_count, value.matched_window_count, value.positioned_window_count
        ),
        ObservedValue::AccentColor { hex, .. } => format!("アクセントカラー {hex}"),
        ObservedValue::NoOsChange => "OS設定の変更なし".to_owned(),
    }
}

fn component_status<T>(component: &ReadinessComponent<T>) -> u8 {
    match component {
        ReadinessComponent::Known { .. } => 0,
        ReadinessComponent::Unknown { .. } => 1,
        ReadinessComponent::Unconfigured => 2,
    }
}

fn shift_trigger_state(shortcut_enabled: bool, confirmation_enabled: bool) -> &'static str {
    match (shortcut_enabled, confirmation_enabled) {
        (true, true) => "確認画面が出る",
        (true, false) => "確認なしで補助機能が始まる",
        (false, true) => "起動は無効（確認の設定だけ残っています）",
        (false, false) => "起動は無効",
    }
}

fn registry_dword_label(action_id: ActionId, configured: Option<u32>) -> String {
    match (action_id, configured) {
        (ActionId::ExplorerShowExtensions, Some(0)) => "拡張子を表示中".to_owned(),
        (ActionId::ExplorerShowExtensions, Some(1)) => "拡張子を非表示".to_owned(),
        (ActionId::ExplorerShowHidden, Some(1)) => "隠しファイルを表示中".to_owned(),
        (ActionId::ExplorerShowHidden, Some(2)) => "隠しファイルを非表示".to_owned(),
        (ActionId::TaskbarSearchMode, Some(0)) => "検索を非表示".to_owned(),
        (ActionId::TaskbarSearchMode, Some(1)) => "検索アイコンを表示".to_owned(),
        (ActionId::TaskbarSearchMode, Some(2)) => "検索アイコンとラベルを表示".to_owned(),
        (ActionId::TaskbarSearchMode, Some(3)) => "検索ボックスを表示".to_owned(),
        (ActionId::TaskbarAlignment, Some(0)) => "タスクバーを左寄せ".to_owned(),
        (ActionId::TaskbarAlignment, Some(1)) => "タスクバーを中央寄せ".to_owned(),
        (ActionId::StartLayout, Some(0)) => "スタートの比率は既定".to_owned(),
        (ActionId::StartLayout, Some(1)) => "スタートのピンを多く表示".to_owned(),
        (ActionId::StartLayout, Some(2)) => "スタートのおすすめを多く表示".to_owned(),
        (ActionId::ExplorerLaunchTarget, Some(1)) => "ExplorerはPCから開始".to_owned(),
        (ActionId::ExplorerLaunchTarget, Some(2)) => "Explorerはホームから開始".to_owned(),
        (ActionId::ExplorerLaunchTarget, Some(3)) => "Explorerはダウンロードから開始".to_owned(),
        (ActionId::TaskbarButtonGrouping | ActionId::TaskbarSecondaryButtonGrouping, Some(0)) => {
            "タスクバーボタンを常に結合".to_owned()
        }
        (ActionId::TaskbarButtonGrouping | ActionId::TaskbarSecondaryButtonGrouping, Some(1)) => {
            "いっぱいのときにタスクバーボタンを結合".to_owned()
        }
        (ActionId::TaskbarButtonGrouping | ActionId::TaskbarSecondaryButtonGrouping, Some(2)) => {
            "タスクバーボタンを結合しない".to_owned()
        }
        (ActionId::TaskbarMultiMonitorMode, Some(0)) => {
            "すべてのタスクバーにアプリボタンを表示".to_owned()
        }
        (ActionId::TaskbarMultiMonitorMode, Some(1)) => {
            "メインとウィンドウのある画面にアプリボタンを表示".to_owned()
        }
        (ActionId::TaskbarMultiMonitorMode, Some(2)) => {
            "ウィンドウのある画面にアプリボタンを表示".to_owned()
        }
        (ActionId::DevicesAutoplay, Some(0)) => "自動再生は有効".to_owned(),
        (ActionId::DevicesAutoplay, Some(1)) => "自動再生は無効".to_owned(),
        (_, Some(0)) => "無効".to_owned(),
        (_, Some(1)) => "有効".to_owned(),
        (_, Some(value)) => format!("設定値 {value}"),
        (_, None) => "値は未設定".to_owned(),
    }
}

fn observed_detail(value: &ObservedValue) -> String {
    match value {
        ObservedValue::PointerFeel { .. } => {
            "Windowsのポインター設定を読んでいます。ゲームが独自にマウスを読んでいる場合、この設定は届きません。"
                .to_owned()
        }
        ObservedValue::CommunicationsMicrophone { .. } => {
            "Windowsの既定の通話用入力1台のsoftware mute設定を読みました。別の入力端末を使うアプリや排他モードでの無音は保証しません。"
                .to_owned()
        }
        ObservedValue::PowerMode { .. } => {
            "選んだモードと、Windowsがいま効いていると報告するモードは別々に確認しています。             選んだほうは他の設定に上書きされることがあります。"
                .to_owned()
        }
        ObservedValue::RegistryDword { .. } => {
            "HKCUの限定値をraw type/bytesで確認しました。".to_owned()
        }
        ObservedValue::Theme(_) => {
            "アプリとシステムの2値を別々に確認しています。".to_owned()
        }
        ObservedValue::SleepLease { .. } => {
            "Windows全体ではなく、PCカスタムが所有するleaseだけを表示します。".to_owned()
        }
        ObservedValue::ShiftInterruptionGuard {
            input_assistance_in_use,
            ..
        } => {
            if *input_assistance_in_use {
                "入力を補助する機能が現在使われているため、このモードからは変更しません。"
                    .to_owned()
            } else {
                "Shiftの連打と長押しで確認画面を開く動作だけを確認しました。入力を補助する機能そのものは変えません。"
                    .to_owned()
            }
        }
        ObservedValue::ActivePowerScheme { .. } => {
            "公開Power APIで読み取りました。設定は変更していません。".to_owned()
        }
        ObservedValue::Processes { .. } => {
            "PID、作成時刻、正規化パス、file identityを照合しています。".to_owned()
        }
        ObservedValue::StartupInventory(inventory) => startup_inventory_detail(inventory),
        ObservedValue::SystemDriveSpace(space) => format!(
            "公開APIで読み取りました。ボリューム全体の空きは {} です。",
            format_bytes(space.total_free_bytes)
        ),
        ObservedValue::TempFiles(temp) => format!(
            "reparse pointを{}件追跡せず、読めない項目{}件を除外しました。削除はしていません。",
            temp.skipped_reparse_points, temp.unreadable_entries
        ),
        ObservedValue::GameReadiness(readiness) => game_readiness_detail(readiness),
        ObservedValue::PowerToysInstallation(_) => "導入判定はPowerToys.exeのApp Paths登録と実ファイルの解決で行います。バージョンはアンインストール登録から取得できた場合だけ表示します。PowerToysの設定ファイルは読み書きしません。".to_owned(),
        ObservedValue::KnownApps(_) => "固定allowlistの既知アプリについて、App Paths解決可否とプロセス名だけを確認します。".to_owned(),
        ObservedValue::WindowsUpdateStatus(_) => "Windows Update Agentの読み取り専用プロパティです。Updateの停止・変更・検索開始は行いません。".to_owned(),
        ObservedValue::AudioOutput(_) => "Windows公開Core Audio APIでactiveな再生デバイスと現在の既定を読み取りました。切り替えはWindowsのサウンド画面で行います。".to_owned(),
        ObservedValue::WindowLayout(value) => format!(
            "公開Win32 APIで照合しました。復元できなかった対象は{}件です。ゲーム登録済み・判定不能・非標準の窓は操作しません。",
            value.issues.len()
        ),
        ObservedValue::AccentColor { hex, opaque_blend } => format!(
            "Windowsが現在使っている色は {hex} です。透明の混ぜ方: {}。この値は読み取るだけで変更しません。",
            if *opaque_blend { "不透明" } else { "半透明" }
        ),
        ObservedValue::NoOsChange => "読み取り専用Actionです。".to_owned(),
    }
}

/// テストから項目一覧を確認するための入口。UIへ出るのと同じ配列を返す。
#[cfg(test)]
pub fn observed_items_for_test(value: &ObservedValue) -> Vec<String> {
    observed_items(value)
}

fn observed_items(value: &ObservedValue) -> Vec<String> {
    match value {
        ObservedValue::StartupInventory(inventory) => inventory
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{} — {} / {}",
                    entry.name,
                    startup_source_label(entry.source),
                    startup_status_label(entry.status)
                )
            })
            .collect(),
        // ゲーム準備チェックは1行の要約ではなく、項目ごとに並べて見せる（BRIEF §4 の準備確認画面）。
        ObservedValue::PowerToysInstallation(observed) => vec![
            format!(
                "導入状況 — {}",
                if observed.installed {
                    "導入済み"
                } else {
                    "未導入"
                }
            ),
            format!(
                "バージョン — {}",
                observed.version.as_deref().unwrap_or("不明")
            ),
            format!(
                "App Paths起動 — {}",
                if observed.launch_available {
                    "利用可能"
                } else {
                    "利用不可"
                }
            ),
        ],
        ObservedValue::KnownApps(value) => value
            .apps
            .iter()
            .map(|app| {
                let state = match app.state {
                    crate::action::KnownAppState::Running => "起動中",
                    crate::action::KnownAppState::NotRunning => "未起動",
                    crate::action::KnownAppState::Unavailable => "利用不可",
                };
                format!("{} — {state}", app.name)
            })
            .collect(),
        ObservedValue::WindowsUpdateStatus(value) => vec![
            format!(
                "最後に更新を確認した日時 — {}",
                match &value.last_checked_local {
                    ReadinessComponent::Known { value } => format!("{value}（ローカル時刻）"),
                    _ => "不明".to_owned(),
                }
            ),
            format!(
                "再起動保留 — {}",
                match &value.restart_pending {
                    ReadinessComponent::Known { value: true } => "あり",
                    ReadinessComponent::Known { value: false } => "なし",
                    _ => "不明",
                }
            ),
        ],
        ObservedValue::AudioOutput(value) => value
            .endpoints
            .iter()
            .map(|endpoint| {
                if endpoint.is_default {
                    format!("{} — 現在の既定", endpoint.friendly_name)
                } else {
                    endpoint.friendly_name.clone()
                }
            })
            .collect(),
        ObservedValue::CommunicationsMicrophone { muted } => vec![format!(
            "既定の通話用入力1台 — {}（排他モード等での無音は保証外）",
            if *muted {
                "ミュート設定"
            } else {
                "ミュート解除設定"
            }
        )],
        ObservedValue::WindowLayout(value) => value
            .issues
            .iter()
            .map(|issue| {
                let reason = match issue.reason {
                    crate::window_layout::WindowLayoutIssueReason::NotRunning => "見つかりません",
                    crate::window_layout::WindowLayoutIssueReason::AmbiguousMatch => {
                        "同じ候補が複数あるため操作しません"
                    }
                    crate::window_layout::WindowLayoutIssueReason::GameExcluded => {
                        "登録ゲームのため操作しません"
                    }
                    crate::window_layout::WindowLayoutIssueReason::VerificationMismatch => {
                        "復元結果を確認できません"
                    }
                };
                format!("{} — {reason}", issue.target)
            })
            .collect(),
        ObservedValue::GameReadiness(readiness) => {
            vec![
                format!(
                    "画面のリフレッシュレート — {}",
                    readiness_line_refresh(readiness)
                ),
                format!(
                    "HDR（Advanced Color） — {}",
                    readiness_line_advanced_color(readiness)
                ),
                format!(
                    "ゲームモードの設定値 — {}",
                    configured_toggle_hint_label(&readiness.game_mode)
                ),
                format!("電源プラン — {}", readiness_line_power(readiness)),
                format!(
                    "システムドライブの空き — {}",
                    readiness_line_space(readiness)
                ),
                format!("既定の音声出力 — {}", readiness_line_audio(readiness)),
                format!(
                    "通知の設定値 — {}",
                    configured_toggle_hint_label(&readiness.toast_notifications)
                ),
            ]
        }
        _ => Vec::new(),
    }
}

fn readiness_line_refresh(r: &GameReadinessObservation) -> String {
    match &r.refresh_rate {
        ReadinessComponent::Known { value } => format!("{} Hz", value.hertz),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    }
}

fn readiness_line_advanced_color(r: &GameReadinessObservation) -> String {
    match &r.advanced_color {
        ReadinessComponent::Known { value } => format!(
            "有効{} / 対応{} / 接続{}（HDRの実効状態とは断定しません）",
            value.enabled_path_count, value.supported_path_count, value.active_path_count
        ),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    }
}

/// Windows標準の電源プランGUIDを、利用者に見せる名前へ。
/// このプロダクトは専門用語を見せない方針なので、生のGUIDを画面に出さない。
/// OEM独自プランなど未知のGUIDは、推測せず「その他のプラン」と表示する。
pub fn power_scheme_display_name(guid: &str) -> String {
    let normalized = guid
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_lowercase();
    match normalized.as_str() {
        "381b4222-f694-41f0-9685-ff5bb260df2e" => "バランス".to_owned(),
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" => "高パフォーマンス".to_owned(),
        "a1841308-3541-4fab-bc81-f71556f20b4a" => "省電力".to_owned(),
        _ => "その他のプラン".to_owned(),
    }
}

fn readiness_line_power(r: &GameReadinessObservation) -> String {
    match &r.active_power_scheme {
        ReadinessComponent::Known { value } => power_scheme_display_name(value),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    }
}

fn readiness_line_space(r: &GameReadinessObservation) -> String {
    match &r.system_drive_space {
        ReadinessComponent::Known { value } => format_bytes(value.available_bytes),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    }
}

fn readiness_line_audio(r: &GameReadinessObservation) -> String {
    match &r.default_render_audio {
        ReadinessComponent::Known { value } if value.endpoint_exists => "既定の出力あり".to_owned(),
        ReadinessComponent::Known { .. } => "既定の出力なし".to_owned(),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    }
}

fn startup_inventory_detail(inventory: &StartupInventoryObservation) -> String {
    let sample = inventory
        .entries
        .iter()
        .take(5)
        .map(|entry| {
            format!(
                "{}（{}/{}）",
                entry.name,
                startup_source_label(entry.source),
                startup_status_label(entry.status)
            )
        })
        .collect::<Vec<_>>()
        .join("、");
    let remaining = inventory.entries.len().saturating_sub(5);
    let items = if sample.is_empty() {
        "項目なし".to_owned()
    } else if remaining == 0 {
        sample
    } else {
        format!("{sample}、ほか{remaining}件")
    };
    bounded(format!(
        "{items}。警告{}件。コマンド本文は保持していません。",
        inventory.warnings.len()
    ))
}

fn startup_source_label(source: StartupEntrySource) -> &'static str {
    match source {
        StartupEntrySource::CurrentUserRun => "ユーザーRun",
        StartupEntrySource::LocalMachineRun64 => "PC Run 64bit",
        StartupEntrySource::LocalMachineRun32 => "PC Run 32bit",
        StartupEntrySource::UserStartupFolder => "ユーザーStartup",
        StartupEntrySource::CommonStartupFolder => "共通Startup",
    }
}

fn startup_status_label(status: StartupEntryStatus) -> &'static str {
    match status {
        StartupEntryStatus::RegistryCommand => "登録値",
        StartupEntryStatus::RegistryExpandableCommand => "展開可能な登録値",
        StartupEntryStatus::StartupFile => "ファイル",
        StartupEntryStatus::ReparsePointNotFollowed => "reparse未追跡",
        StartupEntryStatus::MalformedRegistryValue => "不正な登録値",
        StartupEntryStatus::UnsupportedRegistryType => "未対応の型",
        StartupEntryStatus::RegistryValueTooLarge => "上限超過",
    }
}

fn game_readiness_detail(readiness: &GameReadinessObservation) -> String {
    let refresh = match &readiness.refresh_rate {
        ReadinessComponent::Known { value } => format!("{} Hz", value.hertz),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    };
    let advanced_color = match &readiness.advanced_color {
        ReadinessComponent::Known { value } => format!(
            "有効{}/対応{}/接続{}",
            value.enabled_path_count, value.supported_path_count, value.active_path_count
        ),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    };
    let game_mode_hint = configured_toggle_hint_label(&readiness.game_mode);
    let power = match &readiness.active_power_scheme {
        ReadinessComponent::Known { value } => value.clone(),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    };
    let free_space = match &readiness.system_drive_space {
        ReadinessComponent::Known { value } => format_bytes(value.available_bytes),
        ReadinessComponent::Unknown { .. } => "不明".to_owned(),
        ReadinessComponent::Unconfigured => "未設定".to_owned(),
    };
    let audio = match &readiness.default_render_audio {
        ReadinessComponent::Known { value } if value.endpoint_exists => "既定出力あり",
        ReadinessComponent::Known { .. } => "既定出力なし",
        ReadinessComponent::Unknown { .. } => "不明",
        ReadinessComponent::Unconfigured => "未設定",
    };
    let notifications_hint = configured_toggle_hint_label(&readiness.toast_notifications);
    bounded(format!(
        "Hz: {refresh} / Advanced Color情報: {advanced_color}（HDRとは断定しません） / Game Mode登録値の目安: {game_mode_hint} / 電源: {power} / 空き: {free_space} / 音声: {audio} / 通知登録値の目安: {notifications_hint}"
    ))
}

fn configured_toggle_hint_label(component: &ReadinessComponent<bool>) -> &'static str {
    match component {
        ReadinessComponent::Known { value: true } => "有効",
        ReadinessComponent::Known { value: false } => "無効",
        ReadinessComponent::Unknown { .. } => "不明",
        ReadinessComponent::Unconfigured => "未設定",
    }
}

fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        let tenths = bytes.saturating_mul(10) / GIB;
        format!("{}.{} GiB", tenths / 10, tenths % 10)
    } else {
        format!("{} MiB", bytes / MIB)
    }
}

fn bounded(value: String) -> String {
    value.chars().take(240).collect()
}

fn format_timestamp(unix_ms: u64) -> String {
    let value = i64::try_from(unix_ms).unwrap_or(i64::MAX);
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{
        Action, AdvancedColorObservation, DefaultRenderAudioObservation,
        PowerToysInstallationObservation, PrimaryRefreshRateObservation, StartupInventoryEntry,
        StateEvidence, SystemDriveSpaceObservation, ACTION_REGISTRY,
    };
    use crate::actions::START_LAYOUT_ACTION;
    use crate::compatibility::CompatibilityCatalog;

    #[test]
    fn shift_guard_current_state_distinguishes_trigger_from_confirmation() {
        assert_eq!(
            shift_trigger_state(false, true),
            "起動は無効（確認の設定だけ残っています）"
        );
        assert_eq!(
            shift_trigger_state(true, false),
            "確認なしで補助機能が始まる"
        );
    }

    #[test]
    fn power_scheme_request_schema_rejects_unknown_choice_and_extra_fields() {
        let valid = parse_action_request(PreviewActionRequest {
            action_id: "power.active_scheme_switch".to_owned(),
            parameters: serde_json::from_value(serde_json::json!({
                "scheme": "power_saver"
            }))
            .expect("parameter object"),
        })
        .expect("valid fixed power scheme");
        assert!(matches!(
            valid,
            ActionParameters::PowerActiveSchemeSwitch {
                scheme: PowerScheme::PowerSaver
            }
        ));

        for parameters in [
            serde_json::json!({ "scheme": "ultimate_performance" }),
            serde_json::json!({ "scheme": "balanced", "guid": "arbitrary" }),
        ] {
            let error = parse_action_request(PreviewActionRequest {
                action_id: "power.active_scheme_switch".to_owned(),
                parameters: serde_json::from_value(parameters).expect("parameter object"),
            })
            .expect_err("unregistered power choice or field must fail closed");
            assert_eq!(error.code, "INVALID_REQUEST");
        }
    }

    /// 実機確認が取れて可変にした項目の契約を固定する。
    /// 「反映にシェル再起動が要る」ものが自動適用に載ると、ゲーム起動の瞬間に
    /// 開いているフォルダーの窓が予告なく閉じることになる。そこは塞いだままにする。
    #[test]
    fn verified_registry_actions_are_mutable_but_never_auto_applied() {
        let verified = ACTION_REGISTRY
            .iter()
            .filter(|action| {
                action.metadata().method_class == MethodClass::DocumentedRegistry
                    && action.metadata().requiresExplorerRestart
            })
            .collect::<Vec<_>>();
        assert!(
            !verified.is_empty(),
            "実機確認で昇格した項目が1件も無い。昇格経路が壊れている可能性がある"
        );
        for action in verified {
            let metadata = action.metadata();
            assert_eq!(metadata.kind, ActionKind::Persistent, "{:?}", metadata.id);
            assert_ne!(metadata.maximumTestedBuild, 0, "{:?}", metadata.id);
            assert!(
                !metadata.auto_apply_eligible,
                "シェル再起動が要る項目を自動適用に載せてはいけない: {:?}",
                metadata.id
            );
            metadata
                .validate_static_contract()
                .expect("verified registry metadata must be internally consistent");
        }
    }

    #[test]
    fn unverified_registry_candidates_are_guided_read_only_and_never_claim_ui_state() {
        let candidates = ACTION_REGISTRY
            .iter()
            .filter(|action| action.metadata().method_class == MethodClass::UnverifiedStorage)
            .collect::<Vec<_>>();
        // 実機で往復確認が取れた項目はここから抜けて可変になる。件数は減る方向にしか動かない。
        // 増えていたら、確認していないものを候補として足したということなので止める。
        assert!(
            candidates.len() <= 41,
            "未検証候補が増えている: {}",
            candidates.len()
        );
        for action in candidates {
            let metadata = action.metadata();
            assert_eq!(metadata.kind, ActionKind::Guided);
            assert_eq!(metadata.maximumTestedBuild, 0);
            assert_eq!(metadata.riskLevel, ActionRiskLevel::Experimental);
            assert!(!metadata.auto_apply_eligible);
            metadata
                .validate_static_contract()
                .expect("guided storage candidate metadata must be internally consistent");
        }

        // 見本には「まだ未検証のまま」の項目を使う。昇格した項目を見本にすると、
        // 昇格のたびにこのテストが落ちる。start.layout は 3 値でまだ未検証。
        let metadata = START_LAYOUT_ACTION.metadata();
        let ui = action_presentation(
            metadata,
            CompatibilityCatalog::decision_for_build(26_100),
            Some(DetectedState::Known {
                value: ObservedValue::RegistryDword {
                    configured: Some(3),
                },
                evidence: StateEvidence {
                    source: "test storage observation".to_owned(),
                    observed_at_unix_ms: 1,
                    os_build: 26_100,
                },
            }),
        );
        assert_eq!(ui.kind, "guided");
        assert_eq!(ui.availability, "read_only");
        assert_eq!(ui.method_class, "unverified_storage");
        assert_eq!(ui.maximum_tested_build, None);
        assert!(!ui.reversible);
        assert_eq!(
            ui.current_state.as_ref().map(|state| state.label.as_str()),
            Some("HKCU保存値: DWORD 3")
        );
        assert!(ui
            .current_state
            .as_ref()
            .expect("storage state")
            .detail
            .contains("Windows UIの有効状態を示すものではありません"));
        assert!(!ui
            .current_state
            .as_ref()
            .expect("storage state")
            .label
            .contains("検索ボックス"));
    }

    #[test]
    fn official_windows_guides_are_not_presented_as_unverified_registry_candidates() {
        for id in [ActionId::SetupDefaultApps, ActionId::SetupAudioOutput] {
            let metadata = ACTION_REGISTRY
                .get(id)
                .expect("registered guide")
                .metadata();
            let ui = action_presentation(
                metadata,
                CompatibilityCatalog::decision_for_build(26_100),
                None,
            );
            let combined = format!(
                "{} {} {} {}",
                ui.name,
                ui.desired_state,
                ui.method_summary,
                ui.detail_points.join(" ")
            );
            assert_eq!(ui.kind, "guided");
            assert_eq!(ui.method_class, "public_api");
            assert!(ui.settings_page.is_some());
            assert!(!combined.contains("設計候補"));
            assert!(!combined.contains("固定HKCU"));
            assert!(!combined.contains("setter根拠"));
            assert!(combined.contains("Windows"));
        }
    }

    #[test]
    fn startup_inventory_detail_is_bounded_and_lists_safe_metadata() {
        let inventory = StartupInventoryObservation {
            entries: vec![StartupInventoryEntry {
                name: "Example Launcher".to_owned(),
                source: StartupEntrySource::CurrentUserRun,
                status: StartupEntryStatus::RegistryCommand,
            }],
            warnings: Vec::new(),
            truncated: false,
        };
        let detail = startup_inventory_detail(&inventory);
        assert!(detail.contains("Example Launcher（ユーザーRun/登録値）"));
        assert!(detail.contains("コマンド本文は保持していません"));
        assert!(detail.chars().count() <= 240);
        let items = observed_items(&ObservedValue::StartupInventory(inventory));
        assert_eq!(
            items,
            vec!["Example Launcher — ユーザーRun / 登録値".to_owned()]
        );
    }

    #[test]
    fn readiness_detail_exposes_each_component_without_overclaiming_hdr() {
        let readiness = GameReadinessObservation {
            refresh_rate: ReadinessComponent::Known {
                value: PrimaryRefreshRateObservation { hertz: 144 },
            },
            advanced_color: ReadinessComponent::Known {
                value: AdvancedColorObservation {
                    active_path_count: 2,
                    supported_path_count: 1,
                    enabled_path_count: 1,
                },
            },
            game_mode: ReadinessComponent::Known { value: true },
            active_power_scheme: ReadinessComponent::Known {
                value: "scheme-guid".to_owned(),
            },
            system_drive_space: ReadinessComponent::Known {
                value: SystemDriveSpaceObservation {
                    volume: "C:".to_owned(),
                    available_bytes: 20 * 1024 * 1024 * 1024,
                    total_bytes: 100 * 1024 * 1024 * 1024,
                    total_free_bytes: 25 * 1024 * 1024 * 1024,
                },
            },
            default_render_audio: ReadinessComponent::Known {
                value: DefaultRenderAudioObservation {
                    endpoint_exists: true,
                },
            },
            toast_notifications: ReadinessComponent::Known { value: false },
        };
        let detail = game_readiness_detail(&readiness);
        assert!(detail.contains("144 Hz"));
        assert!(detail.contains("Advanced Color情報: 有効1/対応1/接続2"));
        assert!(detail.contains("HDRとは断定しません"));
        assert!(detail.contains("Game Mode登録値の目安: 有効"));
        assert!(detail.contains("音声: 既定出力あり"));
        assert!(detail.contains("通知登録値の目安: 無効"));
        assert!(!detail.contains("HDR: 有効"));
        assert!(detail.chars().count() <= 240);
    }

    #[test]
    fn readiness_detail_preserves_unconfigured_and_unknown_registry_hints() {
        let readiness = GameReadinessObservation {
            refresh_rate: ReadinessComponent::Unconfigured,
            advanced_color: ReadinessComponent::Unknown {
                reason_code: "advanced_color_unavailable".to_owned(),
            },
            game_mode: ReadinessComponent::Unconfigured,
            active_power_scheme: ReadinessComponent::Unknown {
                reason_code: "active_power_scheme_unavailable".to_owned(),
            },
            system_drive_space: ReadinessComponent::Unconfigured,
            default_render_audio: ReadinessComponent::Unknown {
                reason_code: "default_render_audio_unavailable".to_owned(),
            },
            toast_notifications: ReadinessComponent::Unknown {
                reason_code: "toast_notifications_registry_unavailable".to_owned(),
            },
        };

        let detail = game_readiness_detail(&readiness);
        assert!(detail.contains("Advanced Color情報: 不明"));
        assert!(detail.contains("Game Mode登録値の目安: 未設定"));
        assert!(detail.contains("通知登録値の目安: 不明"));
        assert!(!detail.contains("Game Mode: 無効"));
        assert!(!detail.contains("通知: 無効"));
        assert!(detail.chars().count() <= 240);
    }

    #[test]
    fn powertoys_observation_keeps_typed_install_and_launch_state() {
        let metadata = ACTION_REGISTRY
            .get(ActionId::SetupPowerToysStatus)
            .expect("PowerToys status action registered")
            .metadata();
        let ui = action_presentation(
            metadata,
            CompatibilityCatalog::decision_for_build(26_200),
            Some(DetectedState::Known {
                value: ObservedValue::PowerToysInstallation(PowerToysInstallationObservation {
                    installed: true,
                    version: Some("0.99.1".to_owned()),
                    launch_available: true,
                }),
                evidence: StateEvidence {
                    source: "documented App Paths test".to_owned(),
                    observed_at_unix_ms: 1,
                    os_build: 26_200,
                },
            }),
        );
        assert_eq!(ui.id, "setup.powertoys_status");
        assert_eq!(ui.kind, "observation");
        assert_eq!(ui.availability, "read_only");
        assert_eq!(ui.method_class, "documented_registry");
        let state = ui.current_state.expect("presented PowerToys state");
        assert!(state.label.contains("0.99.1"));
        let integration = state.integration.expect("typed integration state");
        assert!(integration.installed);
        assert!(integration.launch_available);
        assert_eq!(integration.version.as_deref(), Some("0.99.1"));
    }

    #[test]
    fn launch_apps_schema_and_presentation_are_fail_closed_and_one_way() {
        let valid = parse_action_request(PreviewActionRequest {
            action_id: "setup.launch_apps".to_owned(),
            parameters: serde_json::from_value(serde_json::json!({ "bundle": "study" }))
                .expect("parameter object"),
        })
        .expect("fixed bundle");
        assert!(matches!(
            valid,
            ActionParameters::SetupLaunchApps {
                bundle: AppLaunchBundle::Study
            }
        ));
        let powertoys = parse_action_request(PreviewActionRequest {
            action_id: "setup.launch_apps".to_owned(),
            parameters: serde_json::from_value(serde_json::json!({ "bundle": "power_toys" }))
                .expect("parameter object"),
        })
        .expect("fixed PowerToys bundle");
        assert!(matches!(
            powertoys,
            ActionParameters::SetupLaunchApps {
                bundle: AppLaunchBundle::PowerToys
            }
        ));

        for parameters in [
            serde_json::json!({ "bundle": "unknown" }),
            serde_json::json!({ "bundle": "study", "path": "C:\\arbitrary.exe" }),
            serde_json::json!({ "bundle": "study", "arguments": ["user-value"] }),
        ] {
            let error = parse_action_request(PreviewActionRequest {
                action_id: "setup.launch_apps".to_owned(),
                parameters: serde_json::from_value(parameters).expect("parameter object"),
            })
            .expect_err("unknown bundle, path, or arguments must fail closed");
            assert_eq!(error.code, "INVALID_REQUEST");
        }

        let metadata = ACTION_REGISTRY
            .get(ActionId::SetupLaunchApps)
            .expect("launch action registered")
            .metadata();
        let ui = action_presentation(
            metadata,
            CompatibilityCatalog::decision_for_build(26_200),
            None,
        );
        assert_eq!(ui.category, "setup");
        assert_eq!(ui.kind, "one_way");
        assert_eq!(ui.availability, "mutable");
        assert!(!ui.auto_apply_eligible);
        assert!(!ui.reversible);
        assert!(!ui.requires_admin);
        assert!(!ui.requires_restart);
    }
    #[test]
    fn expensive_observations_are_explicit_only_in_catalog_snapshots() {
        for id in [
            ActionId::SetupStartupInventory,
            ActionId::StorageFreeSpaceCheck,
            ActionId::StorageTempFilesCheck,
            ActionId::GamesReadinessCheck,
            ActionId::SetupWindowsUpdateStatus,
        ] {
            assert!(listing_parameters(id).is_none());
            assert!(default_parameters(id).is_some());
        }
        assert!(listing_parameters(ActionId::PowerActiveSchemeCheck).is_some());
        assert!(listing_parameters(ActionId::SetupPowerToysStatus).is_some());
    }
}

#[cfg(test)]
mod category_contract {
    use super::*;

    /// 画面が知っているカテゴリの一覧。`src/model.ts` の `CategoryId` と同じ。
    const KNOWN: [&str; 9] = [
        "session",
        "power",
        "explorer",
        "appearance",
        "games",
        "setup",
        "storage",
        "notifications",
        "input",
    ];

    /// 知らないカテゴリを返す Action が1つでもあると、画面は一覧をまるごと拒否する。
    ///
    /// 以前は画面側が Action ID ごとのカテゴリ表を持っていて、
    /// 新しい Action を足して書き忘れると**アプリが何も表示しなくなった。**
    /// 表は無くしたので、あとはコアが知らない値を出さないことだけを見ればよい。
    #[test]
    fn every_action_reports_a_category_the_screen_knows() {
        for action_id in ActionId::ALL {
            let category = category_for(action_id);
            assert!(
                KNOWN.contains(&category),
                "{} のカテゴリ {} を画面が知らない",
                action_id.as_str(),
                category
            );
        }
    }
}

#[cfg(test)]
mod request_round_trip {
    use super::*;

    /// 画面から来る形で、登録されている全 Action を実際の入口に通す。
    ///
    /// `ActionParameters` は `action_id` でタグ付けされた enum なので、
    /// 変種に `#[serde(rename = "...")]` を付け忘れると、その Action は
    /// **画面から一切呼べなくなる。** それでもコンパイルは通り、
    /// `ActionParameters` を直に組み立てる単体テストも全部通る。
    /// 実際に1つ付け忘れて、この経路を通すまで気付かなかった。
    #[test]
    fn every_registered_action_can_be_requested_from_the_screen() {
        for action_id in ActionId::ALL {
            let Some(defaults) = default_parameters(action_id) else {
                // 画面から直接は呼ばない Action。引数の作り方が別にある。
                continue;
            };
            let serialized = serde_json::to_value(&defaults).expect("serialize defaults");
            let parameters = serialized
                .get("parameters")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let request = PreviewActionRequest {
                action_id: action_id.as_str().to_owned(),
                parameters,
            };
            let parsed = parse_action_request(request)
                .unwrap_or_else(|_| panic!("{} を画面から要求できない", action_id.as_str()));
            assert_eq!(
                parsed.action_id(),
                action_id,
                "{} が別の Action として解釈された",
                action_id.as_str()
            );
        }
    }

    /// タグ名は Action ID と一字一句同じでなければならない。
    #[test]
    fn the_serialized_tag_is_exactly_the_action_id() {
        for action_id in ActionId::ALL {
            let Some(defaults) = default_parameters(action_id) else {
                continue;
            };
            let serialized = serde_json::to_value(&defaults).expect("serialize defaults");
            assert_eq!(
                serialized.get("action_id").and_then(Value::as_str),
                Some(action_id.as_str()),
                "{} のタグ名が Action ID と違う",
                action_id.as_str()
            );
        }
    }
}

#[cfg(test)]
mod count_report {
    use super::*;
    use crate::action::ACTION_REGISTRY;

    /// README に載せる件数を実データから数える。
    ///
    /// **手で書いた件数は2回ずれた。** 公開している README が自分の数を間違えるのは、
    /// 訪問者が最初に確かめられる場所なので、ここで数えて突き合わせる。
    #[test]
    fn dump_action_counts() {
        let mut persistent = 0;
        let mut session = 0;
        let mut observation = 0;
        let mut guided_candidate = 0;
        let mut guided_settings = 0;
        for action in ACTION_REGISTRY.iter() {
            let m = action.metadata();
            match m.kind {
                ActionKind::Persistent => persistent += 1,
                ActionKind::Session => session += 1,
                ActionKind::Observation => observation += 1,
                ActionKind::Guided => {
                    if m.method_class == MethodClass::UnverifiedStorage {
                        guided_candidate += 1;
                    } else {
                        guided_settings += 1;
                    }
                }
                _ => {}
            }
        }
        let total = ACTION_REGISTRY.iter().count();
        println!(
            "総数={} 変更できる(persistent+session)={} 確認のみ={} 表示専用={} 設定案内={}",
            total,
            persistent + session,
            observation,
            guided_candidate,
            guided_settings
        );
        assert_eq!(total, 71, "総数が変わったら README も直すこと");
        assert!(
            guided_candidate <= 39,
            "表示専用が増えている。確認していないものを足していないか"
        );
    }
}
