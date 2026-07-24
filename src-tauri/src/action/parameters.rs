use serde::{Deserialize, Serialize};

use super::ActionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColorMode {
    Light,
    Dark,
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
    #[serde(rename = "power.active_scheme_check")]
    PowerActiveSchemeCheck {},
    #[serde(rename = "explorer.show_extensions")]
    ExplorerShowExtensions { show: bool },
    #[serde(rename = "explorer.show_hidden")]
    ExplorerShowHidden { show: bool },
    #[serde(rename = "theme.color_mode")]
    ThemeColorMode { mode: ThemeColorMode },
    #[serde(rename = "games.process_watch")]
    GamesProcessWatch { binding: ProcessBindingParameters },
}

impl ActionParameters {
    pub const fn action_id(&self) -> ActionId {
        match self {
            Self::SessionPreventSleep { .. } => ActionId::SessionPreventSleep,
            Self::PowerActiveSchemeCheck { .. } => ActionId::PowerActiveSchemeCheck,
            Self::ExplorerShowExtensions { .. } => ActionId::ExplorerShowExtensions,
            Self::ExplorerShowHidden { .. } => ActionId::ExplorerShowHidden,
            Self::ThemeColorMode { .. } => ActionId::ThemeColorMode,
            Self::GamesProcessWatch { .. } => ActionId::GamesProcessWatch,
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
