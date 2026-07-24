//! ゲームプロファイルの背景監視スレッド。
//!
//! 有効プロファイルの同期、プロセス列挙、状態機械 tick を一定間隔で行う。
//! 監視エラーは共有 health に保持して UI と変更ゲートへ伝え、スレッド終了時は
//! 必ず空の定義を同期して、監視中に適用した設定の復元を試みる。

use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::engine::TotonoeEngine;
use crate::error::{CoreError, CoreResult};

use super::engine_sink::EngineProfileSink;
use super::store::ProfileStore;
use super::{
    EventOutcome, ExitOutcome, InstanceKey, ObservedProcess, ProfileError, ProfileRuntime,
};

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const STOP_CHECK: Duration = Duration::from_millis(200);

#[derive(Clone)]
struct WatcherHealth {
    state: Arc<Mutex<WatcherHealthState>>,
}

#[derive(Default)]
struct WatcherHealthState {
    issue: Option<WatcherIssue>,
}

struct WatcherIssue {
    error: CoreError,
    sticky: bool,
}

impl WatcherHealth {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(WatcherHealthState::default())),
        }
    }

    fn record(&self, error: CoreError, sticky: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.issue.as_ref().is_some_and(|issue| issue.sticky) && !sticky {
            return;
        }
        state.issue = Some(WatcherIssue { error, sticky });
    }

    fn clear_transient(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.issue.as_ref().is_some_and(|issue| !issue.sticky) {
            state.issue = None;
        }
    }

    fn has_sticky_issue(&self) -> bool {
        match self.state.lock() {
            Ok(state) => state.issue.as_ref().is_some_and(|issue| issue.sticky),
            Err(_) => true,
        }
    }

    fn error(&self) -> Option<CoreError> {
        match self.state.lock() {
            Ok(state) => state.issue.as_ref().map(|issue| issue.error.clone()),
            Err(_) => Some(CoreError::recovery_required(
                "ゲームプロファイル監視の安全状態を確認できないため、変更操作を停止しています。",
            )),
        }
    }
}

pub struct ProfileWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<CoreResult<()>>>,
    health: WatcherHealth,
}

impl ProfileWatcher {
    pub fn spawn(
        engine: Arc<TotonoeEngine>,
        store: Arc<ProfileStore>,
        theme_schedule: Option<Arc<crate::theme_schedule::ThemeScheduleStore>>,
    ) -> CoreResult<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let health = WatcherHealth::new();
        let stop_thread = stop.clone();
        let health_thread = health.clone();
        let handle = thread::Builder::new()
            .name("totonoe-profile-watcher".to_owned())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_loop(engine, store, theme_schedule, stop_thread, &health_thread)
                }))
                .unwrap_or_else(|_| {
                    Err(CoreError::recovery_required(
                        "ゲームプロファイル監視が予期せず停止したため、変更操作を停止しています。",
                    ))
                });
                if let Err(error) = &result {
                    health_thread.record(error.clone(), true);
                }
                result
            })
            .map_err(|_| {
                CoreError::new(
                    "PROFILE_WATCHER_START_FAILED",
                    "PROFILE_WATCHER",
                    false,
                    "ゲームプロファイル監視を開始できないため、変更操作を停止しています。",
                )
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
            health,
        })
    }

    /// 直近の監視異常。transient な列挙異常も正常な完全 snapshot まで保持する。
    pub fn health_error(&self) -> Option<CoreError> {
        self.health.error()
    }

    /// 監視を停止し、runtime.sync(&[]) による復元結果を呼び側へ返す。
    pub fn shutdown(&mut self) -> CoreResult<()> {
        self.stop.store(true, Ordering::SeqCst);
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        match handle.join() {
            Ok(result) => result,
            Err(_) => {
                let error = CoreError::recovery_required(
                    "ゲームプロファイル監視の終了を確認できないため、復旧確認が必要です。",
                );
                self.health.record(error.clone(), true);
                Err(error)
            }
        }
    }
}

