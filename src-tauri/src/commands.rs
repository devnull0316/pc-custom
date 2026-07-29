use std::str::FromStr;

use tauri::{Manager, State};

use crate::{
    action::{ActionId, ActionParameters},
    bootstrap::ApplicationState,
    error::{CoreError, CoreResult},
    game_profile::{CreateProfileRequest, StoredProfile},
    health_report::HealthReport,
    journal::{ReconcileResult, TimelineItem},
    presentation::{
        ActionPresentation, BootstrapStatus, CommitPreviewRequest, CommitResult, DetectionResponse,
        PreviewActionsRequest, PreviewResponse, RollbackItemRequest,
    },
    taskbar_watcher::TaskbarAutoHideState,
    window_layout::WindowLayoutStatus,
};

#[tauri::command]
pub fn get_bootstrap_status(state: State<'_, ApplicationState>) -> BootstrapStatus {
    state.bootstrap_status()
}

#[tauri::command]
pub fn list_actions(state: State<'_, ApplicationState>) -> CoreResult<Vec<ActionPresentation>> {
    Ok(state.engine()?.list_actions())
}

#[tauri::command]
pub fn detect_action(
    state: State<'_, ApplicationState>,
    action_id: String,
) -> CoreResult<DetectionResponse> {
    let action_id = ActionId::from_str(&action_id)
        .map_err(|_| CoreError::invalid_request("登録されていないAction IDです。"))?;
    if action_id == ActionId::SetupWindowLayout {
        let engine = state.engine()?;
        return engine.detect_parameters(engine.window_layout_parameters()?);
    }
    state.engine()?.detect_action(action_id)
}

#[tauri::command]
pub fn preview_actions(
    state: State<'_, ApplicationState>,
    mut request: PreviewActionsRequest,
) -> CoreResult<PreviewResponse> {
    let engine = state.engine()?;
    for action in &mut request.actions {
        if action.action_id == ActionId::SetupWindowLayout.as_str() {
            if !action.parameters.is_empty() {
                return Err(CoreError::invalid_request(
                    "ウィンドウ配置の復元内容は保存済みデータからだけ作成できます。",
                ));
            }
            let ActionParameters::SetupWindowLayout { invocation } =
                engine.window_layout_parameters()?
            else {
                unreachable!("window layout helper returns the matching variant");
            };
            action.parameters.insert(
                "invocation".to_owned(),
                serde_json::to_value(invocation).map_err(|_| CoreError::storage())?,
            );
        }
    }
    engine.preview(request)
}

#[tauri::command]
pub fn get_window_layout_status(
    state: State<'_, ApplicationState>,
) -> CoreResult<WindowLayoutStatus> {
    state.engine()?.window_layout_status()
}

#[tauri::command]
pub fn save_window_layout(
    state: State<'_, ApplicationState>,
    unregistered_games_closed: bool,
) -> CoreResult<WindowLayoutStatus> {
    state
        .engine()?
        .save_window_layout(unregistered_games_closed)
}

#[tauri::command]
pub fn list_offscreen_windows(
    app: tauri::AppHandle,
    state: State<'_, ApplicationState>,
) -> CoreResult<crate::windows::OffscreenWindowScan> {
    state
        .engine()?
        .scan_offscreen_windows(main_window_handle(&app)?)
}

#[tauri::command]
pub fn rescue_offscreen_window(
    app: tauri::AppHandle,
    state: State<'_, ApplicationState>,
    candidate_id: String,
) -> CoreResult<crate::windows::OffscreenWindowRescueOutcome> {
    let candidate_id = uuid::Uuid::parse_str(&candidate_id)
        .map_err(|_| CoreError::invalid_request("選んだウィンドウの確認情報が無効です。"))?;
    state
        .engine()?
        .rescue_offscreen_window(candidate_id, main_window_handle(&app)?)
}

#[tauri::command]
pub fn rollback_offscreen_window(
    state: State<'_, ApplicationState>,
    undo_id: String,
) -> CoreResult<crate::windows::OffscreenWindowRescueOutcome> {
    let undo_id = uuid::Uuid::parse_str(&undo_id)
        .map_err(|_| CoreError::invalid_request("元の位置へ戻すための確認情報が無効です。"))?;
    state.engine()?.rollback_offscreen_window(undo_id)
}

#[cfg(windows)]
fn main_window_handle(app: &tauri::AppHandle) -> CoreResult<isize> {
    let window = app.get_webview_window("main").ok_or_else(|| {
        CoreError::recovery_required("PCカスタムの画面を確認できないため、移動先を決められません。")
    })?;
    let raw = window.hwnd().map_err(|_| {
        CoreError::recovery_required("PCカスタムの画面を確認できないため、移動先を決められません。")
    })?;
    Ok(raw.0 as isize)
}

