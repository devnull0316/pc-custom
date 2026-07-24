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
    #[serde(rename = "theme.color_mode")]
    ThemeColorMode,
    #[serde(rename = "games.process_watch")]
    GamesProcessWatch,
}

impl ActionId {
    pub const ALL: [Self; 6] = [
        Self::SessionPreventSleep,
        Self::PowerActiveSchemeCheck,
        Self::ExplorerShowExtensions,
        Self::ExplorerShowHidden,
        Self::ThemeColorMode,
        Self::GamesProcessWatch,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionPreventSleep => "session.prevent_sleep",
            Self::PowerActiveSchemeCheck => "power.active_scheme_check",
            Self::ExplorerShowExtensions => "explorer.show_extensions",
            Self::ExplorerShowHidden => "explorer.show_hidden",
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
