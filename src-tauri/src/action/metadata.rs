use serde::{Deserialize, Serialize};

use super::ActionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRiskLevel {
    Safe,
    Caution,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Persistent,
    Session,
    Observation,
    Guided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodClass {
    PublicApi,
    MicrosoftCli,
    WinGet,
    OfficialModule,
    DocumentedRegistry,
    LimitedExternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsReleaseFamily {
    Windows11_24H2,
    Windows11_25H2,
    Windows11_26H1,
}

/// Immutable, first-party metadata compiled into the binary.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct ActionMetadata {
    pub id: ActionId,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub tags: &'static [&'static str],
    pub supportedWindowsVersions: &'static [WindowsReleaseFamily],
    pub minimumBuild: u32,
    pub maximumTestedBuild: u32,
    pub riskLevel: ActionRiskLevel,
    pub requiresAdmin: bool,
    pub requiresRestart: bool,
    pub requiresExplorerRestart: bool,
    pub conflicts: &'static [ActionId],
    pub dependencies: &'static [ActionId],
    pub action_version: u32,
    pub kind: ActionKind,
    pub parameter_schema: &'static str,
    pub resource_keys: &'static [&'static str],
    pub method_class: MethodClass,
    pub evidence_urls: &'static [&'static str],
    pub compatibility_key: &'static str,
    pub backup_codec_version: u32,
    pub rollback_decoder_versions: &'static [u32],
    pub auto_apply_eligible: bool,
    pub windows_update_impact: &'static str,
}

impl ActionMetadata {
    pub fn validate_static_contract(&self) -> Result<(), &'static str> {
        if self.id.as_str().is_empty() || self.name.is_empty() || self.description.is_empty() {
            return Err("Action metadata contains an empty required field");
        }
        if self.action_version == 0 || self.backup_codec_version == 0 {
            return Err("Action and backup codec versions must be non-zero");
        }
        if !self
            .rollback_decoder_versions
            .contains(&self.backup_codec_version)
        {
            return Err("current backup codec has no rollback decoder");
        }
        if self.requiresExplorerRestart {
            return Err("stable MVP Actions may not force-restart Explorer");
        }
        Ok(())
    }
}
