use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// Stable identifiers are never reused and are the only dispatch keys accepted by the core.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ActionId {
    #[serde(rename = "session.prevent_sleep")]
    SessionPreventSleep,
    #[serde(rename = "power.active_scheme_check")]
    PowerActiveSchemeCheck,
    #[serde(rename = "power.active_scheme_switch")]
    PowerActiveSchemeSwitch,
    #[serde(rename = "explorer.show_extensions")]
    ExplorerShowExtensions,
    #[serde(rename = "explorer.show_hidden")]
    ExplorerShowHidden,
    #[serde(rename = "explorer.clock_seconds")]
    ExplorerClockSeconds,
    #[serde(rename = "appearance.transparency")]
    AppearanceTransparency,
    #[serde(rename = "taskbar.task_view")]
    TaskbarTaskView,
    #[serde(rename = "taskbar.widgets")]
    TaskbarWidgets,
    #[serde(rename = "explorer.item_checkboxes")]
    ExplorerItemCheckboxes,
    #[serde(rename = "explorer.compact_view")]
    ExplorerCompactView,
    #[serde(rename = "theme.color_mode")]
    ThemeColorMode,
    #[serde(rename = "games.process_watch")]
    GamesProcessWatch,
    #[serde(rename = "games.readiness_check")]
    GamesReadinessCheck,
    #[serde(rename = "taskbar.search_mode")]
    TaskbarSearchMode,
    #[serde(rename = "taskbar.alignment")]
    TaskbarAlignment,
    #[serde(rename = "start.layout")]
    StartLayout,
    #[serde(rename = "start.recommendations")]
    StartRecommendations,
    #[serde(rename = "explorer.launch_target")]
    ExplorerLaunchTarget,
    #[serde(rename = "explorer.recent_files")]
    ExplorerRecentFiles,
    #[serde(rename = "taskbar.button_grouping")]
    TaskbarButtonGrouping,
    #[serde(rename = "taskbar.flashing")]
    TaskbarFlashing,
    #[serde(rename = "taskbar.share_window")]
    TaskbarShareWindow,
    #[serde(rename = "taskbar.show_desktop")]
    TaskbarShowDesktop,
    #[serde(rename = "search.recent_on_hover")]
    SearchRecentOnHover,
    #[serde(rename = "taskbar.multi_monitor")]
    TaskbarMultiMonitor,
    #[serde(rename = "taskbar.multi_monitor_mode")]
    TaskbarMultiMonitorMode,
    #[serde(rename = "taskbar.secondary_button_grouping")]
    TaskbarSecondaryButtonGrouping,
    #[serde(rename = "start.show_all_pins")]
    StartShowAllPins,
    #[serde(rename = "start.recent_apps")]
    StartRecentApps,
    #[serde(rename = "appearance.accent_start_taskbar")]
    AppearanceAccentStartTaskbar,
    #[serde(rename = "appearance.accent_title_bars")]
    AppearanceAccentTitleBars,
    #[serde(rename = "appearance.auto_accent")]
    AppearanceAutoAccent,
    #[serde(rename = "games.game_mode")]
    GamesGameMode,
    #[serde(rename = "games.controller_game_bar")]
    GamesControllerGameBar,
    #[serde(rename = "devices.autoplay")]
    DevicesAutoplay,
    #[serde(rename = "notifications.usb_errors")]
    NotificationsUsbErrors,
    #[serde(rename = "notifications.weak_charger")]
    NotificationsWeakCharger,
    #[serde(rename = "input.autocorrect")]
    InputAutocorrect,
    #[serde(rename = "input.double_space_period")]
    InputDoubleSpacePeriod,
    #[serde(rename = "input.auto_shift")]
    InputAutoShift,
    #[serde(rename = "input.voice_typing_key")]
    InputVoiceTypingKey,
    #[serde(rename = "input.multilingual_suggestions")]
    InputMultilingualSuggestions,
    #[serde(rename = "explorer.status_bar")]
    ExplorerStatusBar,
    #[serde(rename = "explorer.info_tips")]
    ExplorerInfoTips,
    #[serde(rename = "explorer.hide_empty_drives")]
    ExplorerHideEmptyDrives,
    #[serde(rename = "explorer.nav_expand_current")]
    ExplorerNavExpandCurrent,
    #[serde(rename = "explorer.nav_show_all")]
    ExplorerNavShowAll,
    #[serde(rename = "explorer.separate_process")]
    ExplorerSeparateProcess,
    #[serde(rename = "explorer.icons_only")]
    ExplorerIconsOnly,
    #[serde(rename = "explorer.drive_letters")]
    ExplorerDriveLetters,
    #[serde(rename = "explorer.preview_handlers")]
    ExplorerPreviewHandlers,
    #[serde(rename = "explorer.sharing_wizard")]
    ExplorerSharingWizard,
    #[serde(rename = "explorer.always_show_menus")]
    ExplorerAlwaysShowMenus,
    #[serde(rename = "appearance.taskbar_animations")]
    AppearanceTaskbarAnimations,
    #[serde(rename = "notifications.toast_banners")]
    NotificationsToastBanners,
    #[serde(rename = "setup.startup_inventory")]
    SetupStartupInventory,
    #[serde(rename = "storage.free_space_check")]
    StorageFreeSpaceCheck,
    #[serde(rename = "storage.temp_files_check")]
    StorageTempFilesCheck,
    #[serde(rename = "appearance.accent_color_check")]
    AppearanceAccentColorCheck,
    #[serde(rename = "appearance.window_color")]
    AppearanceWindowColor,
}

