pub mod action;
pub mod actions;
#[cfg(test)]
mod appearance_scene_contract;
pub mod backup;
mod bootstrap;
mod commands;
pub mod compatibility;
pub mod config_snapshot;
pub mod display_profile;
pub mod engine;
pub mod error;
pub mod game_profile;
pub mod health_report;
pub mod hot_corner;
pub mod ipc;
pub mod journal;
pub mod presentation;
mod settings_file;
pub mod settings_link;
pub mod setup;
pub mod storage_history;
pub mod taskbar_watcher;
pub mod theme_schedule;
pub mod window_layout;
pub mod windows;

use tauri::Manager;

pub fn run() {
    let state = bootstrap::ApplicationState::initialize();
    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            app.state::<bootstrap::ApplicationState>()
                .register_hot_corner_target(app.handle().clone());
            #[cfg(windows)]
            {
                let window = app.get_webview_window("main").ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "PCカスタム main window was not created",
                    )
                })?;
                let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
                // tauri は内部で新しい windows クレートを使うため HWND 型が本crate(0.58)と異なる。
                // 生ポインタを取り出して 0.58 の HWND へ包み直す（表現差を吸収するため *mut へキャスト）。
                let raw = window.hwnd()?;
                // `windows` はローカルモジュール(crate::windows)と衝突するため外部crateは `::windows`。
                let hwnd = ::windows::Win32::Foundation::HWND(raw.0);
                crate::windows::apply_mica_backdrop(hwnd, dark)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_status,
            commands::pick_game_executable,
            commands::restart_explorer_shell,
            commands::list_actions,
            commands::detect_action,
            commands::preview_actions,
            commands::commit_preview,
            commands::commit_preview_as_trial,
            commands::confirm_trial,
            commands::list_timeline,
            commands::build_health_report,
            commands::taskbar_auto_hide_state,
            commands::set_taskbar_auto_hide,
            commands::hot_corner_get,
            commands::hot_corner_set,
            commands::rollback_item,
            commands::reconcile_now,
            commands::profiles_list,
            commands::profile_create,
            commands::profile_set_enabled,
            commands::profile_set_ribbon_color,
            commands::profile_run_now,
            commands::profile_restore_now,
            commands::profile_delete,
            commands::config_export,
            commands::config_import_preview,
            commands::config_import_apply,
            commands::setup_app_catalog,
            commands::setup_app_install,
            commands::config_snapshot_export,
            commands::theme_schedule_get,
            commands::theme_schedule_set,
            commands::storage_temp_cleanup_plan,
            commands::storage_temp_cleanup_apply,
            commands::storage_history_capture,
            commands::storage_history_list,
            commands::storage_history_clear,
            commands::open_windows_settings,
            commands::get_window_layout_status,
            commands::save_window_layout,
            commands::list_offscreen_windows,
            commands::rescue_offscreen_window,
            commands::rollback_offscreen_window,
        ])
        .run(tauri::generate_context!())
        .expect("PCカスタム runtime terminated unexpectedly");
}
