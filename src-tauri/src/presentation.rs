use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    action::{
        ActionId, ActionKind, ActionMetadata, ActionParameters, ActionRiskLevel,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, ThemeColorMode,
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
pub struct UiActionState {
    pub kind: String,
    pub label: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
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
    pub availability: String,
    pub method_summary: String,
    pub desired_state: String,
    pub current_state: Option<UiActionState>,
    pub detail_points: Vec<String>,
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
}

pub fn action_presentation(
    metadata: &ActionMetadata,
    compatibility: CompatibilityDecision,
    current_state: Option<DetectedState>,
) -> ActionPresentation {
    let kind = match metadata.kind {
        ActionKind::Persistent => "persistent",
        ActionKind::Session => "session",
        ActionKind::Observation | ActionKind::Guided => "observation",
    };
    let availability = match compatibility.mode {
        CompatibilityMode::TestedMutable
            if matches!(metadata.kind, ActionKind::Persistent | ActionKind::Session) =>
        {
            "mutable"
        }
        CompatibilityMode::TestedMutable | CompatibilityMode::TestedDetectOnly
            if metadata.kind == ActionKind::Observation =>
        {
            "read_only"
        }
        CompatibilityMode::TestedDetectOnly | CompatibilityMode::UnknownBuild => "detect_only",
        CompatibilityMode::Unsupported => "blocked",
        _ => "blocked",
    };
    ActionPresentation {
        id: metadata.id.as_str().to_owned(),
        action_version: metadata.action_version,
        name: metadata.name.to_owned(),
        description: metadata.description.to_owned(),
        audience: audience_for(metadata.id).to_owned(),
        category: category_for(metadata.id).to_owned(),
        tags: metadata.tags.iter().map(|value| (*value).to_owned()).collect(),
        supported_windows_versions: metadata
            .supportedWindowsVersions
            .iter()
            .map(|release| format!("{:?}", release).replace("Windows11_", "Windows 11 "))
            .collect(),
        minimum_build: metadata.minimumBuild,
        maximum_tested_build: Some(metadata.maximumTestedBuild),
        risk_level: risk_name(metadata.riskLevel).to_owned(),
        requires_admin: metadata.requiresAdmin,
        requires_restart: metadata.requiresRestart,
        requires_explorer_restart: metadata.requiresExplorerRestart,
        update_impact: update_impact(metadata.windows_update_impact).to_owned(),
        reversible: metadata.kind != ActionKind::Observation,
        kind: kind.to_owned(),
        availability: availability.to_owned(),
        method_summary: method_name(metadata.method_class).to_owned(),
        desired_state: desired_state(metadata.id).to_owned(),
        current_state: current_state.map(|state| state_to_ui(metadata.id, state)),
        detail_points: vec![
            "適用前の状態を耐久記録へ保存してから変更します。".to_owned(),
            "適用直後と復元直後に、Windowsから状態を再取得して検証します。".to_owned(),
            "外部変更を検出した場合は、自動で上書きしません。".to_owned(),
        ],
    }
}

pub fn state_to_ui(action_id: ActionId, state: DetectedState) -> UiActionState {
    match state {
        DetectedState::Known { value, evidence } => UiActionState {
            kind: "known".to_owned(),
            label: observed_label(action_id, &value),
            detail: observed_detail(&value),
            observed_at: Some(format_timestamp(evidence.observed_at_unix_ms)),
        },
        DetectedState::NeedsRestart { value, evidence } => UiActionState {
            kind: "known".to_owned(),
            label: observed_label(action_id, &value),
            detail: "設定値は確認できました。反映にはアプリ側の再読込が必要です。".to_owned(),
            observed_at: Some(format_timestamp(evidence.observed_at_unix_ms)),
        },
        DetectedState::Unknown { reason } => UiActionState {
            kind: "unknown".to_owned(),
            label: "確認できません".to_owned(),
            detail: bounded(reason),
            observed_at: None,
        },
        DetectedState::Unsupported { reason } => UiActionState {
            kind: "unsupported".to_owned(),
            label: "この環境では利用できません".to_owned(),
            detail: bounded(reason),
            observed_at: None,
        },
        DetectedState::PolicyManaged { authority } => UiActionState {
            kind: "policy_managed".to_owned(),
            label: "組織のポリシーで管理されています".to_owned(),
            detail: authority
                .map(bounded)
                .unwrap_or_else(|| "Totonoeからは上書きしません。".to_owned()),
            observed_at: None,
        },
        DetectedState::Conflict { .. } => UiActionState {
            kind: "unknown".to_owned(),
            label: "別の変更を検出しました".to_owned(),
            detail: "保存した適用値と現在値が異なるため、自動操作を止めています。".to_owned(),
            observed_at: None,
        },
        DetectedState::Error { code, reason } => UiActionState {
            kind: "error".to_owned(),
            label: "状態確認に失敗しました".to_owned(),
            detail: format!("{} ({})", bounded(reason), bounded(code)),
            observed_at: None,
        },
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
        before: state_to_ui(metadata.id, before.clone()).label,
        after: desired_state(metadata.id).to_owned(),
        method: explanation.method.clone(),
        resource_label: explanation.resources.join(" / "),
        risk_level: risk_name(metadata.riskLevel).to_owned(),
        reversible: metadata.kind != ActionKind::Observation,
    }
}

pub fn parse_action_request(request: PreviewActionRequest) -> CoreResult<ActionParameters> {
    let action_id = ActionId::from_str(&request.action_id).map_err(|_| {
        CoreError::invalid_request("登録されていないAction IDは実行できません。")
    })?;
    let mut parameters = request.parameters;
    if let Some(value) = parameters.remove("keepDisplayOn") {
        if parameters.insert("keep_display_on".to_owned(), value).is_some() {
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
        ActionId::PowerActiveSchemeCheck => ActionParameters::PowerActiveSchemeCheck {},
        ActionId::ExplorerShowExtensions => {
            ActionParameters::ExplorerShowExtensions { show: true }
        }
        ActionId::ExplorerShowHidden => ActionParameters::ExplorerShowHidden { show: true },
        ActionId::ExplorerClockSeconds => ActionParameters::ExplorerClockSeconds { show: true },
        ActionId::ThemeColorMode => ActionParameters::ThemeColorMode {
            mode: ThemeColorMode::Dark,
        },
        ActionId::GamesProcessWatch => return None,
    })
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
        ActionId::PowerActiveSchemeCheck => "power",
        ActionId::ExplorerShowExtensions
        | ActionId::ExplorerShowHidden
        | ActionId::ExplorerClockSeconds => "explorer",
        ActionId::ThemeColorMode => "appearance",
        ActionId::GamesProcessWatch => "games",
    }
}