#[cfg(not(windows))]
fn main_window_handle(_app: &tauri::AppHandle) -> CoreResult<isize> {
    Err(CoreError::invalid_request(
        "この機能はWindowsでのみ利用できます。",
    ))
}

#[tauri::command]
pub fn commit_preview(
    state: State<'_, ApplicationState>,
    request: CommitPreviewRequest,
) -> CoreResult<CommitResult> {
    state.engine()?.commit_preview(&request.preview_token)
}

#[tauri::command]
pub fn list_timeline(state: State<'_, ApplicationState>) -> CoreResult<Vec<TimelineItem>> {
    state.engine()?.list_timeline(250)
}

/// 最大化しているときだけタスクバーを隠す設定の、現在の状態。
#[tauri::command]
pub fn taskbar_auto_hide_state(
    state: State<'_, ApplicationState>,
) -> CoreResult<TaskbarAutoHideState> {
    Ok(state.taskbar_store()?.state())
}

/// この機能を使うかどうかを切り替える。
///
/// 切ったときにタスクバーを戻すのは監視側の役目。ここは意思だけを記録する。
#[tauri::command]
pub fn set_taskbar_auto_hide(
    state: State<'_, ApplicationState>,
    enabled: bool,
) -> CoreResult<TaskbarAutoHideState> {
    let store = state.taskbar_store()?;
    // 隠している最中の記録は引き継ぐ。切り替えただけで戻す先を忘れない。
    let current = store.get();
    store.set(crate::taskbar_watcher::TaskbarAutoHideSetting {
        enabled,
        hiding_restore_to: current.hiding_restore_to,
    })?;
    Ok(store.state())
}

/// 適用した設定が今も残っているかを照合する。読むだけで、何も変更しない。
#[tauri::command]
pub fn build_health_report(state: State<'_, ApplicationState>) -> CoreResult<HealthReport> {
    state.engine()?.build_health_report()
}

#[tauri::command]
pub fn rollback_item(
    state: State<'_, ApplicationState>,
    request: RollbackItemRequest,
) -> CoreResult<CommitResult> {
    state.engine()?.rollback_item(request.item_id)
}

#[tauri::command]
pub fn reconcile_now(state: State<'_, ApplicationState>) -> CoreResult<ReconcileResult> {
    state.engine()?.reconcile_now()
}

#[tauri::command]
pub fn profiles_list(state: State<'_, ApplicationState>) -> CoreResult<Vec<StoredProfile>> {
    Ok(state.profile_store()?.list())
}

#[tauri::command]
pub fn profile_create(
    state: State<'_, ApplicationState>,
    request: CreateProfileRequest,
) -> CoreResult<StoredProfile> {
    state.engine()?.create_profile(request)
}

#[tauri::command]
pub fn profile_set_enabled(
    state: State<'_, ApplicationState>,
    id: String,
    enabled: bool,
) -> CoreResult<()> {
    state.profile_store()?.set_enabled(&id, enabled)
}

#[tauri::command]
pub fn profile_run_now(
    state: State<'_, ApplicationState>,
    id: String,
) -> CoreResult<crate::game_profile::ManualProfileResult> {
    crate::game_profile::run_manual_profile(state.engine()?, state.profile_store()?, &id)
}

#[tauri::command]
pub fn profile_restore_now(
    state: State<'_, ApplicationState>,
    id: String,
) -> CoreResult<crate::game_profile::ManualProfileResult> {
    crate::game_profile::restore_manual_profile(state.engine()?, state.profile_store()?, &id)
}
#[tauri::command]
pub fn profile_delete(state: State<'_, ApplicationState>, id: String) -> CoreResult<()> {
    state.engine()?.delete_profile(&id)
}

#[tauri::command]
pub fn theme_schedule_get(
    state: State<'_, ApplicationState>,
) -> CoreResult<crate::theme_schedule::ThemeScheduleState> {
    Ok(state.theme_schedule_store()?.state())
}

#[tauri::command]
pub fn theme_schedule_set(
    state: State<'_, ApplicationState>,
    schedule: crate::theme_schedule::ThemeSchedule,
) -> CoreResult<crate::theme_schedule::ThemeScheduleState> {
    let store = state.theme_schedule_store()?;
    store.set(schedule)?;
    Ok(store.state())
}

/// 現在設定の控え（read-only）。Windowsを変更せず、検出済み状態をJSONで書き出す。
#[tauri::command]
pub fn config_snapshot_export(state: State<'_, ApplicationState>) -> CoreResult<String> {
    let bootstrap = state.bootstrap_status();
    let actions = state.engine()?.list_actions();
    let snapshot = crate::config_snapshot::build_settings_snapshot(
        &actions,
        bootstrap.build,
        chrono::Utc::now().to_rfc3339(),
    );
    serde_json::to_string_pretty(&snapshot).map_err(|_| CoreError::storage())
}

