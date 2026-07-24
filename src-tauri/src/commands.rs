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