fn audience_for(action_id: ActionId) -> &'static str {
    match action_id {
        ActionId::SessionPreventSleep => "長い作業やゲーム中に、自動スリープを避けたい人向け",
        ActionId::PowerActiveSchemeCheck => "現在の電源構成を変更せず確認したい人向け",
        ActionId::ExplorerShowExtensions => "ファイルの種類を見分け、誤操作を減らしたい人向け",
        ActionId::ExplorerShowHidden => "隠しファイルを扱う必要がある人向け",
        ActionId::ExplorerClockSeconds => "タスクバーの時計で秒まで確認したい人向け",
        ActionId::ThemeColorMode => "Windowsとアプリの明暗を揃えたい人向け",
        ActionId::GamesProcessWatch => "登録したゲームの起動と終了だけを安全に検知したい人向け",
    }
}

fn desired_state(action_id: ActionId) -> &'static str {
    match action_id {
        ActionId::SessionPreventSleep => "自動スリープを一時的に防ぐ",
        ActionId::PowerActiveSchemeCheck => "変更せず、現在の電源設定を確認",
        ActionId::ExplorerShowExtensions => "拡張子を表示",
        ActionId::ExplorerShowHidden => "隠しファイルを表示",
        ActionId::ExplorerClockSeconds => "タスクバーの時計に秒を表示",
        ActionId::ThemeColorMode => "選択したライト／ダーク表示",
        ActionId::GamesProcessWatch => "本人性を確認できたプロセスだけ監視",
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
        ObservedValue::RegistryDword { configured } => match (action_id, configured) {
            (ActionId::ExplorerShowExtensions, Some(0)) => "拡張子を表示中".to_owned(),
            (ActionId::ExplorerShowExtensions, Some(1)) => "拡張子を非表示".to_owned(),
            (ActionId::ExplorerShowHidden, Some(1)) => "隠しファイルを表示中".to_owned(),
            (ActionId::ExplorerShowHidden, Some(2)) => "隠しファイルを非表示".to_owned(),
            (_, Some(value)) => format!("設定値 {value}"),
            (_, None) => "値は未設定".to_owned(),
        },
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
                "Totonoeのスリープ防止なし".to_owned()
            }
        }
        ObservedValue::ActivePowerScheme { guid } => format!("有効な電源プラン: {guid}"),
        ObservedValue::Processes { matches } => {
            format!("本人性を確認できたプロセス {}件", matches.len())
        }
        ObservedValue::NoOsChange => "OS設定の変更なし".to_owned(),
    }
}

fn observed_detail(value: &ObservedValue) -> String {
    match value {
        ObservedValue::RegistryDword { .. } => {
            "HKCUの限定値をraw type/bytesで確認しました。".to_owned()
        }
        ObservedValue::Theme(_) => {
            "アプリとシステムの2値を別々に確認しています。".to_owned()
        }
        ObservedValue::SleepLease { .. } => {
            "Windows全体ではなく、Totonoeが所有するleaseだけを表示します。".to_owned()
        }
        ObservedValue::ActivePowerScheme { .. } => {
            "公開Power APIで読み取りました。設定は変更していません。".to_owned()
        }
        ObservedValue::Processes { .. } => {
            "PID、作成時刻、正規化パス、file identityを照合しています。".to_owned()
        }
        ObservedValue::NoOsChange => "読み取り専用Actionです。".to_owned(),
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

