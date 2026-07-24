pub mod action;
pub mod actions;
pub mod backup;
mod bootstrap;
mod commands;
pub mod compatibility;
pub mod engine;
pub mod error;
pub mod ipc;
pub mod journal;
pub mod presentation;
pub mod windows;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .manage(bootstrap::ApplicationState::initialize())
        .setup(|app| {
            #[cfg(windows)]
            {
                use tauri::window::WindowExtWindows;

                let window = app.get_webview_window("main").ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Totonoe main window was not created",
                    )
                })?;
                let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
                crate::windows::apply_mica_backdrop(window.hwnd()?, dark)?;
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
        ])
        .run(tauri::generate_context!())
        .expect("Totonoe runtime terminated unexpectedly");
}

