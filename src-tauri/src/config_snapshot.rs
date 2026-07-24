//! 現在設定のバックアップ（BRIEF §3「現在設定のバックアップ」）。
//!
//! 検出済みのAction状態を、人が読めるdata-only JSONとして書き出す。
//! - 読み取り専用。Windowsを一切変更しない。
//! - 出力はActionの表示情報と観測状態のみ。コマンド本文・パス・個人情報は含めない。
//! - 移行先PCでは「控え」として参照する（自動適用はしない。適用は必ずpreview→commitを通す）。

use serde::{Deserialize, Serialize};

use crate::presentation::ActionPresentation;

pub const SETTINGS_SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSnapshotEntry {
    pub action_id: String,
    pub name: String,
    pub category: String,
    /// persistent / session / observation / guided
    pub kind: String,
    /// mutable / read_only / detect_only / blocked
    pub availability: String,
    /// known / unknown / unconfigured / unavailable（未検出は "not_detected"）
    pub state_kind: String,
    pub state_label: String,
    pub state_detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSnapshot {
    pub version: u32,
    pub captured_at: String,
    pub os_build: Option<u32>,
    pub entry_count: usize,
    pub entries: Vec<SettingsSnapshotEntry>,
}

/// 純関数: 表示用Action一覧から控えを組み立てる。I/Oを行わないためテストしやすい。
pub fn build_settings_snapshot(
    actions: &[ActionPresentation],
    os_build: Option<u32>,
    captured_at: String,
) -> SettingsSnapshot {
    let entries: Vec<SettingsSnapshotEntry> = actions
        .iter()
        .map(|action| {
            let (state_kind, state_label, state_detail) = match &action.current_state {
                Some(state) => (
                    state.kind.clone(),
                    state.label.clone(),
                    state.detail.clone(),
                ),
                None => (
                    "not_detected".to_owned(),
                    "未取得".to_owned(),
                    "この控えを作った時点では状態を取得していません。".to_owned(),
                ),
            };
            SettingsSnapshotEntry {
                action_id: action.id.clone(),
                name: action.name.clone(),
                category: action.category.clone(),
                kind: action.kind.clone(),
                availability: action.availability.clone(),
                state_kind,
                state_label,
                state_detail,
            }
        })
        .collect();
    SettingsSnapshot {
        version: SETTINGS_SNAPSHOT_VERSION,
        captured_at,
        os_build,
        entry_count: entries.len(),
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::UiActionState;

    fn presentation(
        id: &str,
        kind: &str,
        availability: &str,
        state: Option<UiActionState>,
    ) -> ActionPresentation {
        ActionPresentation {
            id: id.to_owned(),
            action_version: 1,
            name: "テスト用".to_owned(),
            description: "説明".to_owned(),
            audience: "誰か".to_owned(),
            category: "appearance".to_owned(),
            tags: vec![],
            supported_windows_versions: vec![],
            minimum_build: 26_100,
            maximum_tested_build: None,
            risk_level: "安全".to_owned(),
            requires_admin: false,
            requires_restart: false,
            requires_explorer_restart: false,
            update_impact: "低".to_owned(),
            reversible: true,
            kind: kind.to_owned(),
            auto_apply_eligible: true,
            availability: availability.to_owned(),
            method_class: "documented_registry".to_owned(),
            method_summary: "HKCU".to_owned(),
            desired_state: "オン".to_owned(),
            current_state: state,
            detail_points: vec![],
        }
    }

    fn state(kind: &str) -> UiActionState {
        UiActionState {
            kind: kind.to_owned(),
            label: "オン".to_owned(),
            detail: "現在の値".to_owned(),
            items: vec![],
            observed_at: Some("2026-07-25T00:00:00Z".to_owned()),
        }
    }

    #[test]
    fn snapshot_records_every_action_and_is_stable_json() {
        let actions = vec![
            presentation("explorer.show_extensions", "persistent", "mutable", Some(state("known"))),
            presentation("taskbar.alignment", "guided", "detect_only", Some(state("known"))),
        ];
        let snapshot = build_settings_snapshot(&actions, Some(26_200), "2026-07-25T00:00:00Z".to_owned());
        assert_eq!(snapshot.version, SETTINGS_SNAPSHOT_VERSION);
        assert_eq!(snapshot.entry_count, 2);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.os_build, Some(26_200));
        // data-onlyでラウンドトリップできる（未知fieldは拒否）。
        let json = serde_json::to_string(&snapshot).expect("serialize");
        let parsed: SettingsSnapshot = serde_json::from_str(&json).expect("round trip");
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn undetected_action_is_recorded_without_guessing_state() {
        let actions = vec![presentation("power.active_scheme_check", "observation", "read_only", None)];
        let snapshot = build_settings_snapshot(&actions, None, "2026-07-25T00:00:00Z".to_owned());
        let entry = &snapshot.entries[0];
        assert_eq!(entry.state_kind, "not_detected");
        assert_eq!(entry.state_label, "未取得");
        assert_eq!(snapshot.os_build, None);
    }

    #[test]
    fn snapshot_contains_no_command_bodies_or_paths() {
        // 控えに載せるのは表示情報と観測ラベルだけで、生の実行文字列やフルパスは含めない。
        let actions = vec![presentation("setup.startup_inventory", "observation", "read_only", Some(state("known")))];
        let snapshot = build_settings_snapshot(&actions, Some(26_200), "2026-07-25T00:00:00Z".to_owned());
        let json = serde_json::to_string(&snapshot).expect("serialize");
        assert!(!json.contains(r"C:\\"));
        assert!(!json.contains("powershell"));
        assert!(!json.contains("cmd.exe"));
    }
}
