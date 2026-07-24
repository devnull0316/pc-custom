//! Six stable, non-admin Actions. All handlers are registered at compile time.

mod active_scheme_check;
mod color_mode;
mod common;
mod explorer_visibility;
mod prevent_sleep;
mod process_watch;

pub use active_scheme_check::{ActiveSchemeCheckAction, ACTIVE_SCHEME_CHECK_ACTION};
pub use color_mode::{ColorModeAction, COLOR_MODE_ACTION};
pub use explorer_visibility::{
    ClockSecondsAction, ShowExtensionsAction, ShowHiddenAction, CLOCK_SECONDS_ACTION,
    SHOW_EXTENSIONS_ACTION, SHOW_HIDDEN_ACTION,
};
pub use prevent_sleep::{PreventSleepAction, PREVENT_SLEEP_ACTION};
pub use process_watch::{ProcessWatchAction, PROCESS_WATCH_ACTION};
