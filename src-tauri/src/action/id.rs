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
}

impl ActionId {
    pub const ALL: [Self; 12] = [
        Self::SessionPreventSleep,
        Self::PowerActiveSchemeCheck,
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
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionPreventSleep => "session.prevent_sleep",
            Self::PowerActiveSchemeCheck => "power.active_scheme_check",
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
