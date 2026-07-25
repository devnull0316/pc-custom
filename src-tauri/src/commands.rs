use std::str::FromStr;

use tauri::State;

use crate::{
    action::ActionId,
    bootstrap::ApplicationState,
    error::{CoreError, CoreResult},
    game_profile::{CreateProfileRequest, StoredProfile},
    journal::{ReconcileResult, TimelineItem},
    presentation::{
        ActionPresentation, BootstrapStatus, CommitPreviewRequest, CommitResult,
        DetectionResponse, PreviewActionsRequest, PreviewResponse, RollbackItemRequest,
    },
};

#[tauri::command]
pub fn get_bootstrap_status(state: State<'_, ApplicationState>) -> BootstrapStatus {
    state.bootstrap_status()
}

#[tauri::command]
pub fn list_actions(
    state: State<'_, ApplicationState>,
) -> CoreResult<Vec<ActionPresentation>> {
    Ok(state.engine()?.list_actions())
}

#[tauri::command]
pub fn detect_action(
    state: State<'_, ApplicationState>,
    action_id: String,
) -> CoreResult<DetectionResponse> {
    let action_id = ActionId::from_str(&action_id)
        .map_err(|_| CoreError::invalid_request("登録されていないAction IDです。"))?;
    state.engine()?.detect_action(action_id)
}

#[tauri::command]
pub fn preview_actions(
    state: State<'_, ApplicationState>,
    request: PreviewActionsRequest,
) -> CoreResult<PreviewResponse> {
    state.engine()?.preview(request)
}

#[tauri::command]
pub fn commit_preview(
    state: State<'_, ApplicationState>,
    request: CommitPreviewRequest,
) -> CoreResult<CommitResult> {
    state.engine()?.commit_preview(&request.preview_token)
}

#[tauri::command]
pub fn list_timeline(
    state: State<'_, ApplicationState>,
) -> CoreResult<Vec<TimelineItem>> {
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
pub fn reconcile_now(
    state: State<'_, ApplicationState>,
) -> CoreResult<ReconcileResult> {
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
    state.profile_store()?.create(request)
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
pub fn profile_delete(state: State<'_, ApplicationState>, id: String) -> CoreResult<()> {
    state.profile_store()?.delete(&id)
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
    state.profile_store()?.import_apply(&json)
}