impl ActionId {
    pub const ALL: [Self; 61] = [
        Self::SessionPreventSleep,
        Self::PowerActiveSchemeCheck,
        Self::PowerActiveSchemeSwitch,
        Self::ExplorerShowExtensions,
        Self::ExplorerShowHidden,
        Self::ExplorerClockSeconds,
        Self::AppearanceTransparency,
        Self::TaskbarTaskView,
        Self::TaskbarWidgets,
        Self::ExplorerItemCheckboxes,
        Self::ExplorerCompactView,
        Self::ThemeColorMode,
        Self::GamesProcessWatch,
        Self::GamesReadinessCheck,
        Self::TaskbarSearchMode,
        Self::TaskbarAlignment,
        Self::StartLayout,
        Self::StartRecommendations,
        Self::ExplorerLaunchTarget,
        Self::ExplorerRecentFiles,
        Self::TaskbarButtonGrouping,
        Self::TaskbarFlashing,
        Self::TaskbarShareWindow,
        Self::TaskbarShowDesktop,
        Self::SearchRecentOnHover,
        Self::TaskbarMultiMonitor,
        Self::TaskbarMultiMonitorMode,
        Self::TaskbarSecondaryButtonGrouping,
        Self::StartShowAllPins,
        Self::StartRecentApps,
        Self::AppearanceAccentStartTaskbar,
        Self::AppearanceAccentTitleBars,
        Self::AppearanceAutoAccent,
        Self::GamesGameMode,
        Self::GamesControllerGameBar,
        Self::DevicesAutoplay,
        Self::NotificationsUsbErrors,
        Self::NotificationsWeakCharger,
        Self::InputAutocorrect,
        Self::InputDoubleSpacePeriod,
        Self::InputAutoShift,
        Self::InputVoiceTypingKey,
        Self::InputMultilingualSuggestions,
        Self::ExplorerStatusBar,
        Self::ExplorerInfoTips,
        Self::ExplorerHideEmptyDrives,
        Self::ExplorerNavExpandCurrent,
        Self::ExplorerNavShowAll,
        Self::ExplorerSeparateProcess,
        Self::ExplorerIconsOnly,
        Self::ExplorerDriveLetters,
        Self::ExplorerPreviewHandlers,
        Self::ExplorerSharingWizard,
        Self::ExplorerAlwaysShowMenus,
        Self::AppearanceTaskbarAnimations,
        Self::NotificationsToastBanners,
        Self::SetupStartupInventory,
        Self::StorageFreeSpaceCheck,
        Self::StorageTempFilesCheck,
        Self::AppearanceAccentColorCheck,
        Self::AppearanceWindowColor,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionPreventSleep => "session.prevent_sleep",
            Self::PowerActiveSchemeCheck => "power.active_scheme_check",
            Self::PowerActiveSchemeSwitch => "power.active_scheme_switch",
            Self::ExplorerShowExtensions => "explorer.show_extensions",
            Self::ExplorerShowHidden => "explorer.show_hidden",
            Self::ExplorerClockSeconds => "explorer.clock_seconds",
            Self::AppearanceTransparency => "appearance.transparency",
            Self::TaskbarTaskView => "taskbar.task_view",
            Self::TaskbarWidgets => "taskbar.widgets",
            Self::ExplorerItemCheckboxes => "explorer.item_checkboxes",
            Self::ExplorerCompactView => "explorer.compact_view",
            Self::ThemeColorMode => "theme.color_mode",
            Self::GamesProcessWatch => "games.process_watch",
            Self::GamesReadinessCheck => "games.readiness_check",
            Self::TaskbarSearchMode => "taskbar.search_mode",
            Self::TaskbarAlignment => "taskbar.alignment",
            Self::StartLayout => "start.layout",
            Self::StartRecommendations => "start.recommendations",
            Self::ExplorerLaunchTarget => "explorer.launch_target",
            Self::ExplorerRecentFiles => "explorer.recent_files",
            Self::TaskbarButtonGrouping => "taskbar.button_grouping",
            Self::TaskbarFlashing => "taskbar.flashing",
            Self::TaskbarShareWindow => "taskbar.share_window",
            Self::TaskbarShowDesktop => "taskbar.show_desktop",
            Self::SearchRecentOnHover => "search.recent_on_hover",
            Self::TaskbarMultiMonitor => "taskbar.multi_monitor",
            Self::TaskbarMultiMonitorMode => "taskbar.multi_monitor_mode",
            Self::TaskbarSecondaryButtonGrouping => "taskbar.secondary_button_grouping",
            Self::StartShowAllPins => "start.show_all_pins",
            Self::StartRecentApps => "start.recent_apps",
            Self::AppearanceAccentStartTaskbar => "appearance.accent_start_taskbar",
            Self::AppearanceAccentTitleBars => "appearance.accent_title_bars",
            Self::AppearanceAutoAccent => "appearance.auto_accent",
            Self::GamesGameMode => "games.game_mode",
            Self::GamesControllerGameBar => "games.controller_game_bar",
            Self::DevicesAutoplay => "devices.autoplay",
            Self::NotificationsUsbErrors => "notifications.usb_errors",
            Self::NotificationsWeakCharger => "notifications.weak_charger",
            Self::InputAutocorrect => "input.autocorrect",
            Self::InputDoubleSpacePeriod => "input.double_space_period",
            Self::InputAutoShift => "input.auto_shift",
            Self::InputVoiceTypingKey => "input.voice_typing_key",
            Self::InputMultilingualSuggestions => "input.multilingual_suggestions",
            Self::ExplorerStatusBar => "explorer.status_bar",
            Self::ExplorerInfoTips => "explorer.info_tips",
            Self::ExplorerHideEmptyDrives => "explorer.hide_empty_drives",
            Self::ExplorerNavExpandCurrent => "explorer.nav_expand_current",
            Self::ExplorerNavShowAll => "explorer.nav_show_all",
            Self::ExplorerSeparateProcess => "explorer.separate_process",
            Self::ExplorerIconsOnly => "explorer.icons_only",
            Self::ExplorerDriveLetters => "explorer.drive_letters",
            Self::ExplorerPreviewHandlers => "explorer.preview_handlers",
            Self::ExplorerSharingWizard => "explorer.sharing_wizard",
            Self::ExplorerAlwaysShowMenus => "explorer.always_show_menus",
            Self::AppearanceTaskbarAnimations => "appearance.taskbar_animations",
            Self::NotificationsToastBanners => "notifications.toast_banners",
            Self::SetupStartupInventory => "setup.startup_inventory",
            Self::StorageFreeSpaceCheck => "storage.free_space_check",
            Self::StorageTempFilesCheck => "storage.temp_files_check",
            Self::AppearanceAccentColorCheck => "appearance.accent_color_check",
            Self::AppearanceWindowColor => "appearance.window_color",
        }
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseActionIdError;

impl fmt::Display for ParseActionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown Action ID")
    }
}