impl Drop for ProfileWatcher {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            // Drop は Result を返せないため、共有 health に加えて診断 ID を stderr へ残す。
            eprintln!("profile watcher shutdown failed: {error}");
        }
    }
}

fn run_loop(
    engine: Arc<TotonoeEngine>,
    store: Arc<ProfileStore>,
    theme_schedule: Option<Arc<crate::theme_schedule::ThemeScheduleStore>>,
    stop: Arc<AtomicBool>,
    health: &WatcherHealth,
) -> CoreResult<()> {
    let theme_engine = engine.clone();
    let sink = EngineProfileSink::new(engine);
    let mut runtime = ProfileRuntime::new(sink);
    let mut theme_tracker = crate::theme_schedule::ThemeScheduleTracker::default();

    while !stop.load(Ordering::SeqCst) {
        run_cycle(&mut runtime, &store, health);
        if let Some(theme_store) = theme_schedule.as_ref() {
            apply_theme_schedule_if_due(&theme_engine, theme_store, &mut theme_tracker);
        }

        let ticks = (POLL_INTERVAL.as_millis() / STOP_CHECK.as_millis()).max(1);
        for _ in 0..ticks {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(STOP_CHECK);
        }
    }

    // 終了境界: active instance を終了扱いにして、適用済み resource を先に復元する。
    let failures = runtime.sync(&[]);
    if let Some((error, _)) = sync_failures_error(&failures) {
        health.record(error.clone(), true);
        return Err(error);
    }
    Ok(())
}

/// 時間帯によるライト/ダーク自動切り替え。
/// 境界をまたいだ時だけ、検証済みの `theme.color_mode` を preview→commit で適用する。
/// 失敗は握り潰さず store へ記録し、UIから見えるようにする。
fn apply_theme_schedule_if_due(
    engine: &TotonoeEngine,
    store: &crate::theme_schedule::ThemeScheduleStore,
    tracker: &mut crate::theme_schedule::ThemeScheduleTracker,
) {
    use chrono::Timelike;

    let schedule = store.get();
    let now = chrono::Local::now();
    let now_minutes = now.hour() * 60 + now.minute();
    let Some(mode) = tracker.evaluate(&schedule, now_minutes) else {
        return;
    };

    let mut parameters = serde_json::Map::new();
    parameters.insert(
        "mode".to_owned(),
        serde_json::Value::String(mode.as_parameter().to_owned()),
    );
    let request = crate::presentation::PreviewActionsRequest {
        actions: vec![crate::presentation::PreviewActionRequest {
            action_id: "theme.color_mode".to_owned(),
            parameters,
        }],
    };

    match engine.preview(request) {
        Ok(preview) => {
            // 既に望ましい表示になっている場合は、履歴を増やさず何もしない。
            if preview
                .changes
                .iter()
                .all(|change| change.before == change.after)
            {
                store.record_apply_result(None);
                return;
            }
            match engine.commit_preview(&preview.preview_token) {
                Ok(_) => store.record_apply_result(None),
                Err(error) => store.record_apply_result(Some(error.user_message)),
            }
        }
        Err(error) => store.record_apply_result(Some(error.user_message)),
    }
}

