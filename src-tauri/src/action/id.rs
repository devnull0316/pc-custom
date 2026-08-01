use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// Stable identifiers are never reused and are the only dispatch keys accepted by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// 耐久記録に残る表記。
///
/// `rename_all = "snake_case"` が効いているので、**`rename` を1つ書き忘れた版**は
/// `power_mode_switch` のような表記で記録を書いてしまう。
/// その記録を、`rename` を足した後の版は読めない。読めない backup は**戻せない**。
///
/// この書き忘れは実際に2回起きている。起きない仕組みにするより先に、
/// **起きても読める**ようにしておく。各 variant は点表記と snake 表記の両方を受ける。
pub enum ActionId {
    #[serde(rename = "session.prevent_sleep", alias = "session_prevent_sleep")]
    SessionPreventSleep,
    #[serde(rename = "session.default_printer", alias = "session_default_printer")]
    SessionDefaultPrinter,
    #[serde(rename = "session.temporary_vpn", alias = "session_temporary_vpn")]
    SessionTemporaryVpn,
    #[serde(
        rename = "input.shift_interruption_guard",
        alias = "input_shift_interruption_guard"
    )]
    InputShiftInterruptionGuard,
    #[serde(
        rename = "power.active_scheme_check",
        alias = "power_active_scheme_check"
    )]
    PowerActiveSchemeCheck,
    #[serde(
        rename = "power.active_scheme_switch",
        alias = "power_active_scheme_switch"
    )]
    PowerActiveSchemeSwitch,
    #[serde(rename = "power.mode_switch", alias = "power_mode_switch")]
    PowerModeSwitch,
    #[serde(rename = "input.pointer_feel", alias = "input_pointer_feel")]
    InputPointerFeel,
    #[serde(rename = "audio.comms_mic_mute", alias = "audio_comms_mic_mute")]
    AudioCommsMicMute,
    #[serde(
        rename = "explorer.show_extensions",
        alias = "explorer_show_extensions"
    )]
    ExplorerShowExtensions,
    #[serde(rename = "explorer.show_hidden", alias = "explorer_show_hidden")]
    ExplorerShowHidden,
    #[serde(rename = "explorer.clock_seconds", alias = "explorer_clock_seconds")]
    ExplorerClockSeconds,
    #[serde(rename = "appearance.transparency", alias = "appearance_transparency")]
    AppearanceTransparency,
    #[serde(
        rename = "appearance.high_contrast_trial",
        alias = "appearance_high_contrast_trial"
    )]
    AppearanceHighContrastTrial,
    #[serde(rename = "taskbar.task_view", alias = "taskbar_task_view")]
    TaskbarTaskView,
    #[serde(rename = "taskbar.widgets", alias = "taskbar_widgets")]
    TaskbarWidgets,
    #[serde(
        rename = "explorer.item_checkboxes",
        alias = "explorer_item_checkboxes"
    )]
    ExplorerItemCheckboxes,
    #[serde(rename = "explorer.compact_view", alias = "explorer_compact_view")]
    ExplorerCompactView,
    #[serde(rename = "theme.color_mode", alias = "theme_color_mode")]
    ThemeColorMode,
    #[serde(rename = "games.process_watch", alias = "games_process_watch")]
    GamesProcessWatch,
    #[serde(rename = "games.readiness_check", alias = "games_readiness_check")]
    GamesReadinessCheck,
    #[serde(rename = "taskbar.search_mode", alias = "taskbar_search_mode")]
    TaskbarSearchMode,
    #[serde(rename = "taskbar.alignment", alias = "taskbar_alignment")]
    TaskbarAlignment,
    #[serde(rename = "start.layout", alias = "start_layout")]
    StartLayout,
    #[serde(rename = "start.recommendations", alias = "start_recommendations")]
    StartRecommendations,
    #[serde(rename = "explorer.launch_target", alias = "explorer_launch_target")]
    ExplorerLaunchTarget,
    #[serde(rename = "explorer.recent_files", alias = "explorer_recent_files")]
    ExplorerRecentFiles,
    #[serde(rename = "taskbar.button_grouping", alias = "taskbar_button_grouping")]
    TaskbarButtonGrouping,
    #[serde(rename = "taskbar.flashing", alias = "taskbar_flashing")]
    TaskbarFlashing,
    #[serde(rename = "taskbar.share_window", alias = "taskbar_share_window")]
    TaskbarShareWindow,
    #[serde(rename = "taskbar.show_desktop", alias = "taskbar_show_desktop")]
    TaskbarShowDesktop,
    #[serde(rename = "search.recent_on_hover", alias = "search_recent_on_hover")]
    SearchRecentOnHover,
    #[serde(rename = "taskbar.multi_monitor", alias = "taskbar_multi_monitor")]
    TaskbarMultiMonitor,
    #[serde(
        rename = "taskbar.multi_monitor_mode",
        alias = "taskbar_multi_monitor_mode"
    )]
    TaskbarMultiMonitorMode,
    #[serde(
        rename = "taskbar.secondary_button_grouping",
        alias = "taskbar_secondary_button_grouping"
    )]
    TaskbarSecondaryButtonGrouping,
    #[serde(rename = "start.show_all_pins", alias = "start_show_all_pins")]
    StartShowAllPins,
    #[serde(rename = "start.recent_apps", alias = "start_recent_apps")]
    StartRecentApps,
    #[serde(
        rename = "appearance.accent_start_taskbar",
        alias = "appearance_accent_start_taskbar"
    )]
    AppearanceAccentStartTaskbar,
    #[serde(
        rename = "appearance.accent_title_bars",
        alias = "appearance_accent_title_bars"
    )]
    AppearanceAccentTitleBars,
    #[serde(rename = "appearance.auto_accent", alias = "appearance_auto_accent")]
    AppearanceAutoAccent,
    #[serde(rename = "games.game_mode", alias = "games_game_mode")]
    GamesGameMode,
    #[serde(
        rename = "games.controller_game_bar",
        alias = "games_controller_game_bar"
    )]
    GamesControllerGameBar,
    #[serde(rename = "devices.autoplay", alias = "devices_autoplay")]
    DevicesAutoplay,
    #[serde(
        rename = "notifications.usb_errors",
        alias = "notifications_usb_errors"
    )]
    NotificationsUsbErrors,
    #[serde(
        rename = "notifications.weak_charger",
        alias = "notifications_weak_charger"
    )]
    NotificationsWeakCharger,
    #[serde(rename = "input.autocorrect", alias = "input_autocorrect")]
    InputAutocorrect,
    #[serde(
        rename = "input.double_space_period",
        alias = "input_double_space_period"
    )]
    InputDoubleSpacePeriod,
    #[serde(rename = "input.auto_shift", alias = "input_auto_shift")]
    InputAutoShift,
    #[serde(rename = "input.voice_typing_key", alias = "input_voice_typing_key")]
    InputVoiceTypingKey,
    #[serde(
        rename = "input.multilingual_suggestions",
        alias = "input_multilingual_suggestions"
    )]
    InputMultilingualSuggestions,
    #[serde(rename = "explorer.status_bar", alias = "explorer_status_bar")]
    ExplorerStatusBar,
    #[serde(rename = "explorer.info_tips", alias = "explorer_info_tips")]
    ExplorerInfoTips,
    #[serde(
        rename = "explorer.hide_empty_drives",
        alias = "explorer_hide_empty_drives"
    )]
    ExplorerHideEmptyDrives,
    #[serde(
        rename = "explorer.nav_expand_current",
        alias = "explorer_nav_expand_current"
    )]
    ExplorerNavExpandCurrent,
    #[serde(rename = "explorer.nav_show_all", alias = "explorer_nav_show_all")]
    ExplorerNavShowAll,
    #[serde(
        rename = "explorer.separate_process",
        alias = "explorer_separate_process"
    )]
    ExplorerSeparateProcess,
    #[serde(rename = "explorer.icons_only", alias = "explorer_icons_only")]
    ExplorerIconsOnly,
    #[serde(rename = "explorer.drive_letters", alias = "explorer_drive_letters")]
    ExplorerDriveLetters,
    #[serde(
        rename = "explorer.preview_handlers",
        alias = "explorer_preview_handlers"
    )]
    ExplorerPreviewHandlers,
    #[serde(rename = "explorer.sharing_wizard", alias = "explorer_sharing_wizard")]
    ExplorerSharingWizard,
    #[serde(
        rename = "explorer.always_show_menus",
        alias = "explorer_always_show_menus"
    )]
    ExplorerAlwaysShowMenus,
    #[serde(
        rename = "appearance.taskbar_animations",
        alias = "appearance_taskbar_animations"
    )]
    AppearanceTaskbarAnimations,
    #[serde(
        rename = "notifications.toast_banners",
        alias = "notifications_toast_banners"
    )]
    NotificationsToastBanners,
    #[serde(rename = "setup.startup_inventory", alias = "setup_startup_inventory")]
    SetupStartupInventory,
    #[serde(
        rename = "storage.free_space_check",
        alias = "storage_free_space_check"
    )]
    StorageFreeSpaceCheck,
    #[serde(
        rename = "storage.temp_files_check",
        alias = "storage_temp_files_check"
    )]
    StorageTempFilesCheck,
    #[serde(
        rename = "appearance.accent_color_check",
        alias = "appearance_accent_color_check"
    )]
    AppearanceAccentColorCheck,
    #[serde(rename = "appearance.window_color", alias = "appearance_window_color")]
    AppearanceWindowColor,
    #[serde(rename = "setup.powertoys_status", alias = "setup_power_toys_status")]
    SetupPowerToysStatus,
    #[serde(rename = "setup.launch_apps", alias = "setup_launch_apps")]
    SetupLaunchApps,
    #[serde(
        rename = "setup.windows_update_status",
        alias = "setup_windows_update_status"
    )]
    SetupWindowsUpdateStatus,
    #[serde(rename = "setup.default_apps", alias = "setup_default_apps")]
    SetupDefaultApps,
    #[serde(rename = "setup.window_layout", alias = "setup_window_layout")]
    SetupWindowLayout,
    #[serde(rename = "setup.audio_output", alias = "setup_audio_output")]
    SetupAudioOutput,
}

