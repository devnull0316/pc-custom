use serde::{Deserialize, Serialize};

use crate::window_layout::WindowLayoutInvocation;

use super::{ActionId, WindowColorPreset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColorMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerScheme {
    Balanced,
    PowerSaver,
    HighPerformance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppLaunchBundle {
    Study,
    Work,
    Creative,
    PowerToys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarSearchMode {
    Hidden,
    Icon,
    IconAndLabel,
    SearchBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarAlignment {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartLayout {
    Default,
    MorePins,
    MoreRecommendations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerLaunchTarget {
    Home,
    ThisPc,
    Downloads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarGroupingMode {
    Always,
    WhenFull,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarMultiMonitorMode {
    AllTaskbars,
    MainAndWindow,
    WindowMonitor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessFileIdentity {
    pub volume_serial_number: u64,
    pub file_id: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessBindingParameters {
    /// Device-local absolute path selected by the user. It must be revalidated from a handle.
    pub canonical_path: String,
    pub file_identity: ProcessFileIdentity,
}

/// The tagged enum prevents an Action ID from being paired with another Action's parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_id", content = "parameters", deny_unknown_fields)]
pub enum ActionParameters {
    #[serde(rename = "session.prevent_sleep")]
    SessionPreventSleep {
        #[serde(default)]
        keep_display_on: bool,
    },
    #[serde(rename = "input.shift_interruption_guard")]
    InputShiftInterruptionGuard {},
    #[serde(rename = "power.active_scheme_check")]
    PowerActiveSchemeCheck {},
    #[serde(rename = "power.active_scheme_switch")]
    PowerActiveSchemeSwitch { scheme: PowerScheme },
    #[serde(rename = "explorer.show_extensions")]
    ExplorerShowExtensions { show: bool },
    #[serde(rename = "explorer.show_hidden")]
    ExplorerShowHidden { show: bool },
    #[serde(rename = "explorer.clock_seconds")]
    ExplorerClockSeconds { show: bool },
    #[serde(rename = "appearance.transparency")]
    AppearanceTransparency { enabled: bool },
    #[serde(rename = "taskbar.task_view")]
    TaskbarTaskView { show: bool },
    #[serde(rename = "taskbar.widgets")]
    TaskbarWidgets { show: bool },
    #[serde(rename = "explorer.item_checkboxes")]
    ExplorerItemCheckboxes { show: bool },
    #[serde(rename = "explorer.compact_view")]
    ExplorerCompactView { enabled: bool },
    #[serde(rename = "theme.color_mode")]
    ThemeColorMode { mode: ThemeColorMode },
    #[serde(rename = "games.process_watch")]
    GamesProcessWatch { binding: ProcessBindingParameters },
    #[serde(rename = "games.readiness_check")]
    GamesReadinessCheck {},
    #[serde(rename = "taskbar.search_mode")]
    TaskbarSearchMode { mode: TaskbarSearchMode },
    #[serde(rename = "taskbar.alignment")]
    TaskbarAlignment { alignment: TaskbarAlignment },
    #[serde(rename = "start.layout")]
    StartLayout { layout: StartLayout },
    #[serde(rename = "start.recommendations")]
    StartRecommendations { enabled: bool },
    #[serde(rename = "explorer.launch_target")]
    ExplorerLaunchTarget { target: ExplorerLaunchTarget },
    #[serde(rename = "explorer.recent_files")]
    ExplorerRecentFiles { show: bool },
    #[serde(rename = "taskbar.button_grouping")]
    TaskbarButtonGrouping { mode: TaskbarGroupingMode },
    #[serde(rename = "taskbar.flashing")]
    TaskbarFlashing { enabled: bool },
    #[serde(rename = "taskbar.share_window")]
    TaskbarShareWindow { enabled: bool },
    #[serde(rename = "taskbar.show_desktop")]
    TaskbarShowDesktop { enabled: bool },
    #[serde(rename = "search.recent_on_hover")]
    SearchRecentOnHover { enabled: bool },
    #[serde(rename = "taskbar.multi_monitor")]
    TaskbarMultiMonitor { enabled: bool },
    #[serde(rename = "taskbar.multi_monitor_mode")]
    TaskbarMultiMonitorMode { mode: TaskbarMultiMonitorMode },
    #[serde(rename = "taskbar.secondary_button_grouping")]
    TaskbarSecondaryButtonGrouping { mode: TaskbarGroupingMode },
    #[serde(rename = "start.show_all_pins")]
    StartShowAllPins { enabled: bool },
    #[serde(rename = "start.recent_apps")]
    StartRecentApps { show: bool },
    #[serde(rename = "appearance.accent_start_taskbar")]
    AppearanceAccentStartTaskbar { enabled: bool },
    #[serde(rename = "appearance.accent_title_bars")]
    AppearanceAccentTitleBars { enabled: bool },
    #[serde(rename = "appearance.auto_accent")]
    AppearanceAutoAccent { enabled: bool },
    #[serde(rename = "games.game_mode")]
    GamesGameMode { enabled: bool },
    #[serde(rename = "games.controller_game_bar")]
    GamesControllerGameBar { enabled: bool },
    #[serde(rename = "devices.autoplay")]
    DevicesAutoplay { enabled: bool },
    #[serde(rename = "notifications.usb_errors")]
    NotificationsUsbErrors { enabled: bool },
    #[serde(rename = "notifications.weak_charger")]
    NotificationsWeakCharger { enabled: bool },
    #[serde(rename = "input.autocorrect")]
    InputAutocorrect { enabled: bool },
    #[serde(rename = "input.double_space_period")]
    InputDoubleSpacePeriod { enabled: bool },
    #[serde(rename = "input.auto_shift")]
    InputAutoShift { enabled: bool },
    #[serde(rename = "input.voice_typing_key")]
    InputVoiceTypingKey { enabled: bool },
    #[serde(rename = "input.multilingual_suggestions")]
    InputMultilingualSuggestions { enabled: bool },
    #[serde(rename = "explorer.status_bar")]
    ExplorerStatusBar { show: bool },
    #[serde(rename = "explorer.info_tips")]
    ExplorerInfoTips { show: bool },
    #[serde(rename = "explorer.hide_empty_drives")]
    ExplorerHideEmptyDrives { hide: bool },
    #[serde(rename = "explorer.nav_expand_current")]
    ExplorerNavExpandCurrent { enabled: bool },
    #[serde(rename = "explorer.nav_show_all")]
    ExplorerNavShowAll { enabled: bool },
    #[serde(rename = "explorer.separate_process")]
    ExplorerSeparateProcess { enabled: bool },
    #[serde(rename = "explorer.icons_only")]
    ExplorerIconsOnly { enabled: bool },
    #[serde(rename = "explorer.drive_letters")]
    ExplorerDriveLetters { show: bool },
    #[serde(rename = "explorer.preview_handlers")]
    ExplorerPreviewHandlers { enabled: bool },
    #[serde(rename = "explorer.sharing_wizard")]
    ExplorerSharingWizard { enabled: bool },
    #[serde(rename = "explorer.always_show_menus")]
    ExplorerAlwaysShowMenus { enabled: bool },
    #[serde(rename = "appearance.taskbar_animations")]
    AppearanceTaskbarAnimations { enabled: bool },
    #[serde(rename = "notifications.toast_banners")]
    NotificationsToastBanners { enabled: bool },
    #[serde(rename = "setup.startup_inventory")]
    SetupStartupInventory {},
    #[serde(rename = "storage.free_space_check")]
    StorageFreeSpaceCheck {},
    #[serde(rename = "storage.temp_files_check")]
    StorageTempFilesCheck {},
    #[serde(rename = "appearance.accent_color_check")]
    AppearanceAccentColorCheck {},
    #[serde(rename = "appearance.window_color")]
    AppearanceWindowColor { color: WindowColorPreset },
    #[serde(rename = "setup.powertoys_status")]
    SetupPowerToysStatus {},
    #[serde(rename = "setup.launch_apps")]
    SetupLaunchApps { bundle: AppLaunchBundle },
    #[serde(rename = "setup.windows_update_status")]
    SetupWindowsUpdateStatus {},
    #[serde(rename = "setup.default_apps")]
    SetupDefaultApps {},
    #[serde(rename = "setup.window_layout")]
    SetupWindowLayout { invocation: WindowLayoutInvocation },
    #[serde(rename = "setup.audio_output")]
    SetupAudioOutput {},
}

impl ActionParameters {
    pub const fn action_id(&self) -> ActionId {
        match self {
            Self::SessionPreventSleep { .. } => ActionId::SessionPreventSleep,
            Self::InputShiftInterruptionGuard { .. } => ActionId::InputShiftInterruptionGuard,
            Self::PowerActiveSchemeCheck { .. } => ActionId::PowerActiveSchemeCheck,
            Self::PowerActiveSchemeSwitch { .. } => ActionId::PowerActiveSchemeSwitch,
            Self::ExplorerShowExtensions { .. } => ActionId::ExplorerShowExtensions,
            Self::ExplorerShowHidden { .. } => ActionId::ExplorerShowHidden,
            Self::ExplorerClockSeconds { .. } => ActionId::ExplorerClockSeconds,
            Self::AppearanceTransparency { .. } => ActionId::AppearanceTransparency,
            Self::TaskbarTaskView { .. } => ActionId::TaskbarTaskView,
            Self::TaskbarWidgets { .. } => ActionId::TaskbarWidgets,
            Self::ExplorerItemCheckboxes { .. } => ActionId::ExplorerItemCheckboxes,
            Self::ExplorerCompactView { .. } => ActionId::ExplorerCompactView,
            Self::ThemeColorMode { .. } => ActionId::ThemeColorMode,
            Self::GamesProcessWatch { .. } => ActionId::GamesProcessWatch,
            Self::GamesReadinessCheck { .. } => ActionId::GamesReadinessCheck,
            Self::TaskbarSearchMode { .. } => ActionId::TaskbarSearchMode,
            Self::TaskbarAlignment { .. } => ActionId::TaskbarAlignment,
            Self::StartLayout { .. } => ActionId::StartLayout,
            Self::StartRecommendations { .. } => ActionId::StartRecommendations,
            Self::ExplorerLaunchTarget { .. } => ActionId::ExplorerLaunchTarget,
            Self::ExplorerRecentFiles { .. } => ActionId::ExplorerRecentFiles,
            Self::TaskbarButtonGrouping { .. } => ActionId::TaskbarButtonGrouping,
            Self::TaskbarFlashing { .. } => ActionId::TaskbarFlashing,
            Self::TaskbarShareWindow { .. } => ActionId::TaskbarShareWindow,
            Self::TaskbarShowDesktop { .. } => ActionId::TaskbarShowDesktop,
            Self::SearchRecentOnHover { .. } => ActionId::SearchRecentOnHover,
            Self::TaskbarMultiMonitor { .. } => ActionId::TaskbarMultiMonitor,
            Self::TaskbarMultiMonitorMode { .. } => ActionId::TaskbarMultiMonitorMode,
            Self::TaskbarSecondaryButtonGrouping { .. } => ActionId::TaskbarSecondaryButtonGrouping,
            Self::StartShowAllPins { .. } => ActionId::StartShowAllPins,
            Self::StartRecentApps { .. } => ActionId::StartRecentApps,
            Self::AppearanceAccentStartTaskbar { .. } => ActionId::AppearanceAccentStartTaskbar,
            Self::AppearanceAccentTitleBars { .. } => ActionId::AppearanceAccentTitleBars,
            Self::AppearanceAutoAccent { .. } => ActionId::AppearanceAutoAccent,
            Self::GamesGameMode { .. } => ActionId::GamesGameMode,
            Self::GamesControllerGameBar { .. } => ActionId::GamesControllerGameBar,
            Self::DevicesAutoplay { .. } => ActionId::DevicesAutoplay,
            Self::NotificationsUsbErrors { .. } => ActionId::NotificationsUsbErrors,
            Self::NotificationsWeakCharger { .. } => ActionId::NotificationsWeakCharger,
            Self::InputAutocorrect { .. } => ActionId::InputAutocorrect,
            Self::InputDoubleSpacePeriod { .. } => ActionId::InputDoubleSpacePeriod,
            Self::InputAutoShift { .. } => ActionId::InputAutoShift,
            Self::InputVoiceTypingKey { .. } => ActionId::InputVoiceTypingKey,
            Self::InputMultilingualSuggestions { .. } => ActionId::InputMultilingualSuggestions,
            Self::ExplorerStatusBar { .. } => ActionId::ExplorerStatusBar,
            Self::ExplorerInfoTips { .. } => ActionId::ExplorerInfoTips,
            Self::ExplorerHideEmptyDrives { .. } => ActionId::ExplorerHideEmptyDrives,
            Self::ExplorerNavExpandCurrent { .. } => ActionId::ExplorerNavExpandCurrent,
            Self::ExplorerNavShowAll { .. } => ActionId::ExplorerNavShowAll,
            Self::ExplorerSeparateProcess { .. } => ActionId::ExplorerSeparateProcess,
            Self::ExplorerIconsOnly { .. } => ActionId::ExplorerIconsOnly,
            Self::ExplorerDriveLetters { .. } => ActionId::ExplorerDriveLetters,
            Self::ExplorerPreviewHandlers { .. } => ActionId::ExplorerPreviewHandlers,
            Self::ExplorerSharingWizard { .. } => ActionId::ExplorerSharingWizard,
            Self::ExplorerAlwaysShowMenus { .. } => ActionId::ExplorerAlwaysShowMenus,
            Self::AppearanceTaskbarAnimations { .. } => ActionId::AppearanceTaskbarAnimations,
            Self::NotificationsToastBanners { .. } => ActionId::NotificationsToastBanners,
            Self::SetupStartupInventory { .. } => ActionId::SetupStartupInventory,
            Self::StorageFreeSpaceCheck { .. } => ActionId::StorageFreeSpaceCheck,
            Self::StorageTempFilesCheck { .. } => ActionId::StorageTempFilesCheck,
            Self::AppearanceAccentColorCheck { .. } => ActionId::AppearanceAccentColorCheck,
            Self::AppearanceWindowColor { .. } => ActionId::AppearanceWindowColor,
            Self::SetupPowerToysStatus { .. } => ActionId::SetupPowerToysStatus,
            Self::SetupLaunchApps { .. } => ActionId::SetupLaunchApps,
            Self::SetupWindowsUpdateStatus { .. } => ActionId::SetupWindowsUpdateStatus,
            Self::SetupDefaultApps { .. } => ActionId::SetupDefaultApps,
            Self::SetupWindowLayout { .. } => ActionId::SetupWindowLayout,
            Self::SetupAudioOutput { .. } => ActionId::SetupAudioOutput,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_parameter_fields_are_rejected() {
        let json = r#"{
            "action_id":"session.prevent_sleep",
            "parameters":{"keep_display_on":false,"command":"calc.exe"}
        }"#;
        assert!(serde_json::from_str::<ActionParameters>(json).is_err());
    }
}
