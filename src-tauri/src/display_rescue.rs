//! Read-only detection for restoring a saved layout after display changes.
//!
//! This module never moves a window. A restore still has to travel through the
//! ordinary explicit preview -> commit -> journaled WindowLayout Action.

use serde::Serialize;

use crate::{
    display_profile::{compare_topology, DisplayProfile},
    window_layout::{
        WindowLayoutExclusion, WindowLayoutInspection, WindowLayoutIssue, WindowLayoutIssueReason,
        WindowLayoutSnapshot,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayRescueState {
    NoSavedLayout,
    SavedDisplayUnknown,
    DisplayTopologyChanged,
    Stable,
    RescueAvailable,
    TargetsUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRescueReport {
    pub state: DisplayRescueState,
    pub can_restore: bool,
    pub saved_window_count: u32,
    pub matched_window_count: u32,
    pub drifted_window_count: u32,
    pub message: String,
    pub exclusions: Vec<WindowLayoutExclusion>,
    pub unavailable_targets: Vec<WindowLayoutIssue>,
}

impl DisplayRescueReport {
    pub fn no_saved_layout() -> Self {
        Self {
            state: DisplayRescueState::NoSavedLayout,
            can_restore: false,
            saved_window_count: 0,
            matched_window_count: 0,
            drifted_window_count: 0,
            message: "戻す先がありません。先に正しいウィンドウ配置を保存してください。".to_owned(),
            exclusions: Vec::new(),
            unavailable_targets: Vec::new(),
        }
    }

    pub fn saved_display_unknown(snapshot: &WindowLayoutSnapshot) -> Self {
        Self {
            state: DisplayRescueState::SavedDisplayUnknown,
            can_restore: false,
            saved_window_count: bounded_count(snapshot.entries.len()),
            matched_window_count: 0,
            drifted_window_count: 0,
            message:
                "保存時の表示構成が記録されていません。現在の正しい配置を保存し直してください。"
                    .to_owned(),
            exclusions: snapshot.exclusions.clone(),
            unavailable_targets: Vec::new(),
        }
    }

    pub fn topology_changed(snapshot: &WindowLayoutSnapshot, reason: String) -> Self {
        Self {
            state: DisplayRescueState::DisplayTopologyChanged,
            can_restore: false,
            saved_window_count: bounded_count(snapshot.entries.len()),
            matched_window_count: 0,
            drifted_window_count: 0,
            message: format!(
                "保存時と表示構成が違うため、ウィンドウは動かしません。{reason} 元の画面構成へ戻してから確認してください。"
            ),
            exclusions: snapshot.exclusions.clone(),
            unavailable_targets: Vec::new(),
        }
    }
}

pub fn build_report(
    snapshot: &WindowLayoutSnapshot,
    current_display: &DisplayProfile,
    inspection: WindowLayoutInspection,
) -> DisplayRescueReport {
    let Some(saved_display) = snapshot.display_profile.as_ref() else {
        return DisplayRescueReport::saved_display_unknown(snapshot);
    };
    if let Err(reason) = compare_topology(saved_display, current_display) {
        return DisplayRescueReport::topology_changed(snapshot, reason);
    }

    let observation = inspection.observation;
    let drifted = observation
        .matched_window_count
        .saturating_sub(observation.positioned_window_count);
    let (state, message) = if drifted > 0 {
        (
            DisplayRescueState::RescueAvailable,
            format!(
                "保存時から配置が変わったウィンドウを{drifted}個検出しました。自動では動かしません。内容を確認してから復元できます。"
            ),
        )
    } else if observation.matched_window_count == 0 {
        (
            DisplayRescueState::TargetsUnavailable,
            "保存済みの対象ウィンドウを現在のセッションで確認できません。何も動かしません。"
                .to_owned(),
        )
    } else {
        (
            DisplayRescueState::Stable,
            "保存済みの対象ウィンドウに配置の差はありません。".to_owned(),
        )
    };

    let mut exclusions = snapshot.exclusions.clone();
    for exclusion in inspection.exclusions {
        if !exclusions.contains(&exclusion) {
            exclusions.push(exclusion);
        }
    }
    DisplayRescueReport {
        state,
        can_restore: drifted > 0,
        saved_window_count: observation.saved_window_count,
        matched_window_count: observation.matched_window_count,
        drifted_window_count: drifted,
        message,
        exclusions,
        unavailable_targets: observation
            .issues
            .into_iter()
            .filter(|issue| {
                !matches!(
                    issue.reason,
                    WindowLayoutIssueReason::ExternalChange
                        | WindowLayoutIssueReason::VerificationMismatch
                )
            })
            .collect(),
    }
}

fn bounded_count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        display_profile::DisplayPathFacts,
        window_layout::{WindowLayoutIssueReason, WindowLayoutObservation},
    };

    fn profile(source_x: i32) -> DisplayProfile {
        DisplayProfile {
            paths: vec![DisplayPathFacts {
                target_id: 1,
                source_id: 1,
                clone_group: 0,
                adapter_id_low: 1,
                adapter_id_high: 0,
                source_x,
                source_y: 0,
                width: 1920,
                height: 1080,
                pixel_format: 1,
                rotation: 1,
                scaling: 1,
                output_technology: 5,
                refresh_numerator: 60,
                refresh_denominator: 1,
                boost_refresh_rate: false,
            }],
        }
    }

    fn inspection(positioned: u32, issues: Vec<WindowLayoutIssue>) -> WindowLayoutInspection {
        WindowLayoutInspection {
            observation: WindowLayoutObservation {
                saved_window_count: 1,
                matched_window_count: 1,
                positioned_window_count: positioned,
                issues,
                geometry_fingerprint: "geometry".to_owned(),
            },
            exclusions: Vec::new(),
        }
    }

    fn snapshot(saved_display: DisplayProfile) -> WindowLayoutSnapshot {
        // Report construction needs only the bounded public counts and display profile.
        WindowLayoutSnapshot {
            snapshot_id: uuid::Uuid::new_v4(),
            captured_at_unix_ms: 1,
            display_profile: Some(saved_display),
            entries: Vec::new(),
            excluded_game_windows: 0,
            skipped_windows: 0,
            exclusions: Vec::new(),
        }
    }

    #[test]
    fn a_different_topology_blocks_restore_before_geometry_is_used() {
        let report = build_report(
            &snapshot(profile(0)),
            &profile(1920),
            inspection(0, Vec::new()),
        );
        assert_eq!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert!(!report.can_restore);
        assert_eq!(report.drifted_window_count, 0);
    }

    #[test]
    fn a_real_saved_current_difference_offers_only_explicit_restore() {
        let issue = WindowLayoutIssue {
            target: "owned-test.exe".to_owned(),
            reason: WindowLayoutIssueReason::VerificationMismatch,
        };
        let report = build_report(
            &snapshot(profile(0)),
            &profile(0),
            inspection(0, vec![issue]),
        );
        assert_eq!(report.state, DisplayRescueState::RescueAvailable);
        assert!(report.can_restore);
        assert_eq!(report.drifted_window_count, 1);
        assert!(report.unavailable_targets.is_empty());
    }

    fn path_facts(target_id: u32, source_x: i32, width: u32, height: u32) -> DisplayPathFacts {
        DisplayPathFacts {
            target_id,
            source_id: target_id,
            clone_group: 0,
            adapter_id_low: 1,
            adapter_id_high: 0,
            source_x,
            source_y: 0,
            width,
            height,
            pixel_format: 1,
            rotation: 1,
            scaling: 1,
            output_technology: 5,
            refresh_numerator: 60,
            refresh_denominator: 1,
            boost_refresh_rate: false,
        }
    }

    #[test]
    fn saved_two_displays_current_one_display_detected_as_topology_changed() {
        let saved = DisplayProfile {
            paths: vec![
                path_facts(1, 0, 1920, 1080),
                path_facts(2, 1920, 1920, 1080),
            ],
        };
        let current = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let report = build_report(&snapshot(saved), &current, inspection(0, Vec::new()));
        assert_eq!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert!(!report.can_restore);
        assert_eq!(report.drifted_window_count, 0);
    }

    #[test]
    fn saved_one_display_current_two_displays_detected_as_topology_changed() {
        let saved = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let current = DisplayProfile {
            paths: vec![
                path_facts(1, 0, 1920, 1080),
                path_facts(2, 1920, 1920, 1080),
            ],
        };
        let report = build_report(&snapshot(saved), &current, inspection(0, Vec::new()));
        assert_eq!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert!(!report.can_restore);
        assert_eq!(report.drifted_window_count, 0);
    }

    #[test]
    fn same_display_count_different_resolution_detected_as_topology_changed() {
        let saved = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let current = DisplayProfile {
            paths: vec![path_facts(1, 0, 2560, 1440)],
        };
        let report = build_report(&snapshot(saved), &current, inspection(0, Vec::new()));
        assert_eq!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert!(!report.can_restore);
        assert_eq!(report.drifted_window_count, 0);
    }

    #[test]
    fn same_display_count_different_position_detected_as_topology_changed() {
        let saved = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let current = DisplayProfile {
            paths: vec![path_facts(1, 1920, 1920, 1080)],
        };
        let report = build_report(&snapshot(saved), &current, inspection(0, Vec::new()));
        assert_eq!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert!(!report.can_restore);
        assert_eq!(report.drifted_window_count, 0);
    }

    #[test]
    fn identical_topology_does_not_say_topology_changed() {
        let saved = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let current = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let report = build_report(&snapshot(saved), &current, inspection(1, Vec::new()));
        assert_ne!(report.state, DisplayRescueState::DisplayTopologyChanged);
        assert_eq!(report.state, DisplayRescueState::Stable);
        assert!(!report.can_restore);
    }

    #[test]
    fn saved_window_not_in_any_work_area_handled_safely_when_unmatched() {
        let saved = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let current = DisplayProfile {
            paths: vec![path_facts(1, 0, 1920, 1080)],
        };
        let issue = WindowLayoutIssue {
            target: "unmatched-app.exe".to_owned(),
            reason: WindowLayoutIssueReason::NotRunning,
        };
        let mut insp = inspection(0, vec![issue.clone()]);
        insp.observation.matched_window_count = 0;

        let report = build_report(&snapshot(saved), &current, insp);
        assert_eq!(report.state, DisplayRescueState::TargetsUnavailable);
        assert!(!report.can_restore);
        assert_eq!(report.unavailable_targets.len(), 1);
        assert_eq!(
            report.unavailable_targets[0].reason,
            WindowLayoutIssueReason::NotRunning
        );
    }
}