impl ActionId {
    pub const ALL: [Self; 74] = [
        Self::SessionPreventSleep,
        Self::SessionDefaultPrinter,
        Self::SessionTemporaryVpn,
        Self::InputShiftInterruptionGuard,
        Self::PowerActiveSchemeCheck,
        Self::PowerActiveSchemeSwitch,
        Self::PowerModeSwitch,
        Self::InputPointerFeel,
        Self::AudioCommsMicMute,
        Self::ExplorerShowExtensions,
        Self::ExplorerShowHidden,
        Self::ExplorerClockSeconds,
        Self::AppearanceTransparency,
        Self::AppearanceHighContrastTrial,
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
        Self::SetupPowerToysStatus,
        Self::SetupLaunchApps,
        Self::SetupWindowsUpdateStatus,
        Self::SetupDefaultApps,
        Self::SetupWindowLayout,
        Self::SetupAudioOutput,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionPreventSleep => "session.prevent_sleep",
            Self::SessionDefaultPrinter => "session.default_printer",
            Self::SessionTemporaryVpn => "session.temporary_vpn",
            Self::InputShiftInterruptionGuard => "input.shift_interruption_guard",
            Self::PowerActiveSchemeCheck => "power.active_scheme_check",
            Self::PowerActiveSchemeSwitch => "power.active_scheme_switch",
            Self::PowerModeSwitch => "power.mode_switch",
            Self::InputPointerFeel => "input.pointer_feel",
            Self::AudioCommsMicMute => "audio.comms_mic_mute",
            Self::ExplorerShowExtensions => "explorer.show_extensions",
            Self::ExplorerShowHidden => "explorer.show_hidden",
            Self::ExplorerClockSeconds => "explorer.clock_seconds",
            Self::AppearanceTransparency => "appearance.transparency",
            Self::AppearanceHighContrastTrial => "appearance.high_contrast_trial",
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
            Self::SetupPowerToysStatus => "setup.powertoys_status",
            Self::SetupLaunchApps => "setup.launch_apps",
            Self::SetupWindowsUpdateStatus => "setup.windows_update_status",
            Self::SetupDefaultApps => "setup.default_apps",
            Self::SetupWindowLayout => "setup.window_layout",
            Self::SetupAudioOutput => "setup.audio_output",
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
            "session.default_printer" => Ok(Self::SessionDefaultPrinter),
            "session.temporary_vpn" => Ok(Self::SessionTemporaryVpn),
            "input.shift_interruption_guard" => Ok(Self::InputShiftInterruptionGuard),
            "power.active_scheme_check" => Ok(Self::PowerActiveSchemeCheck),
            "power.active_scheme_switch" => Ok(Self::PowerActiveSchemeSwitch),
            "power.mode_switch" => Ok(Self::PowerModeSwitch),
            "input.pointer_feel" => Ok(Self::InputPointerFeel),
            "audio.comms_mic_mute" => Ok(Self::AudioCommsMicMute),
            "explorer.show_extensions" => Ok(Self::ExplorerShowExtensions),
            "explorer.show_hidden" => Ok(Self::ExplorerShowHidden),
            "explorer.clock_seconds" => Ok(Self::ExplorerClockSeconds),
            "appearance.transparency" => Ok(Self::AppearanceTransparency),
            "appearance.high_contrast_trial" => Ok(Self::AppearanceHighContrastTrial),
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
            "taskbar.secondary_button_grouping" => Ok(Self::TaskbarSecondaryButtonGrouping),
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
            "setup.powertoys_status" => Ok(Self::SetupPowerToysStatus),
            "setup.launch_apps" => Ok(Self::SetupLaunchApps),
            "setup.windows_update_status" => Ok(Self::SetupWindowsUpdateStatus),
            "setup.default_apps" => Ok(Self::SetupDefaultApps),
            "setup.window_layout" => Ok(Self::SetupWindowLayout),
            "setup.audio_output" => Ok(Self::SetupAudioOutput),
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
            let serialized = serde_json::to_value(id).expect("serialize action ID");
            assert_eq!(serialized, id.as_str());
            assert_eq!(
                serde_json::from_value::<ActionId>(serialized).expect("deserialize action ID"),
                id
            );
        }
    }

    #[test]
    fn shipped_underscore_names_remain_readable_for_existing_backups() {
        assert_eq!(
            serde_json::from_str::<ActionId>(r#""power_mode_switch""#)
                .expect("read legacy power mode ID"),
            ActionId::PowerModeSwitch
        );
        assert_eq!(
            serde_json::from_str::<ActionId>(r#""input_pointer_feel""#)
                .expect("read legacy pointer feel ID"),
            ActionId::InputPointerFeel
        );
    }
}

#[cfg(test)]
mod durable_id_compatibility {
    use super::*;

    /// `rename` を書き忘れた版が書いた記録も読めること。
    ///
    /// `rename_all = "snake_case"` が効いているので、書き忘れた版は
    /// `power_mode_switch` のような表記で耐久記録を書く。
    /// それを読めないと、**その変更は永久に戻せなくなる。**
    ///
    /// 書き忘れは実際に2回起きている。起きた後でも読めることを固定する。
    #[test]
    fn every_action_id_also_parses_from_the_snake_case_form() {
        let mut checked = 0usize;
        for id in ActionId::ALL {
            let dotted = id.as_str();
            // serde の `rename_all = "snake_case"` は **variant 名**を分解する。
            // 点表記を `_` に置き換えたものとは限らない。
            // 例: `SetupPowerToysStatus` は `setup_power_toys_status` になる。
            // 点表記由来の `setup_powertoys_status` は、どの版も書いたことがない表記。
            let snake = match dotted {
                "setup.powertoys_status" => "setup_power_toys_status".to_owned(),
                other => other.replace('.', "_"),
            };
            assert_ne!(dotted, snake, "点を含まない ID がある: {dotted}");

            let from_dotted: ActionId =
                serde_json::from_str(&format!("\"{dotted}\"")).expect("点表記を読めること");
            let from_snake: ActionId = serde_json::from_str(&format!("\"{snake}\""))
                .unwrap_or_else(|error| {
                    panic!("旧表記 {snake} を読めない。この Action の記録は戻せなくなる: {error}")
                });
            assert_eq!(from_dotted, id);
            assert_eq!(from_snake, id, "{snake} が別の Action として読まれた");
            checked += 1;
        }
        // 1件も見ていないなら合格ではない。
        assert!(checked > 0, "ActionId を1件も確認していない");
        assert_eq!(checked, ActionId::ALL.len());
    }

    /// 書き出す表記は点のまま。alias を足したことで、
    /// 新しく書く記録まで snake になっては困る。
    #[test]
    fn what_gets_written_is_still_the_dotted_form() {
        for id in ActionId::ALL {
            let written = serde_json::to_string(&id).expect("serialize");
            assert_eq!(
                written,
                format!("\"{}\"", id.as_str()),
                "書き出す表記が変わっている"
            );
            assert!(written.contains('.'), "点表記でなくなっている: {written}");
        }
    }
}
