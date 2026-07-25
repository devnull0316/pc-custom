//! Registered, non-admin Actions. Every handler is compiled into the binary.

mod active_scheme_check;
mod color_mode;
mod common;
mod explorer_visibility;
mod game_readiness;
mod prevent_sleep;
mod power_scheme_switch;
mod process_watch;
mod registry_settings;
mod system_observations;
mod window_color;

pub use active_scheme_check::{ActiveSchemeCheckAction, ACTIVE_SCHEME_CHECK_ACTION};
pub use color_mode::{ColorModeAction, COLOR_MODE_ACTION};
pub use explorer_visibility::{
    ClockSecondsAction, CompactViewAction, ItemCheckboxesAction, ShowExtensionsAction,
    ShowHiddenAction, TaskViewAction, TransparencyAction, WidgetsAction,
    CLOCK_SECONDS_ACTION, COMPACT_VIEW_ACTION, ITEM_CHECKBOXES_ACTION,
    SHOW_EXTENSIONS_ACTION, SHOW_HIDDEN_ACTION, TASK_VIEW_ACTION, TRANSPARENCY_ACTION,
    WIDGETS_ACTION,
};
pub use game_readiness::{
    GameReadinessCheckAction, GAME_READINESS_CHECK_ACTION,
};
pub use prevent_sleep::{PreventSleepAction, PREVENT_SLEEP_ACTION};
pub use power_scheme_switch::{PowerSchemeSwitchAction, POWER_SCHEME_SWITCH_ACTION};
pub use process_watch::{ProcessWatchAction, PROCESS_WATCH_ACTION};
pub use registry_settings::*;
pub use system_observations::{
    SystemObservationAction, ACCENT_COLOR_CHECK_ACTION, FREE_SPACE_CHECK_ACTION,
    STARTUP_INVENTORY_ACTION, TEMP_FILES_CHECK_ACTION,
};
pub use window_color::{WindowColorAction, WINDOW_COLOR_ACTION};
