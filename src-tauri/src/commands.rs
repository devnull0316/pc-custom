use std::str::FromStr;

use tauri::State;

use crate::{
    action::{ActionId, ActionParameters},
    bootstrap::ApplicationState,
    error::{CoreError, CoreResult},
    game_profile::{CreateProfileRequest, StoredProfile},
    journal::{ReconcileResult, TimelineItem},
    presentation::{
        ActionPresentation, BootstrapStatus, CommitPreviewRequest, CommitResult, DetectionResponse,
        PreviewActionsRequest, PreviewResponse, RollbackItemRequest,
    },
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
