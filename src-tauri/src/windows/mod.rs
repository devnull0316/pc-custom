//! Narrow Windows primitives. No function accepts a shell command or user-selected registry path.

mod accessibility_shortcuts;
mod app_launch;
mod audio;
mod broadcast;
mod execution_state;
// 実行ファイルを手で打たせない。Windows自身の選択画面を開く。
mod file_picker;
mod observations;
// オーバーレイをタスクバーの上へ置き続けるための、位置と状況の読み取り。
mod overlay_anchor;
mod pointer_feel;
mod power;
mod power_mode;
mod powertoys;
mod process;
mod readiness;
mod registry;
// 反映にはシェル再起動が要る項目がある。利用者が明示的に選んだときだけ実行する。
mod shell_restart;
mod taskbar_autohide;
mod transaction_lock;
// 検証専用の計器。実UIを外から読むためにエクスプローラーの窓を開き、シェル設定を書き、
// 窓を閉じる処理を含む。製品側からは一度も呼ばれないので、出荷バイナリへは入れない。
#[cfg(test)]
mod ui_probe;
mod update_status;
mod window_effects;
mod window_placement;
mod wmi_process;

pub use accessibility_shortcuts::{
    filter_confirmation_is_enabled, filter_feature_is_enabled, filter_shortcut_is_enabled,
    read_keyboard_accessibility_settings, replace_keyboard_accessibility_settings,
    sticky_confirmation_is_enabled, sticky_feature_is_enabled, sticky_shortcut_is_enabled,
    sticky_transient_state_is_active, without_shift_shortcuts, FILTER_SHORTCUT_FLAGS,
    STICKY_SHORTCUT_FLAGS,
};
pub use app_launch::{
    apps_for_bundle, launch_known_apps, observe_known_apps, resolve_known_app,
    resolve_powertoys_app_path, KnownApp,
};
pub use audio::read_audio_output_observation;
pub use broadcast::{notify_explorer_settings_changed, notify_theme_changed, BroadcastReport};
pub use execution_state::{sleep_lease_manager, SleepLeaseManager, SleepLeaseSnapshot};
pub use file_picker::pick_executable;
pub use observations::{
    delete_user_temp_files, plan_user_temp_cleanup, read_startup_inventory,
    read_system_drive_space, read_user_temp_inventory, TempCleanupOutcome, TempCleanupPlan,
    TEMP_CLEANUP_MIN_AGE_DAYS,
};
pub use overlay_anchor::{
    foreground_is_fullscreen, read_taskbar_anchor, TaskbarAnchor, TaskbarEdge,
};
pub use power::{active_power_scheme, active_power_scheme_guid, set_active_power_scheme};
pub use powertoys::read_powertoys_installation;
pub use process::{
    process_instance_status, registered_file_identity, snapshot_process_identities,
    ProcessIdentity, ProcessInstanceStatus, ProcessSnapshotReport,
};
pub use readiness::{
    read_active_advanced_color, read_default_render_audio_endpoint, read_primary_refresh_rate,
};
#[cfg(test)]
pub use registry::delete_key_if_empty;
pub use registry::{
    delete_value, read_value_state, write_raw_value, RawRegistryValue, RawRegistryValueState,
};
pub use shell_restart::{restart_shell, taskbar_is_present, ShellRestartOutcome};
pub use transaction_lock::{
    acquire_app_instance_lock, acquire_core_mutation_lock, AppInstanceGuard, CoreMutationGuard,
};
pub use update_status::read_windows_update_status;
pub use window_effects::{apply_mica_backdrop, system_accent_color, AccentColor};
#[cfg(all(test, windows))]
pub(crate) use window_placement::{
    allow_own_window_candidates_for_test, capture_window_entry_for_test,
    read_window_placement_for_test,
};
pub use window_placement::{
    capture_window_layout, capture_window_layout_originals, classify_window_layout_transaction,
    observe_original_window_placements, observe_window_layout, restore_window_layout,
    restore_window_placement_entries, verify_captured_window_layout_originals,
    OffscreenWindowBlockReason, OffscreenWindowCandidate, OffscreenWindowRescueManager,
    OffscreenWindowRescueOutcome, OffscreenWindowScan, OffscreenWindowUndo,
    WindowLayoutTransactionState,
};
pub use wmi_process::wmi_process_ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsErrorKind {
    UnsupportedPlatform,
    ApiFailure,
    AccessDenied,
    ResourceLimit,
    InvalidData,
    ChannelClosed,
    /// A primitive re-read a mutable resource immediately before writing and
    /// found that it no longer matched the caller's expected state.
    ExternalConflict,
    /// A primitive dispatched at least one write and could not prove that its
    /// bounded inverse compensation completed.
    RecoveryRequired,
}

#[derive(Debug, thiserror::Error)]
#[error("{operation} failed")]
pub struct WindowsError {
    pub kind: WindowsErrorKind,
    pub operation: &'static str,
    pub os_code: Option<i64>,
}

impl WindowsError {
    pub const fn new(
        kind: WindowsErrorKind,
        operation: &'static str,
        os_code: Option<i64>,
    ) -> Self {
        Self {
            kind,
            operation,
            os_code,
        }
    }

    #[cfg(windows)]
    pub fn io(operation: &'static str, error: &std::io::Error) -> Self {
        let kind = match error.raw_os_error() {
            Some(5) => WindowsErrorKind::AccessDenied,
            _ => WindowsErrorKind::ApiFailure,
        };
        Self::new(kind, operation, error.raw_os_error().map(i64::from))
    }

    pub const fn unsupported(operation: &'static str) -> Self {
        Self::new(WindowsErrorKind::UnsupportedPlatform, operation, None)
    }
}

pub type WindowsResult<T> = Result<T, WindowsError>;

pub use pointer_feel::{read_pointer_feel, replace_pointer_feel, PointerFeel};
pub use power_mode::{
    read_ac_mode, read_dc_mode, read_effective_mode, write_ac_mode, write_ac_mode_raw,
    write_dc_mode, write_dc_mode_raw, EffectiveMode, PowerMode, PowerModeReading,
};
pub use taskbar_autohide::{
    observe_taskbar_auto_hide, replace_taskbar_auto_hide, TaskbarAutoHideObservation,
};
#[cfg(test)]
pub use ui_probe::{observe_taskbar_layout, TaskbarLayoutObservation};