/// 削除候補の一覧（read-only）。適用前に必ずこれを見せる。
#[tauri::command]
pub fn storage_temp_cleanup_plan(
    state: State<'_, ApplicationState>,
) -> CoreResult<crate::windows::TempCleanupPlan> {
    // 安全コアが使える状態でだけ受け付ける（fail-closed）。
    let _engine = state.engine()?;
    crate::windows::plan_user_temp_cleanup(crate::windows::TEMP_CLEANUP_MIN_AGE_DAYS)
        .map_err(|_| CoreError::invalid_request("一時ファイルの一覧を取得できませんでした。"))
}

/// 一覧と同じ条件で再走査して削除する。**元に戻せない**。
#[tauri::command]
pub fn storage_temp_cleanup_apply(
    state: State<'_, ApplicationState>,
) -> CoreResult<crate::windows::TempCleanupOutcome> {
    let _engine = state.engine()?;
    crate::windows::delete_user_temp_files(crate::windows::TEMP_CLEANUP_MIN_AGE_DAYS)
        .map_err(|_| CoreError::invalid_request("一時ファイルを削除できませんでした。"))
}

/// 該当するWindows設定ページを開く。固定表にあるms-settings URIのみ。
#[tauri::command]
pub fn open_windows_settings(action_id: String) -> CoreResult<String> {
    let action_id = ActionId::from_str(&action_id)
        .map_err(|_| CoreError::invalid_request("登録されていないAction IDです。"))?;
    crate::settings_link::open_settings_page(action_id).map(|page| page.to_owned())
}

#[tauri::command]
pub fn setup_app_catalog() -> Vec<crate::setup::SetupAppDto> {
    crate::setup::app_catalog()
}

#[tauri::command]
pub fn setup_app_install(app_id: String) -> CoreResult<crate::setup::InstallOutcome> {
    crate::setup::install(&app_id)
}

#[tauri::command]
pub fn config_export(state: State<'_, ApplicationState>) -> CoreResult<String> {
    state.profile_store()?.export_json()
}

#[tauri::command]
pub fn config_import_preview(
    state: State<'_, ApplicationState>,
    json: String,
) -> CoreResult<Vec<crate::game_profile::ImportPreviewItem>> {
    state.profile_store()?.import_preview(&json)
}

#[tauri::command]
pub fn config_import_apply(
    state: State<'_, ApplicationState>,
    json: String,
) -> CoreResult<crate::game_profile::ImportResult> {
    state.engine()?.import_profiles(&json)
}

/// エクスプローラー(シェル)を再起動する。
///
/// 一部の設定はレジストリへ書いただけでは画面が変わらず、シェルを再起動して初めて反映される。
/// これは**利用者が明示的に押したときだけ**呼ばれる。適用処理が自動で呼ぶことはない。
///
/// 引数を取らない。フロントから渡せるものが無いので、経路として悪用しようがない。
#[tauri::command]
pub fn restart_explorer_shell() -> CoreResult<crate::windows::ShellRestartOutcome> {
    crate::windows::restart_shell().map_err(|_error| {
        CoreError::new(
            "SHELL_RESTART_FAILED",
            "APPLY",
            true,
            "エクスプローラーを再起動できませんでした。手動で再起動するか、サインインし直してください。",
        )
    })
}

/// ゲームの実行ファイルを利用者に選んでもらう。
///
/// **引数を取らない。** フロントから開き先や絞り込みを操作できないので、
/// 経路として悪用する余地がない。返るのは利用者が実際に選んだ 1 件のパスだけで、
/// 取り消された場合は `None`（取り消しは失敗ではない）。
#[tauri::command]
pub fn pick_game_executable() -> CoreResult<Option<String>> {
    crate::windows::pick_executable().map_err(|_| {
        CoreError::new(
            "FILE_PICKER_FAILED",
            "DETECT",
            true,
            "ファイル選択画面を開けませんでした。パスを直接入力することもできます。",
        )
    })
}

/// 試用として適用する。`holdSeconds` 以内に確定されなければ、次の起動で元へ戻る。
#[tauri::command]
pub fn commit_preview_as_trial(
    state: State<'_, ApplicationState>,
    request: CommitPreviewRequest,
    hold_seconds: u32,
) -> CoreResult<CommitResult> {
    state
        .engine()?
        .commit_preview_as_trial(&request.preview_token, hold_seconds)
}

/// 試用を確定する。以後この変更は自動で戻さない。
#[tauri::command]
pub fn confirm_trial(
    state: State<'_, ApplicationState>,
    transaction_id: String,
) -> CoreResult<bool> {
    let parsed = uuid::Uuid::parse_str(&transaction_id)
        .map_err(|_| CoreError::invalid_request("指定された変更のまとまりが見つかりません。"))?;
    state.engine()?.confirm_trial(parsed)
}
