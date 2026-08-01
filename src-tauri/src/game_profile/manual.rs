//! Explicit execution and restoration for profiles without an executable binding.

use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::{
    engine::PcCustomEngine,
    error::{CoreError, CoreResult},
    journal::TimelineItem,
    presentation::{PreviewActionRequest, PreviewActionsRequest},
};

use super::{ManualRunRecord, ProfileStore};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualProfileResult {
    pub transaction_id: String,
    pub status: String,
    pub reversible_item_count: usize,
    pub message: String,
    pub details: Vec<String>,
}

pub fn run_manual_profile(
    engine: Arc<PcCustomEngine>,
    store: Arc<ProfileStore>,
    id: &str,
) -> CoreResult<ManualProfileResult> {
    let profile = store.get(id)?;
    if !profile.is_manual() {
        return Err(CoreError::invalid_request(
            "ゲームプロファイルは『いま実行』の対象ではありません。",
        ));
    }
    if profile.active_run.is_some() {
        return Err(CoreError::invalid_request(
            "この手動モードは既に実行中です。先に実行した分を復元してください。",
        ));
    }
    if profile.actions.is_empty() {
        return Err(CoreError::invalid_request("実行するActionがありません。"));
    }

    let actions = profile
        .actions
        .iter()
        .map(|stored| {
            if stored.action_id == crate::action::ActionId::SetupWindowLayout.as_str() {
                if !stored
                    .parameters
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty)
                {
                    return Err(CoreError::invalid_request(
                        "ウィンドウ配置は保存済みデータだけを使います。",
                    ));
                }
                return Ok(PreviewActionRequest {
                    action_id: stored.action_id.clone(),
                    parameters: serde_json::Map::new(),
                });
            }
            let parameters = super::store::parse_stored_profile_action(stored)?;
            let action = crate::action::ACTION_REGISTRY
                .get(parameters.action_id())
                .ok_or_else(|| {
                    CoreError::invalid_request("登録済みActionを解決できませんでした。")
                })?;
            if !matches!(
                action.metadata().kind,
                crate::action::ActionKind::Persistent
                    | crate::action::ActionKind::Session
                    | crate::action::ActionKind::OneWay
            ) {
                return Err(CoreError::invalid_request(
                    "手動モードで実行できないActionが含まれています。",
                ));
            }
            Ok(PreviewActionRequest {
                action_id: stored.action_id.clone(),
                parameters: stored.parameters.as_object().cloned().unwrap_or_default(),
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;

    let preview = engine.preview_with_runtime_parameters(PreviewActionsRequest { actions })?;
    let commit = engine.commit_preview(&preview.preview_token)?;
    if commit.status != "succeeded" {
        return Err(CoreError::recovery_required(commit.message));
    }

    // list_timeline returns transaction items in reverse durable ordinal, which is
    // exactly the rollback order. Persist that order rather than the profile UI order.
    let transaction_items = manual_transaction_items(&engine, commit.transaction_id)?;
    if transaction_items.len() != profile.actions.len() {
        return Err(CoreError::recovery_required(
            "手動モードの変更記録が不足しています。",
        ));
    }
    let reversible_item_ids = transaction_items
        .iter()
        .filter(|item| item.rollback_available)
        .map(|item| item.item_id.to_string())
        .collect::<Vec<_>>();
    if !reversible_item_ids.is_empty() {
        let record = ManualRunRecord {
            transaction_id: commit.transaction_id.to_string(),
            reversible_item_ids: reversible_item_ids.clone(),
        };
        if let Err(persist_error) = store.set_active_run(id, record) {
            let mut rollback_failed = false;
            for item_id in compensation_order(&reversible_item_ids) {
                match Uuid::parse_str(item_id)
                    .ok()
                    .and_then(|item| engine.rollback_item(item).ok())
                {
                    Some(result) if result.status == "rolled_back" => {}
                    _ => rollback_failed = true,
                }
            }
            if rollback_failed {
                return Err(CoreError::recovery_required("手動モードの実行記録を保存できず、一部を復元できませんでした。変更履歴を確認してください。"));
            }
            return Err(persist_error);
        }
    }

    Ok(ManualProfileResult {
        transaction_id: commit.transaction_id.to_string(),
        status: "succeeded".to_owned(),
        reversible_item_count: reversible_item_ids.len(),
        message: if reversible_item_ids.is_empty() {
            "アプリを起動しました。起動したアプリは自動で終了しません。".to_owned()
        } else if profile
            .actions
            .iter()
            .any(|action| action.action_id == crate::action::ActionId::SetupWindowLayout.as_str())
        {
            "一時ワークスペースを始めました。開始時に捕捉した窓と、このアプリが変更した設定だけを『終わる』で戻せます。".to_owned()
        } else {
            "手動モードを実行しました。『実行した分を戻す』で可逆な変更だけを元へ戻せます。起動したアプリは終了しません。".to_owned()
        },
        details: commit.details,
    })
}

pub fn restore_manual_profile(
    engine: Arc<PcCustomEngine>,
    store: Arc<ProfileStore>,
    id: &str,
) -> CoreResult<ManualProfileResult> {
    let profile = store.get(id)?;
    if !profile.is_manual() {
        return Err(CoreError::invalid_request(
            "ゲームプロファイルは手動復元の対象ではありません。",
        ));
    }
    let run = profile.active_run.clone().ok_or_else(|| {
        CoreError::invalid_request("この手動モードに復元待ちの変更はありません。")
    })?;
    let mut remaining = run.reversible_item_ids.clone();
    let restored_count = remaining.len();
    let mut details = Vec::new();
    while let Some(item_id) = remaining.first().cloned() {
        let item_id = Uuid::parse_str(&item_id)
            .map_err(|_| CoreError::recovery_required("手動モードの復元参照が不正です。"))?;
        let result = engine.rollback_item(item_id)?;
        if result.status != "rolled_back" {
            return Err(CoreError::recovery_required(result.message));
        }
        details.extend(result.details);
        remaining.remove(0);
        store.acknowledge_rolled_back_items(id, &run.transaction_id, remaining.clone())?;
    }
    Ok(ManualProfileResult {
        transaction_id: run.transaction_id,
        status: "rolled_back".to_owned(),
        reversible_item_count: restored_count,
        message: if profile
            .actions
            .iter()
            .any(|action| action.action_id == crate::action::ActionId::SetupWindowLayout.as_str())
        {
            "一時ワークスペースを終わりました。窓と設定を戻しました。アプリは閉じていません。"
                .to_owned()
        } else {
            "この手動モードが変更した可逆項目を逆順に元へ戻しました。起動したアプリは終了していません。".to_owned()
        },
        details,
    })
}

fn manual_transaction_items(
    engine: &PcCustomEngine,
    transaction_id: Uuid,
) -> CoreResult<Vec<TimelineItem>> {
    engine.transaction_timeline(transaction_id)
}

fn compensation_order(item_ids: &[String]) -> impl Iterator<Item = &String> {
    item_ids.iter()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{
        action::{ActionId, ActionParameters},
        backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup},
        compatibility::OsIdentity,
        game_profile::{CreateProfileRequest, StoredProfileAction},
        journal::{JournalDatabase, PreparedItem},
        window_layout::{WindowLayoutStore, WindowRect},
    };
    use serde::{Deserialize, Serialize};

    fn observation_items(transaction_id: Uuid, count: usize) -> Vec<PreparedItem> {
        (0..count)
            .map(|ordinal| {
                let item_id = Uuid::new_v4();
                let before = Fingerprint::of_bytes(format!("before-{ordinal}").as_bytes());
                let backup = BackupEnvelope::from_draft(
                    BackupDraft {
                        precondition_fingerprint: before,
                        intended_fingerprint: before,
                        payload: BackupPayload::Observation(ObservationBackup {
                            source: "manual cap regression".to_owned(),
                        }),
                    },
                    transaction_id,
                    item_id,
                    ActionId::PowerActiveSchemeCheck,
                    1,
                    1,
                    26_200,
                );
                PreparedItem {
                    item_id,
                    ordinal: u32::try_from(ordinal).expect("bounded test ordinal"),
                    action_id: ActionId::PowerActiveSchemeCheck,
                    action_version: 1,
                    parameters: ActionParameters::PowerActiveSchemeCheck {},
                    resource_keys: vec![format!("manual-cap-{ordinal}")],
                    backup,
                }
            })
            .collect()
    }

    #[test]
    fn manual_run_reads_all_129_transaction_items_past_the_old_128_cap() {
        let journal = Arc::new(JournalDatabase::open_in_memory().unwrap());
        let transaction_id = Uuid::new_v4();
        let items = observation_items(transaction_id, 129);
        journal
            .record_prepared_transaction(
                transaction_id,
                "manual-cap-regression",
                "test",
                "test-os",
                &items,
                1,
            )
            .unwrap();
        let engine =
            PcCustomEngine::new(journal, Some(OsIdentity::from_test_build(26_200))).unwrap();

        let loaded = manual_transaction_items(&engine, transaction_id).unwrap();
        assert_eq!(loaded.len(), 129);
    }

    #[test]
    fn failed_manual_run_persistence_compensates_in_saved_rollback_order() {
        let ids = vec!["second-applied".to_owned(), "first-applied".to_owned()];
        assert_eq!(
            compensation_order(&ids).cloned().collect::<Vec<_>>(),
            ids,
            "timeline already stores reverse durable ordinal, the rollback order"
        );
    }

    #[test]
    fn manual_profile_uses_engine_journal_and_restores_its_items() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ProfileStore::open(dir.path().join("profiles.json")).unwrap());
        let profile = store
            .create(CreateProfileRequest {
                name: "集中".to_owned(),
                executable_path: None,
                conflict_policy: None,
                ribbon_color: None,
                actions: vec![StoredProfileAction {
                    action_id: "session.prevent_sleep".to_owned(),
                    parameters: serde_json::json!({"keepDisplayOn": false}),
                }],
            })
            .unwrap();
        let journal = Arc::new(JournalDatabase::open_in_memory().unwrap());
        let engine = Arc::new(
            PcCustomEngine::new(journal, Some(OsIdentity::from_test_build(26_200))).unwrap(),
        );

        let run = run_manual_profile(engine.clone(), store.clone(), &profile.id).unwrap();
        assert_eq!(run.reversible_item_count, 1);
        assert!(store.get(&profile.id).unwrap().active_run.is_some());

        let restored = restore_manual_profile(engine, store.clone(), &profile.id).unwrap();
        assert_eq!(restored.status, "rolled_back");
        assert!(store.get(&profile.id).unwrap().active_run.is_none());
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ChildWindowProbe {
        rect: WindowRect,
        show_cmd: u32,
    }

    struct ChildOwnedWindow(windows::Win32::Foundation::HWND);

    impl Drop for ChildOwnedWindow {
        fn drop(&mut self) {
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0) };
        }
    }

    struct ChildWindowThread {
        thread_id: u32,
        handles: Vec<isize>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl ChildWindowThread {
        fn start() -> Self {
            use std::sync::mpsc::sync_channel;
            use windows::Win32::{
                Foundation::{HINSTANCE, HWND},
                System::Threading::GetCurrentThreadId,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DispatchMessageW, GetMessageW, TranslateMessage, HMENU, MSG,
                    WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
                },
            };

            let (ready_sender, ready_receiver) = sync_channel(1);
            let join = std::thread::spawn(move || {
                let suffix = Uuid::new_v4();
                let titles = [
                    windows::core::HSTRING::from(format!("workspace-child-one-{suffix}")),
                    windows::core::HSTRING::from(format!("workspace-child-two-{suffix}")),
                ];
                let positions = [(120, 140), (520, 180)];
                let windows = titles
                    .iter()
                    .zip(positions)
                    .map(|(title, (left, top))| {
                        let handle = unsafe {
                            CreateWindowExW(
                                WINDOW_EX_STYLE::default(),
                                windows::core::w!("STATIC"),
                                title,
                                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                                left,
                                top,
                                360,
                                240,
                                HWND::default(),
                                HMENU::default(),
                                HINSTANCE::default(),
                                None,
                            )
                        }
                        .expect("create separate-process workspace test window");
                        ChildOwnedWindow(handle)
                    })
                    .collect::<Vec<_>>();
                let handles = windows
                    .iter()
                    .map(|window| window.0 .0 as isize)
                    .collect::<Vec<_>>();
                ready_sender
                    .send((unsafe { GetCurrentThreadId() }, handles))
                    .expect("publish child window handles");

                let mut message = MSG::default();
                while unsafe { GetMessageW(&mut message, HWND::default(), 0, 0) }.0 > 0 {
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                drop(windows);
            });
            let (thread_id, handles) = ready_receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("separate-process windows become ready");
            Self {
                thread_id,
                handles,
                join: Some(join),
            }
        }
    }

    impl Drop for ChildWindowThread {
        fn drop(&mut self) {
            use windows::Win32::{
                Foundation::{LPARAM, WPARAM},
                UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT},
            };

            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn move_child_windows(handles: &[isize], positions: &[(i32, i32)]) {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER},
        };

        for (&handle, &(left, top)) in handles.iter().zip(positions) {
            unsafe {
                SetWindowPos(
                    HWND(handle as *mut core::ffi::c_void),
                    HWND::default(),
                    left,
                    top,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .expect("move separate-process workspace window");
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }

    fn read_child_windows(handles: &[isize]) -> Vec<ChildWindowProbe> {
        use windows::Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::{GetWindowPlacement, GetWindowRect, WINDOWPLACEMENT},
        };

        handles
            .iter()
            .map(|&handle| {
                let window = HWND(handle as *mut core::ffi::c_void);
                let mut rect = RECT::default();
                unsafe { GetWindowRect(window, &mut rect) }.expect("child reads its window rect");
                let mut placement = WINDOWPLACEMENT {
                    length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                unsafe { GetWindowPlacement(window, &mut placement) }
                    .expect("child reads its window placement");
                ChildWindowProbe {
                    rect: WindowRect {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    },
                    show_cmd: placement.showCmd,
                }
            })
            .collect()
    }

    #[test]
    #[ignore = "helper process for the separate-process workspace session smoke"]
    fn workspace_window_child_process() {
        if std::env::var_os("PC_CUSTOM_WORKSPACE_CHILD").is_none() {
            return;
        }
        use std::io::{BufRead, Write};

        let windows = ChildWindowThread::start();
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line.expect("read workspace child command");
            match line.as_str() {
                "read" => {}
                "move_before" => {
                    move_child_windows(&windows.handles, &[(460, 360), (900, 390)]);
                }
                "move_external" => {
                    move_child_windows(&windows.handles[0..1], &[(1_180, 650)]);
                }
                "exit" => break,
                _ => panic!("unknown workspace child command"),
            }
            let probes = read_child_windows(&windows.handles);
            writeln!(
                std::io::stderr().lock(),
                "WORKSPACE_JSON:{}",
                serde_json::to_string(&probes).expect("serialize workspace child probe")
            )
            .expect("write workspace child probe");
        }
    }

    struct WorkspaceChildProcess {
        child: std::process::Child,
        stdin: std::process::ChildStdin,
        stderr: std::io::BufReader<std::process::ChildStderr>,
    }

    impl WorkspaceChildProcess {
        fn start() -> Self {
            use std::process::{Command, Stdio};

            let mut child = Command::new(std::env::current_exe().expect("current test executable"))
                .args([
                    "--ignored",
                    "--exact",
                    "game_profile::manual::tests::workspace_window_child_process",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("PC_CUSTOM_WORKSPACE_CHILD", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn separate workspace window process");
            let stdin = child.stdin.take().expect("workspace child stdin");
            let stderr =
                std::io::BufReader::new(child.stderr.take().expect("workspace child stderr"));
            Self {
                child,
                stdin,
                stderr,
            }
        }

        fn process_id(&self) -> u32 {
            self.child.id()
        }

        fn request(&mut self, command: &str) -> Vec<ChildWindowProbe> {
            use std::io::{BufRead, Write};

            writeln!(self.stdin, "{command}").expect("send workspace child command");
            self.stdin.flush().expect("flush workspace child command");
            loop {
                let mut line = String::new();
                let read = self
                    .stderr
                    .read_line(&mut line)
                    .expect("read workspace child output");
                assert_ne!(read, 0, "workspace child exited before replying");
                if let Some(payload) = line.trim().strip_prefix("WORKSPACE_JSON:") {
                    return serde_json::from_str(payload)
                        .expect("parse workspace child coordinates");
                }
            }
        }
    }

    impl Drop for WorkspaceChildProcess {
        fn drop(&mut self) {
            use std::io::Write;
            use std::time::{Duration, Instant};

            let _ = writeln!(self.stdin, "exit");
            let _ = self.stdin.flush();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    _ => {
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        return;
                    }
                }
            }
        }
    }

    fn probe_text(values: &[ChildWindowProbe]) -> String {
        values
            .iter()
            .map(|value| {
                format!(
                    "({},{} {}x{} show={})",
                    value.rect.left,
                    value.rect.top,
                    value.rect.width(),
                    value.rect.height(),
                    value.show_cmd
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    #[ignore = "real-machine separate-process workspace preview, commit, external-change skip, and rollback smoke"]
    fn workspace_session_restores_only_unchanged_separate_process_windows() {
        let directory = tempfile::tempdir().expect("private workspace session directory");
        let profile_store = Arc::new(
            ProfileStore::open(directory.path().join("profiles.json"))
                .expect("workspace profile store"),
        );
        let layout_store = Arc::new(
            WindowLayoutStore::open(directory.path().join("window-layout.json"))
                .expect("workspace layout store"),
        );
        let journal = Arc::new(JournalDatabase::open_in_memory().expect("workspace journal"));
        let mut child = WorkspaceChildProcess::start();

        let desired = child.request("read");
        assert_eq!(desired.len(), 2);
        let snapshot =
            crate::windows::capture_window_layout_for_process_for_test(child.process_id())
                .expect("capture only the separate child process windows");
        assert_eq!(snapshot.entries.len(), 2);
        layout_store
            .replace(snapshot)
            .expect("save desired workspace layout");
        let before = child.request("move_before");
        assert_ne!(before, desired);

        let profile = profile_store
            .create(CreateProfileRequest {
                name: "別プロセス一時ワークスペース".to_owned(),
                executable_path: None,
                conflict_policy: None,
                ribbon_color: None,
                actions: vec![
                    StoredProfileAction {
                        action_id: "setup.window_layout".to_owned(),
                        parameters: serde_json::json!({}),
                    },
                    StoredProfileAction {
                        action_id: "session.prevent_sleep".to_owned(),
                        parameters: serde_json::json!({"keepDisplayOn": false}),
                    },
                ],
            })
            .expect("create workspace profile without adding an Action");
        let engine = Arc::new(
            PcCustomEngine::new_with_runtime_stores(
                journal,
                Some(OsIdentity::load().expect("real Windows identity")),
                Some(profile_store.clone()),
                Some(layout_store),
            )
            .expect("workspace engine"),
        );

        let started = run_manual_profile(engine.clone(), profile_store.clone(), &profile.id)
            .expect("workspace preview and commit");
        assert_eq!(started.status, "succeeded");
        let applied = child.request("read");
        assert_eq!(applied, desired);

        let externally_changed = child.request("move_external");
        assert_ne!(externally_changed[0], applied[0]);
        assert_eq!(externally_changed[1], applied[1]);

        let ended = restore_manual_profile(engine, profile_store.clone(), &profile.id)
            .expect("workspace rollback through journal items");
        assert_eq!(ended.status, "rolled_back");
        assert!(
            ended
                .details
                .iter()
                .any(|detail| detail.contains("外部から移動")),
            "the user-facing result must explain why one window was not overwritten"
        );
        let after = child.request("read");
        assert_eq!(after[0], externally_changed[0]);
        assert_eq!(after[1], before[1]);
        assert!(profile_store
            .get(&profile.id)
            .expect("workspace profile after finish")
            .active_run
            .is_none());

        println!(
            "EVIDENCE: workspace_windows desired=[{}] before_start=[{}] applied=[{}] externally_changed=[{}] after_finish=[{}] details={:?}",
            probe_text(&desired),
            probe_text(&before),
            probe_text(&applied),
            probe_text(&externally_changed),
            probe_text(&after),
            ended.details,
        );
    }

    #[test]
    #[ignore = "real-machine screen-sharing session probes and rollback smoke"]
    fn share_session_measures_each_item_without_combining_unmeasured_results() {
        let directory = tempfile::tempdir().expect("private screen-sharing session directory");
        let profile_store = Arc::new(
            ProfileStore::open(directory.path().join("profiles.json"))
                .expect("screen-sharing profile store"),
        );
        let layout_store = Arc::new(
            WindowLayoutStore::open(directory.path().join("window-layout.json"))
                .expect("screen-sharing layout store"),
        );
        let share_store = Arc::new(
            crate::share_session::ShareSessionStore::open(
                directory.path().join("share-session.json"),
            )
            .expect("screen-sharing session store"),
        );
        let journal = Arc::new(JournalDatabase::open_in_memory().expect("screen-sharing journal"));
        let mut child = WorkspaceChildProcess::start();

        let desired = child.request("read");
        assert_eq!(desired.len(), 2);
        let snapshot =
            crate::windows::capture_window_layout_for_process_for_test(child.process_id())
                .expect("capture screen-sharing child windows");
        layout_store
            .replace(snapshot)
            .expect("save screen-sharing layout");
        let before = child.request("move_before");
        assert_ne!(before, desired);

        let engine = Arc::new(
            PcCustomEngine::new_with_runtime_stores(
                journal,
                Some(OsIdentity::load().expect("real Windows identity")),
                Some(profile_store),
                Some(layout_store),
            )
            .expect("screen-sharing engine"),
        );
        let started = crate::share_session::start(engine.clone(), share_store.clone())
            .expect("start through preview, commit, and journal");
        assert_eq!(started.status, "started");

        let sleep_item_id = share_store
            .sleep_item_id_for_test()
            .expect("persisted sleep item reference");
        let sleep_during = crate::windows::sleep_lease_manager()
            .and_then(|manager| manager.snapshot_for(sleep_item_id))
            .expect("read sleep request through manager probe");
        assert!(sleep_during.requested_owner_active);

        let applied = child.request("read");
        assert_eq!(applied, desired);
        let externally_changed = child.request("move_external");
        assert_ne!(externally_changed[0], applied[0]);
        assert_eq!(externally_changed[1], applied[1]);

        let microphone_evidence = match crate::windows::read_default_comms_mic_mute() {
            Ok(observed) => format!(
                "EVIDENCE: share_session item=microphone measured=true muted={} reason=windows_default_comms_input_only_meeting_app_delivery_not_measured",
                observed.muted
            ),
            Err(_) => "EVIDENCE: share_session item=microphone measured=false reason=windows_default_comms_input_probe_unavailable".to_owned(),
        };
        let audio_evidence = match crate::windows::read_audio_output_observation() {
            Ok(observed) => format!(
                "EVIDENCE: share_session item=audio_output measured=true endpoints={} default_exists={} reason=windows_default_output_only_meeting_app_route_not_measured",
                observed.endpoints.len(),
                observed.endpoints.iter().any(|endpoint| endpoint.is_default),
            ),
            Err(_) => "EVIDENCE: share_session item=audio_output measured=false reason=windows_default_output_probe_unavailable".to_owned(),
        };

        let finished = crate::share_session::finish(engine, share_store.clone())
            .expect("finish through reverse journal rollback");
        assert_eq!(finished.status, "finished");
        assert!(
            finished
                .details
                .iter()
                .any(|detail| detail.contains("外部から移動")),
            "the skipped window must be explained separately"
        );
        let after = child.request("read");
        assert_eq!(after[0], externally_changed[0]);
        assert_eq!(after[1], before[1]);

        let sleep_after = crate::windows::sleep_lease_manager()
            .and_then(|manager| manager.snapshot_for(sleep_item_id))
            .expect("read released sleep request through manager probe");
        assert!(!sleep_after.requested_owner_active);
        assert!(!share_store.state().active);

        println!(
            "EVIDENCE: share_session item=sleep measured=true during_active={} after_active={} reason=independent_lease_snapshot_before_and_after",
            sleep_during.requested_owner_active, sleep_after.requested_owner_active
        );
        println!(
            "EVIDENCE: share_session item=window_layout measured=true desired=[{}] before=[{}] applied=[{}] externally_changed=[{}] after=[{}] reason=coordinates_read_by_separate_process",
            probe_text(&desired),
            probe_text(&before),
            probe_text(&applied),
            probe_text(&externally_changed),
            probe_text(&after),
        );
        println!("{microphone_evidence}");
        println!("{audio_evidence}");
        println!(
            "EVIDENCE: share_session item=notifications measured=false reason=no_general_probe_for_priority_or_app_notifications"
        );
    }
}
