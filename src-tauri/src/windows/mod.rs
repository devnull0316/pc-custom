//! Narrow Windows primitives. No function accepts a shell command or user-selected registry path.

mod app_launch;
mod broadcast;
mod execution_state;
mod observations;
mod power;
mod powertoys;
mod process;
mod readiness;
mod registry;
mod transaction_lock;
mod ui_probe;
mod update_status;
mod window_effects;
mod wmi_process;

pub use app_launch::{
    apps_for_bundle, launch_known_apps, observe_known_apps, resolve_known_app,
    resolve_powertoys_app_path, KnownApp,
};
pub use broadcast::{notify_explorer_settings_changed, notify_theme_changed, BroadcastReport};
pub use execution_state::{sleep_lease_manager, SleepLeaseManager, SleepLeaseSnapshot};
pub use observations::{
    delete_user_temp_files, plan_user_temp_cleanup, read_startup_inventory,
    read_system_drive_space, read_user_temp_inventory, TempCleanupOutcome, TempCleanupPlan,
    TEMP_CLEANUP_MIN_AGE_DAYS,
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
pub use transaction_lock::{
    acquire_app_instance_lock, acquire_core_mutation_lock, AppInstanceGuard, CoreMutationGuard,
};
pub use update_status::read_windows_update_status;
pub use window_effects::{apply_mica_backdrop, system_accent_color, AccentColor};
pub use wmi_process::wmi_process_ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsErrorKind {
    UnsupportedPlatform,
    ApiFailure,
    AccessDenied,
    ResourceLimit,
    InvalidData,
    ChannelClosed,
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

pub use ui_probe::{observe_taskbar_layout, TaskbarLayoutObservation};
