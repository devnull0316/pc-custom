//! 現在設定のバックアップ（BRIEF §3「現在設定のバックアップ」）。
//!
//! 検出済みのAction状態を、人が読めるdata-only JSONとして書き出す。
//! - 読み取り専用。Windowsを一切変更しない。
//! - 出力はActionの表示情報と観測状態のみ。コマンド本文・パス・個人情報は含めない。
//! - 移行先PCでは「控え」として参照する（自動適用はしない。適用は必ずpreview→commitを通す）。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::presentation::ActionPresentation;

pub const SETTINGS_SNAPSHOT_VERSION: u32 = 1;
pub const MAX_CUSTOM_CARD_JSON_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CUSTOM_CARD_ENTRIES: usize = 500;

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
    /// 構造化された観測値から作った照合キー。表示文言は比較に使わない。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_comparison_key: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCardEntry {
    pub action_id: String,
    pub name: String,
    pub category: String,
    /// 何が起きているかの説明（原因は書かない）
    pub note: String,
    pub card_state_label: Option<String>,
    pub current_state_label: Option<String>,
}

/// マイPCカスタムカード照合レポート（read-only）。
/// カードと現在の状態を比較した結果。Windowsの設定変更は一切行わない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCardReport {
    pub captured_at: String,
    pub os_build_note: Option<String>,
    pub matching: Vec<CustomCardEntry>,
    pub changed: Vec<CustomCardEntry>,
    pub missing_in_current: Vec<CustomCardEntry>,
    pub missing_in_card: Vec<CustomCardEntry>,
    pub unknown: Vec<CustomCardEntry>,
    pub summary: String,
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
            let (state_kind, state_label, state_detail, state_comparison_key) =
                match &action.current_state {
                    Some(state) => (
                        state.kind.clone(),
                        state.label.clone(),
                        state.detail.clone(),
                        state.comparison_key.clone(),
                    ),
                    None => (
                        "not_detected".to_owned(),
                        "未取得".to_owned(),
                        "この控えを作った時点では状態を取得していません。".to_owned(),
                        None,
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
                state_comparison_key,
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

/// 純関数: 読み込んだカスタムカード(JSON)と現在のPC状態を照合する（read-only）。
/// Windowsの状態は一切変更しない。適用も行わない。
pub fn inspect_custom_card(
    card_json: &str,
    actions: &[ActionPresentation],
    current_os_build: Option<u32>,
) -> CoreResult<CustomCardReport> {
    if card_json.len() > MAX_CUSTOM_CARD_JSON_BYTES {
        return Err(CoreError::invalid_request(
            "カスタムカードのデータサイズが大きすぎます。",
        ));
    }

    let snapshot: SettingsSnapshot = serde_json::from_str(card_json)
        .map_err(|_| CoreError::invalid_request("カスタムカードの形式が正しくありません。"))?;

    if snapshot.version != SETTINGS_SNAPSHOT_VERSION {
        return Err(CoreError::invalid_request(
            "この版のカスタムカードには対応していません。",
        ));
    }

    if snapshot.entries.len() > MAX_CUSTOM_CARD_ENTRIES {
        return Err(CoreError::invalid_request(
            "カスタムカードに含まれる項目数が多すぎます。",
        ));
    }

    if snapshot.entry_count != snapshot.entries.len() {
        return Err(CoreError::invalid_request(
            "カスタムカードの項目数が一致しません。",
        ));
    }

    let mut card_action_ids = HashSet::new();
    if snapshot
        .entries
        .iter()
        .any(|entry| !card_action_ids.insert(entry.action_id.as_str()))
    {
        return Err(CoreError::invalid_request(
            "カスタムカードに同じ項目が複数含まれています。",
        ));
    }

    let os_build_note = match (snapshot.os_build, current_os_build) {
        (Some(card_build), Some(curr_build)) if card_build != curr_build => Some(format!(
            "カードを作成した時点のWindowsビルド({card_build})と現在のビルド({curr_build})が異なります。"
        )),
        _ => None,
    };

    let mut matching = Vec::new();
    let mut changed = Vec::new();
    let mut missing_in_current = Vec::new();
    let mut missing_in_card = Vec::new();
    let mut unknown = Vec::new();

    let curr_map: HashMap<&str, &ActionPresentation> = actions
        .iter()
        .map(|action| (action.id.as_str(), action))
        .collect();

    let mut visited_card_action_ids = HashSet::new();

    for card_entry in &snapshot.entries {
        visited_card_action_ids.insert(card_entry.action_id.as_str());

        match curr_map.get(card_entry.action_id.as_str()) {
            None => {
                missing_in_current.push(CustomCardEntry {
                    action_id: card_entry.action_id.clone(),
                    name: card_entry.name.clone(),
                    category: card_entry.category.clone(),
                    note: "カードに記録されていますが、現在の環境には該当する項目がありません。"
                        .to_owned(),
                    card_state_label: Some(card_entry.state_label.clone()),
                    current_state_label: None,
                });
            }
            Some(curr_action) => {
                let curr_state = &curr_action.current_state;

                let is_card_unknown =
                    card_entry.state_kind != "known" || card_entry.state_comparison_key.is_none();
                let is_curr_unknown = curr_state
                    .as_ref()
                    .is_none_or(|state| state.kind != "known" || state.comparison_key.is_none());

                if is_card_unknown || is_curr_unknown {
                    unknown.push(CustomCardEntry {
                        action_id: card_entry.action_id.clone(),
                        name: curr_action.name.clone(),
                        category: curr_action.category.clone(),
                        note: "カード作成時または現在の状態を確認できませんでした。".to_owned(),
                        card_state_label: Some(card_entry.state_label.clone()),
                        current_state_label: curr_state.as_ref().map(|s| s.label.clone()),
                    });
                } else {
                    let curr_label = curr_state.as_ref().map(|s| s.label.as_str()).unwrap_or("");
                    let curr_comparison_key = curr_state
                        .as_ref()
                        .and_then(|state| state.comparison_key.as_deref());
                    if card_entry.state_comparison_key.as_deref() == curr_comparison_key {
                        matching.push(CustomCardEntry {
                            action_id: card_entry.action_id.clone(),
                            name: curr_action.name.clone(),
                            category: curr_action.category.clone(),
                            note: "カード作成時と同じ状態です。".to_owned(),
                            card_state_label: Some(card_entry.state_label.clone()),
                            current_state_label: Some(curr_label.to_owned()),
                        });
                    } else {
                        changed.push(CustomCardEntry {
                            action_id: card_entry.action_id.clone(),
                            name: curr_action.name.clone(),
                            category: curr_action.category.clone(),
                            note: format!(
                                "カードの記録は「{}」、現在は「{}」です。",
                                card_entry.state_label, curr_label
                            ),
                            card_state_label: Some(card_entry.state_label.clone()),
                            current_state_label: Some(curr_label.to_owned()),
                        });
                    }
                }
            }
        }
    }

    for action in actions {
        if !visited_card_action_ids.contains(action.id.as_str()) {
            let curr_label = action.current_state.as_ref().map(|s| s.label.clone());
            missing_in_card.push(CustomCardEntry {
                action_id: action.id.clone(),
                name: action.name.clone(),
                category: action.category.clone(),
                note: "現在のPCに存在しますが、カードには記録されていません。".to_owned(),
                card_state_label: None,
                current_state_label: curr_label,
            });
        }
    }

    let summary = format!(
        "照合結果: 同じ状態が{}件、変更あり{}件、確認不可{}件、カードに無い項目{}件、今に無い項目{}件です。",
        matching.len(),
        changed.len(),
        unknown.len(),
        missing_in_card.len(),
        missing_in_current.len()
    );

    Ok(CustomCardReport {
        captured_at: snapshot.captured_at,
        os_build_note,
        matching,
        changed,
        missing_in_current,
        missing_in_card,
        unknown,
        summary,
    })
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
            name: format!("テスト項目 {id}"),
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
            settings_page: None,
        }
    }

    fn state(kind: &str, label: &str) -> UiActionState {
        state_with_key(kind, label, &format!("{kind}:{label}"))
    }

    fn state_with_key(kind: &str, label: &str, comparison_key: &str) -> UiActionState {
        UiActionState {
            kind: kind.to_owned(),
            label: label.to_owned(),
            detail: "現在の値".to_owned(),
            comparison_key: Some(comparison_key.to_owned()),
            items: vec![],
            observed_at: Some("2026-07-25T00:00:00Z".to_owned()),
            integration: None,
        }
    }

    #[test]
    fn snapshot_records_every_action_and_is_stable_json() {
        let actions = vec![
            presentation(
                "explorer.show_extensions",
                "persistent",
                "mutable",
                Some(state("known", "オン")),
            ),
            presentation(
                "taskbar.alignment",
                "guided",
                "detect_only",
                Some(state("known", "中央")),
            ),
        ];
        let snapshot =
            build_settings_snapshot(&actions, Some(26_200), "2026-07-25T00:00:00Z".to_owned());
        assert_eq!(snapshot.version, SETTINGS_SNAPSHOT_VERSION);
        assert_eq!(snapshot.entry_count, 2);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.os_build, Some(26_200));
        let json = serde_json::to_string(&snapshot).expect("serialize");
        let parsed: SettingsSnapshot = serde_json::from_str(&json).expect("round trip");
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn undetected_action_is_recorded_without_guessing_state() {
        let actions = vec![presentation(
            "power.active_scheme_check",
            "observation",
            "read_only",
            None,
        )];
        let snapshot = build_settings_snapshot(&actions, None, "2026-07-25T00:00:00Z".to_owned());
        let entry = &snapshot.entries[0];
        assert_eq!(entry.state_kind, "not_detected");
        assert_eq!(entry.state_label, "未取得");
        assert_eq!(snapshot.os_build, None);
    }

    #[test]
    fn snapshot_contains_no_command_bodies_or_paths() {
        let actions = vec![presentation(
            "setup.startup_inventory",
            "observation",
            "read_only",
            Some(state("known", "確認済み")),
        )];
        let snapshot =
            build_settings_snapshot(&actions, Some(26_200), "2026-07-25T00:00:00Z".to_owned());
        let json = serde_json::to_string(&snapshot).expect("serialize");
        assert!(!json.contains(r"C:\\"));
        assert!(!json.contains("powershell"));
        assert!(!json.contains("cmd.exe"));
    }

    #[test]
    fn missing_in_card_and_missing_in_current_land_in_correct_sections() {
        let card_actions = vec![presentation(
            "action.only_in_card",
            "persistent",
            "mutable",
            Some(state("known", "オン")),
        )];
        let card_snapshot = build_settings_snapshot(
            &card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        );
        let card_json = serde_json::to_string(&card_snapshot).unwrap();

        let current_actions = vec![presentation(
            "action.only_in_current",
            "persistent",
            "mutable",
            Some(state("known", "オフ")),
        )];

        let report = inspect_custom_card(&card_json, &current_actions, Some(26_100)).unwrap();

        assert_eq!(report.missing_in_current.len(), 1);
        assert_eq!(
            report.missing_in_current[0].action_id,
            "action.only_in_card"
        );
        assert_eq!(report.missing_in_card.len(), 1);
        assert_eq!(
            report.missing_in_card[0].action_id,
            "action.only_in_current"
        );
        assert!(report.matching.is_empty());
        assert!(report.changed.is_empty());
        assert!(report.unknown.is_empty());
    }

    #[test]
    fn uncomparable_items_never_mix_into_matching() {
        let card_actions = vec![presentation(
            "action.undetected",
            "persistent",
            "mutable",
            None,
        )];
        let card_snapshot = build_settings_snapshot(
            &card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        );
        let card_json = serde_json::to_string(&card_snapshot).unwrap();

        let current_actions = vec![presentation(
            "action.undetected",
            "persistent",
            "mutable",
            Some(state("known", "オン")),
        )];

        let report = inspect_custom_card(&card_json, &current_actions, Some(26_100)).unwrap();

        assert_eq!(report.unknown.len(), 1);
        assert_eq!(report.unknown[0].action_id, "action.undetected");
        assert!(report.matching.is_empty());
        assert!(report.changed.is_empty());
    }

    #[test]
    fn comparison_uses_structured_state_key_and_never_the_label() {
        let card_actions = vec![
            presentation(
                "action.same_label",
                "persistent",
                "mutable",
                Some(state_with_key("known", "同じ要約", "structured-value-a")),
            ),
            presentation(
                "action.different_label",
                "persistent",
                "mutable",
                Some(state_with_key("known", "古い表示", "structured-value-c")),
            ),
        ];
        let card_json = serde_json::to_string(&build_settings_snapshot(
            &card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        ))
        .unwrap();
        let current_actions = vec![
            presentation(
                "action.same_label",
                "persistent",
                "mutable",
                Some(state_with_key("known", "同じ要約", "structured-value-b")),
            ),
            presentation(
                "action.different_label",
                "persistent",
                "mutable",
                Some(state_with_key("known", "新しい表示", "structured-value-c")),
            ),
        ];

        let report = inspect_custom_card(&card_json, &current_actions, Some(26_100)).unwrap();

        assert_eq!(report.changed.len(), 1);
        assert_eq!(report.changed[0].action_id, "action.same_label");
        assert_eq!(report.matching.len(), 1);
        assert_eq!(report.matching[0].action_id, "action.different_label");
    }

    #[test]
    fn unsupported_policy_managed_and_error_are_not_comparable() {
        let kinds = ["unsupported", "policy_managed", "error"];
        let unavailable_actions: Vec<_> = kinds
            .iter()
            .map(|kind| {
                presentation(
                    &format!("action.{kind}"),
                    "persistent",
                    "mutable",
                    Some(state_with_key(kind, "同じ表示", "forged-comparison-key")),
                )
            })
            .collect();
        let card_json = serde_json::to_string(&build_settings_snapshot(
            &unavailable_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        ))
        .unwrap();
        let current_actions: Vec<_> = kinds
            .iter()
            .map(|kind| {
                presentation(
                    &format!("action.{kind}"),
                    "persistent",
                    "mutable",
                    Some(state_with_key("known", "同じ表示", "forged-comparison-key")),
                )
            })
            .collect();

        let report = inspect_custom_card(&card_json, &current_actions, Some(26_100)).unwrap();

        assert_eq!(report.unknown.len(), kinds.len());
        assert!(report.matching.is_empty());
        assert!(report.changed.is_empty());

        let known_card_actions: Vec<_> = kinds
            .iter()
            .map(|kind| {
                presentation(
                    &format!("action.{kind}"),
                    "persistent",
                    "mutable",
                    Some(state_with_key("known", "同じ表示", "forged-comparison-key")),
                )
            })
            .collect();
        let known_card_json = serde_json::to_string(&build_settings_snapshot(
            &known_card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        ))
        .unwrap();
        let reverse_report =
            inspect_custom_card(&known_card_json, &unavailable_actions, Some(26_100)).unwrap();

        assert_eq!(reverse_report.unknown.len(), kinds.len());
        assert!(reverse_report.matching.is_empty());
        assert!(reverse_report.changed.is_empty());
    }

    #[test]
    fn duplicate_card_action_ids_are_rejected() {
        let actions = vec![presentation(
            "action.duplicate",
            "persistent",
            "mutable",
            Some(state("known", "オン")),
        )];
        let mut snapshot =
            build_settings_snapshot(&actions, Some(26_100), "2026-08-01T12:00:00Z".to_owned());
        snapshot.entries.push(snapshot.entries[0].clone());
        snapshot.entry_count = snapshot.entries.len();
        let card_json = serde_json::to_string(&snapshot).unwrap();

        let error = inspect_custom_card(&card_json, &actions, Some(26_100))
            .expect_err("duplicate action IDs must be rejected");

        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("同じ項目"));
    }

    #[test]
    fn unknown_card_version_is_rejected() {
        let actions = vec![presentation(
            "action.versioned",
            "persistent",
            "mutable",
            Some(state("known", "オン")),
        )];
        let mut snapshot =
            build_settings_snapshot(&actions, Some(26_100), "2026-08-01T12:00:00Z".to_owned());
        snapshot.version = SETTINGS_SNAPSHOT_VERSION + 1;
        let card_json = serde_json::to_string(&snapshot).unwrap();

        let error = inspect_custom_card(&card_json, &actions, Some(26_100))
            .expect_err("unknown card versions must be rejected");

        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("版"));
    }

    #[test]
    fn cause_is_never_asserted_in_messages() {
        let card_actions = vec![presentation(
            "action.changed",
            "persistent",
            "mutable",
            Some(state("known", "オン")),
        )];
        let card_snapshot = build_settings_snapshot(
            &card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        );
        let card_json = serde_json::to_string(&card_snapshot).unwrap();

        let current_actions = vec![presentation(
            "action.changed",
            "persistent",
            "mutable",
            Some(state("known", "オフ")),
        )];

        let report = inspect_custom_card(&card_json, &current_actions, Some(26_200)).unwrap();

        assert_eq!(report.changed.len(), 1);
        let item = &report.changed[0];
        assert!(!item.note.contains("Update"));
        assert!(!item.note.contains("更新"));
        assert!(!item.note.contains("原因"));
        assert!(!item.note.contains("せい"));
        assert!(!item.note.contains("理由"));

        if let Some(os_note) = &report.os_build_note {
            assert!(!os_note.contains("原因"));
            assert!(!os_note.contains("せい"));
            assert!(!os_note.contains("理由"));
            assert!(os_note.contains("異なります"));
        }

        assert!(!report.summary.contains("原因"));
        assert!(!report.summary.contains("せい"));
    }

    #[test]
    fn invalid_json_and_unknown_action_ids_do_not_panic() {
        // 壊れたJSON
        let bad_json = "{ invalid json }";
        let res = inspect_custom_card(bad_json, &[], None);
        assert!(res.is_err());

        // 知らない項目ID入りのカードJSON
        let raw_json = r#"{
            "version": 1,
            "capturedAt": "2026-08-01T12:00:00Z",
            "osBuild": 26100,
            "entryCount": 1,
            "entries": [{
                "actionId": "unknown.future_action_id",
                "name": "将来の機能",
                "category": "appearance",
                "kind": "persistent",
                "availability": "mutable",
                "stateKind": "known",
                "stateLabel": "有効",
                "stateDetail": "詳細"
            }]
        }"#;

        let report_res = inspect_custom_card(raw_json, &[], None);
        assert!(report_res.is_ok());
        let report = report_res.unwrap();
        assert_eq!(report.missing_in_current.len(), 1);
        assert_eq!(
            report.missing_in_current[0].action_id,
            "unknown.future_action_id"
        );
    }

    #[test]
    fn inspecting_card_is_read_only_and_never_modifies_windows() {
        let card_actions = vec![presentation(
            "explorer.show_extensions",
            "persistent",
            "mutable",
            Some(state("known", "表示")),
        )];
        let card_snapshot = build_settings_snapshot(
            &card_actions,
            Some(26_100),
            "2026-08-01T12:00:00Z".to_owned(),
        );
        let card_json = serde_json::to_string(&card_snapshot).unwrap();

        let current_actions = vec![presentation(
            "explorer.show_extensions",
            "persistent",
            "mutable",
            Some(state("known", "非表示")),
        )];

        // 照合を実行
        let report = inspect_custom_card(&card_json, &current_actions, Some(26_100)).unwrap();

        // 照合結果が正しく生成されること
        assert_eq!(report.changed.len(), 1);

        // inspect_custom_card は純関数であり、I/Oを一切行わずメモリ上でのみ処理されるため、
        // current_actions の値や Windows 状態は変更されない。
        assert_eq!(
            current_actions[0].current_state.as_ref().unwrap().label,
            "非表示"
        );
    }
}