fn run_cycle(
    runtime: &mut ProfileRuntime<EngineProfileSink>,
    store: &ProfileStore,
    health: &WatcherHealth,
) {
    let profiles = store.list();
    let sync_failures = runtime.sync(&profiles);
    if let Some((error, sticky)) = sync_failures_error(&sync_failures) {
        health.record(error, sticky);
    }

    // runtime の適用/復元失敗後は状態を推測して新しい launch を処理しない。
    // store の同期だけは続けるため、ユーザーは自動適用を無効化・削除できる。
    if health.has_sticky_issue() {
        return;
    }

    if !runtime.has_targets() {
        if sync_failures.is_empty() {
            health.clear_transient();
        }
        return;
    }

    let snapshot = match current_processes() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            // 全体列挙に失敗しても、既知 instance は handle + creation time で個別確認する。
            let candidates = runtime.tracked_instances();
            let (confirmed_exits, _) = confirm_exits(&candidates);
            let tick_result = runtime.tick_incomplete(&[], &confirmed_exits);
            if !record_tick_result(tick_result, health) {
                return;
            }
            health.record(error, false);
            return;
        }
    };

    let (tick_result, unknown_exit_count) = if snapshot.complete {
        (runtime.tick(&snapshot.processes), 0)
    } else {
        let candidates = runtime.missing_instances(&snapshot.processes);
        let (confirmed_exits, unknown_count) = confirm_exits(&candidates);
        (
            runtime.tick_incomplete(&snapshot.processes, &confirmed_exits),
            unknown_count,
        )
    };
    if !record_tick_result(tick_result, health) {
        return;
    }

    if unknown_exit_count > 0 {
        health.record(
            CoreError::new(
                "PROFILE_PROCESS_EXIT_UNKNOWN",
                "PROFILE_WATCHER",
                true,
                format!(
                    "実行状態を確認できないゲームプロセスが{}件あるため、終了判定を保留しています。",
                    unknown_exit_count
                ),
            ),
            false,
        );
    } else if sync_failures.is_empty() {
        health.clear_transient();
    }
}

fn record_tick_result(
    tick_result: Result<Vec<EventOutcome>, ProfileError>,
    health: &WatcherHealth,
) -> bool {
    match tick_result {
        Err(error) => {
            health.record(runtime_error(error), true);
            false
        }
        Ok(outcomes) if has_partial_rollback(&outcomes) => {
            health.record(
                CoreError::recovery_required(
                    "ゲームプロファイルの一部設定を復元できませんでした。変更記録からの復旧が必要です。",
                ),
                true,
            );
            false
        }
        Ok(_) => true,
    }
}

fn confirm_exits(candidates: &BTreeSet<InstanceKey>) -> (BTreeSet<InstanceKey>, usize) {
    let mut confirmed = BTreeSet::new();
    let mut unknown = 0usize;
    for instance in candidates {
        match crate::windows::process_instance_status(
            instance.process_id,
            instance.creation_time_100ns,
        ) {
            Ok(crate::windows::ProcessInstanceStatus::Exited) => {
                confirmed.insert(*instance);
            }
            Ok(crate::windows::ProcessInstanceStatus::Running) => {}
            Err(_) => unknown += 1,
        }
    }
    (confirmed, unknown)
}

fn sync_failures_error(failures: &[(String, CoreError)]) -> Option<(CoreError, bool)> {
    if failures.is_empty() {
        return None;
    }
    let recovery_required = failures
        .iter()
        .any(|(_, error)| error.code == "RECOVERY_REQUIRED");
    let message = format!(
        "ゲームプロファイル監視で{}件の定義または復元エラーを検出しました。自動適用を無効化し、変更記録を確認してください。",
        failures.len()
    );
    let error = if recovery_required {
        CoreError::recovery_required(message)
    } else {
        CoreError::new(
            "PROFILE_DEFINITION_INVALID",
            "PROFILE_WATCHER",
            false,
            message,
        )
    };
    Some((error, recovery_required))
}

fn has_partial_rollback(outcomes: &[EventOutcome]) -> bool {
    outcomes.iter().any(|outcome| {
        matches!(
            outcome,
            EventOutcome::Exit(ExitOutcome::PartiallyFailed { .. })
        )
    })
}

fn runtime_error(_error: ProfileError) -> CoreError {
    CoreError::recovery_required(
        "ゲームプロファイルの適用または復元を完了できませんでした。自動適用を無効化し、変更記録を確認してください。",
    )
}

struct ObservedSnapshot {
    processes: Vec<ObservedProcess>,
    complete: bool,
}

