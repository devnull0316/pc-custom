pub mod action;
pub mod actions;
pub mod backup;
mod bootstrap;
mod commands;
pub mod compatibility;
pub mod config_snapshot;
pub mod engine;
pub mod error;
pub mod game_profile;
pub mod ipc;
pub mod journal;
pub mod presentation;
pub mod settings_link;
pub mod setup;
pub mod theme_schedule;
pub mod windows;

use tauri::Manager;

pub fn run() {
    let state = bootstrap::ApplicationState::initialize();
    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
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
                let hwnd = ::windows::Win32::Foundation::HWND(raw.0 as *mut core::ffi::c_void);
                crate::windows::apply_mica_backdrop(hwnd, dark)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_status,
            commands::list_actions,
            commands::detect_action,
            commands::preview_actions,
            commands::commit_preview,
            commands::list_timeline,
            commands::rollback_item,
            commands::reconcile_now,
            commands::profiles_list,
            commands::profile_create,
            commands::profile_set_enabled,
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
            commands::open_windows_settings,
        ])
        .run(tauri::generate_context!())
        .expect("PCカスタム runtime terminated unexpectedly");
}