impl std::error::Error for ParseActionIdError {}

impl FromStr for ActionId {
    type Err = ParseActionIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "session.prevent_sleep" => Ok(Self::SessionPreventSleep),
            "power.active_scheme_check" => Ok(Self::PowerActiveSchemeCheck),
            "power.active_scheme_switch" => Ok(Self::PowerActiveSchemeSwitch),
            "explorer.show_extensions" => Ok(Self::ExplorerShowExtensions),
            "explorer.show_hidden" => Ok(Self::ExplorerShowHidden),
            "explorer.clock_seconds" => Ok(Self::ExplorerClockSeconds),
            "appearance.transparency" => Ok(Self::AppearanceTransparency),
            "taskbar.task_view" => Ok(Self::TaskbarTaskView),
            "taskbar.widgets" => Ok(Self::TaskbarWidgets),
            "explorer.item_checkboxes" => Ok(Self::ExplorerItemCheckboxes),
            "explorer.compact_view" => Ok(Self::ExplorerCompactView),
            "theme.color_mode" => Ok(Self::ThemeColorMode),
            "games.process_watch" => Ok(Self::GamesProcessWatch),
            "games.readiness_check" => Ok(Self::GamesReadinessCheck),
            "taskbar.search_mode" => Ok(Self::TaskbarSearchMode),
            "taskbar.alignment" => Ok(Self::TaskbarAlignment),
            "start.layout" => Ok(Self::StartLayout),
            "start.recommendations" => Ok(Self::StartRecommendations),
            "explorer.launch_target" => Ok(Self::ExplorerLaunchTarget),
            "explorer.recent_files" => Ok(Self::ExplorerRecentFiles),
            "taskbar.button_grouping" => Ok(Self::TaskbarButtonGrouping),
            "taskbar.flashing" => Ok(Self::TaskbarFlashing),
            "taskbar.share_window" => Ok(Self::TaskbarShareWindow),
            "taskbar.show_desktop" => Ok(Self::TaskbarShowDesktop),
            "search.recent_on_hover" => Ok(Self::SearchRecentOnHover),
            "taskbar.multi_monitor" => Ok(Self::TaskbarMultiMonitor),
            "taskbar.multi_monitor_mode" => Ok(Self::TaskbarMultiMonitorMode),
            "taskbar.secondary_button_grouping" => {
                Ok(Self::TaskbarSecondaryButtonGrouping)
            }
            "start.show_all_pins" => Ok(Self::StartShowAllPins),
            "start.recent_apps" => Ok(Self::StartRecentApps),
            "appearance.accent_start_taskbar" => Ok(Self::AppearanceAccentStartTaskbar),
            "appearance.accent_title_bars" => Ok(Self::AppearanceAccentTitleBars),
            "appearance.auto_accent" => Ok(Self::AppearanceAutoAccent),
            "games.game_mode" => Ok(Self::GamesGameMode),
            "games.controller_game_bar" => Ok(Self::GamesControllerGameBar),
            "devices.autoplay" => Ok(Self::DevicesAutoplay),
            "notifications.usb_errors" => Ok(Self::NotificationsUsbErrors),
            "notifications.weak_charger" => Ok(Self::NotificationsWeakCharger),
            "input.autocorrect" => Ok(Self::InputAutocorrect),
            "input.double_space_period" => Ok(Self::InputDoubleSpacePeriod),
            "input.auto_shift" => Ok(Self::InputAutoShift),
            "input.voice_typing_key" => Ok(Self::InputVoiceTypingKey),
            "input.multilingual_suggestions" => Ok(Self::InputMultilingualSuggestions),
            "explorer.status_bar" => Ok(Self::ExplorerStatusBar),
            "explorer.info_tips" => Ok(Self::ExplorerInfoTips),
            "explorer.hide_empty_drives" => Ok(Self::ExplorerHideEmptyDrives),
            "explorer.nav_expand_current" => Ok(Self::ExplorerNavExpandCurrent),
            "explorer.nav_show_all" => Ok(Self::ExplorerNavShowAll),
            "explorer.separate_process" => Ok(Self::ExplorerSeparateProcess),
            "explorer.icons_only" => Ok(Self::ExplorerIconsOnly),
            "explorer.drive_letters" => Ok(Self::ExplorerDriveLetters),
            "explorer.preview_handlers" => Ok(Self::ExplorerPreviewHandlers),
            "explorer.sharing_wizard" => Ok(Self::ExplorerSharingWizard),
            "explorer.always_show_menus" => Ok(Self::ExplorerAlwaysShowMenus),
            "appearance.taskbar_animations" => Ok(Self::AppearanceTaskbarAnimations),
            "notifications.toast_banners" => Ok(Self::NotificationsToastBanners),
            "setup.startup_inventory" => Ok(Self::SetupStartupInventory),
            "storage.free_space_check" => Ok(Self::StorageFreeSpaceCheck),
            "storage.temp_files_check" => Ok(Self::StorageTempFilesCheck),
            "appearance.accent_color_check" => Ok(Self::AppearanceAccentColorCheck),
            "appearance.window_color" => Ok(Self::AppearanceWindowColor),
            _ => Err(ParseActionIdError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_identifier_round_trips() {
        for id in ActionId::ALL {
            assert_eq!(id.as_str().parse::<ActionId>(), Ok(id));
        }
    }
}