fn current_processes() -> CoreResult<ObservedSnapshot> {
    let report = crate::windows::snapshot_process_identities().map_err(|_| {
        CoreError::new(
            "PROFILE_PROCESS_SNAPSHOT_FAILED",
            "PROFILE_WATCHER",
            true,
            "実行中プロセスを完全に確認できないため、ゲームプロファイルの終了判定を保留しています。",
        )
    })?;
    let complete = snapshot_is_complete(&report);
    let processes = report
        .processes
        .into_iter()
        .map(|process| ObservedProcess {
            instance: InstanceKey {
                process_id: process.process_id,
                creation_time_100ns: process.creation_time_100ns,
            },
            canonical_path: process.canonical_path,
            file_identity: Some(process.file_identity),
        })
        .collect();
    Ok(ObservedSnapshot {
        processes,
        complete,
    })
}

fn snapshot_is_complete(report: &crate::windows::ProcessSnapshotReport) -> bool {
    report.inaccessible_or_vanished == 0
        && report.wmi_available
        && report.wmi_failure_operation.is_none()
        && report.wmi_failure_code.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_error(code: &str) -> CoreError {
        CoreError::new(code, "PROFILE_WATCHER", false, "test")
    }

    #[test]
    fn transient_health_clears_but_sticky_runtime_health_does_not() {
        let health = WatcherHealth::new();
        health.record(test_error("SNAPSHOT"), false);
        assert_eq!(health.error().expect("transient").code, "SNAPSHOT");
        health.clear_transient();
        assert!(health.error().is_none());

        health.record(test_error("RUNTIME"), true);
        health.record(test_error("SNAPSHOT"), false);
        health.clear_transient();
        assert_eq!(health.error().expect("sticky").code, "RUNTIME");
        assert!(health.has_sticky_issue());
    }

    #[test]
    fn sync_failures_are_aggregated_and_recovery_is_sticky() {
        let failures = vec![
            ("one".to_owned(), test_error("INVALID_REQUEST")),
            (
                "two".to_owned(),
                CoreError::recovery_required("rollback failed"),
            ),
        ];
        let (error, sticky) = sync_failures_error(&failures).expect("aggregate");
        assert_eq!(error.code, "RECOVERY_REQUIRED");
        assert!(error.user_message.contains("2件"));
        assert!(sticky);
    }

    #[test]
    fn any_identity_gap_or_wmi_failure_marks_snapshot_incomplete() {
        let mut report = crate::windows::ProcessSnapshotReport {
            processes: Vec::new(),
            inaccessible_or_vanished: 0,
            inaccessible_executable_names: Vec::new(),
            first_identity_error_code: None,
            wmi_available: true,
            wmi_failure_operation: None,
            wmi_failure_code: None,
        };
        assert!(snapshot_is_complete(&report));

        report.inaccessible_or_vanished = 1;
        assert!(!snapshot_is_complete(&report));
        report.inaccessible_or_vanished = 0;
        report.wmi_available = false;
        report.wmi_failure_operation = Some("WMI query");
        assert!(!snapshot_is_complete(&report));
    }

    #[cfg(windows)]
    #[test]
    fn real_thread_spawn_and_graceful_shutdown_report_success() {
        use crate::compatibility::OsIdentity;
        use crate::journal::JournalDatabase;

        let temp = tempfile::tempdir().expect("tempdir");
        let store =
            Arc::new(ProfileStore::open(temp.path().join("profiles.json")).expect("profile store"));
        let journal = Arc::new(JournalDatabase::open_in_memory().expect("journal"));
        let engine = Arc::new(
            TotonoeEngine::new(journal, Some(OsIdentity::from_test_build(26_200))).expect("engine"),
        );
        let mut watcher = ProfileWatcher::spawn(engine, store, None).expect("spawn watcher");
        watcher.shutdown().expect("graceful shutdown");
        assert!(watcher.health_error().is_none());
    }
}
